//! The load pipeline: parse, scope imports, elaborate, and derive the ABI.

use pith_core::{Action, Pure};
use pith_diag::{Diag, Severity};
use pith_elaborator::{RuleSignature, Visibility, abi_digest, elaborate, scope_imports};
use pith_hir::{ModuleFiles, PositionSidecar, RuleCategory};

use crate::bind::{
    DeclarationMetadata, EntryDeclaration, HostRuleDeclaration, RepresentedRuleDeclaration,
    RuleDeclaration,
};
use crate::import::ImportEnv;
use crate::loaded::{LoadedModule, declaration_definitions};
use crate::source::{ModuleSource, ParsedModule, parse_module};

/// # Errors
///
/// Returns every parse, import, and elaboration error attached to its source.
pub fn elaborate_module(
    parsed: ParsedModule,
    imports: &ImportEnv,
) -> Result<LoadedModule, Box<[Diag]>> {
    let ParsedModule {
        module,
        artifact_id,
        source,
        surface,
        mut diagnostics,
        positions,
    } = parsed;
    let files = ModuleFiles::one(&source);
    let scoped = scope_imports(&surface, &imports.inner, &files, &mut diagnostics);
    let definitions = declaration_definitions(&positions);
    let elaborated = elaborate(
        &module,
        &surface,
        &scoped,
        &definitions,
        &files,
        &mut diagnostics,
    );
    let ordered_imports = scoped
        .iter()
        .map(|(name, imported)| (Box::from(name), imported.abi_digest()))
        .collect::<Vec<_>>();
    let abi_signatures = elaborated
        .rules
        .iter()
        .filter(|rule| rule.visibility == Visibility::Public)
        .map(|rule| RuleSignature {
            category: rule.category,
            interface: rule.interface.clone(),
        })
        .collect::<Vec<_>>();
    let abi_digest = abi_digest(
        &module,
        &elaborated.table,
        &ordered_imports,
        &abi_signatures,
    );
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(diagnostics.into());
    }

    let mut pure_rules = Vec::new();
    let mut action_rules = Vec::new();
    for rule in elaborated.rules {
        let pith_elaborator::ElaboratedRule {
            label,
            category,
            interface,
            span,
            body,
            visibility,
        } = rule;
        match (category, body) {
            (RuleCategory::Pure, None) => {
                let metadata =
                    DeclarationMetadata::<Pure>::new(&module, label, interface, span, visibility);
                pure_rules.push(RuleDeclaration::Host(HostRuleDeclaration::new(metadata)));
            }
            (RuleCategory::Pure, Some(body)) => {
                let metadata =
                    DeclarationMetadata::<Pure>::new(&module, label, interface, span, visibility);
                pure_rules.push(RuleDeclaration::Represented(
                    RepresentedRuleDeclaration::new(metadata, body),
                ));
            }
            (RuleCategory::Action, None) => {
                let metadata =
                    DeclarationMetadata::<Action>::new(&module, label, interface, span, visibility);
                action_rules.push(RuleDeclaration::Host(HostRuleDeclaration::new(metadata)));
            }
            (RuleCategory::Action, Some(body)) => {
                let metadata =
                    DeclarationMetadata::<Action>::new(&module, label, interface, span, visibility);
                action_rules.push(RuleDeclaration::Represented(
                    RepresentedRuleDeclaration::new(metadata, body),
                ));
            }
        }
    }
    let visible_imports = scoped
        .iter()
        .map(|(name, imported)| (Box::from(name), imported.definitions().into()))
        .collect();
    let entries = elaborated
        .entries
        .into_iter()
        .map(|entry| {
            EntryDeclaration::new(&module, entry.name, entry.interface, entry.span, entry.body)
        })
        .collect();
    Ok(LoadedModule {
        module,
        artifact_id,
        source,
        diagnostics: diagnostics.into(),
        table: elaborated.table,
        imports: ordered_imports.into(),
        pure_rules: pure_rules.into(),
        action_rules: action_rules.into(),
        abi_digest,
        positions: PositionSidecar::new(positions.definitions().to_vec(), elaborated.references),
        visible_imports,
        entries,
        about: surface.about.clone(),
    })
}

/// # Errors
///
/// Returns every parse, import, and elaboration error attached to its source.
pub fn load_module(
    source: &ModuleSource,
    imports: &ImportEnv,
) -> Result<LoadedModule, Box<[Diag]>> {
    elaborate_module(parse_module(source), imports)
}
