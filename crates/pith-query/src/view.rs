//! The query views over a loaded module: what `explore` reports about
//! declarations, rules, entries, and metadata, and how diagnostics render.

use std::collections::BTreeMap;
use std::path::Path;

use pith_diag::Diag;
use pith_loader::{DefinitionKind, DefinitionLocation, LoadedModule};
use pith_output::dto::{
    AboutValueRepr, AboutView, DeclarationView, DiagnosticRepr, EntryView, ImportView, ModuleView,
    RuleCategoryRepr, RuleView, SeverityRepr, TierRepr,
};

/// What the loaded root module declares, as `explore` reports it.
pub(crate) fn module_view(path: &Path, loaded: &LoadedModule) -> ModuleView {
    let definitions = definition_index(loaded);
    let entry_definitions = entry_definition_index(loaded);
    ModuleView {
        module: loaded.module().into(),
        path: path.display().to_string().into(),
        abi_digest: loaded.abi_digest().digest().to_string().into(),
        imports: loaded
            .imports()
            .iter()
            .map(|(module, abi)| ImportView {
                module: module.clone(),
                abi_digest: abi.digest().to_string().into(),
            })
            .collect(),
        declarations: loaded
            .table()
            .iter()
            .map(|declaration| {
                let mut view = DeclarationView::from(declaration);
                view.documentation = documentation(&definitions, &view.name).into();
                view
            })
            .collect(),
        rules: rule_views(loaded, &definitions),
        entries: loaded
            .entries()
            .iter()
            .map(|entry| EntryView {
                name: entry.name().into(),
                coordinate: format!("{}::{}", loaded.module(), entry.rule_label()).into(),
                tier: TierRepr::Represented,
                interface: entry.interface().into(),
                documentation: documentation(&entry_definitions, entry.name()).into(),
            })
            .collect(),
        about: loaded
            .about()
            .iter()
            .map(|about| AboutView {
                fields: about
                    .fields
                    .iter()
                    .map(|(name, value)| {
                        let value = match value {
                            pith_hir::SurfaceAboutValue::Text(text) => {
                                AboutValueRepr::Text { text: text.clone() }
                            }
                            pith_hir::SurfaceAboutValue::List(elements) => AboutValueRepr::List {
                                elements: elements.clone(),
                            },
                        };
                        (name.clone(), value)
                    })
                    .collect(),
                documentation: span_documentation(loaded, &about.documentation).into(),
            })
            .collect(),
    }
}

fn rule_views(
    loaded: &LoadedModule,
    definitions: &BTreeMap<&str, &DefinitionLocation>,
) -> Box<[RuleView]> {
    let pure = loaded.pure_rules().iter().map(|rule| {
        (
            RuleCategoryRepr::Pure,
            rule.coordinate(),
            rule.interface(),
            rule.is_represented(),
        )
    });
    let action = loaded.action_rules().iter().map(|rule| {
        (
            RuleCategoryRepr::Action,
            rule.coordinate(),
            rule.interface(),
            rule.is_represented(),
        )
    });
    pure.chain(action)
        .map(|(category, coordinate, interface, represented)| RuleView {
            label: coordinate.name.clone(),
            category,
            tier: if represented {
                TierRepr::Represented
            } else {
                TierRepr::Host
            },
            interface: interface.into(),
            documentation: documentation(definitions, &coordinate.name).into(),
        })
        .collect()
}

fn definition_index(loaded: &LoadedModule) -> BTreeMap<&str, &DefinitionLocation> {
    let mut definitions = BTreeMap::new();
    for definition in loaded.positions().definitions().iter() {
        definitions
            .entry(definition.coordinate().name.as_ref())
            .or_insert(definition);
    }
    definitions
}

fn entry_definition_index(loaded: &LoadedModule) -> BTreeMap<&str, &DefinitionLocation> {
    loaded
        .positions()
        .definitions()
        .iter()
        .filter(|definition| matches!(definition.kind(), DefinitionKind::Entry))
        .map(|definition| (definition.coordinate().name.as_ref(), definition))
        .collect()
}

fn documentation(definitions: &BTreeMap<&str, &DefinitionLocation>, name: &str) -> String {
    definitions
        .get(name)
        .map_or_else(String::new, |definition| definition.documentation())
}

fn span_documentation(loaded: &LoadedModule, spans: &[pith_diag::Span]) -> String {
    spans
        .iter()
        .filter_map(|span| {
            let (source, local) = loaded.files().file_of(*span);
            let text = source.source_text();
            let start = usize::try_from(local.start.0).ok()?;
            let end = usize::try_from(local.end.0).ok()?;
            text.get(start..end)
        })
        .map(|line| line.strip_prefix("--").unwrap_or(line).trim())
        .collect::<Vec<_>>()
        .join("\n")
}

/// One diagnostic as a query report carries it.
pub(crate) fn diagnostic(diagnostic: &Diag) -> DiagnosticRepr {
    let position = diagnostic
        .source
        .as_ref()
        .map(|source| (source.label.clone(), source.line_col(diagnostic.span.start)));
    let (label, line, column) = match position {
        Some((label, (line, column))) => (
            Some(label),
            Some(u64::try_from(line).unwrap_or(u64::MAX)),
            Some(u64::try_from(column).unwrap_or(u64::MAX)),
        ),
        None => (None, None, None),
    };
    DiagnosticRepr {
        severity: severity(diagnostic.severity),
        code: diagnostic.code.0,
        label,
        line,
        column,
        message: diagnostic.message.0.clone(),
    }
}

fn severity(severity: pith_diag::Severity) -> SeverityRepr {
    match severity {
        pith_diag::Severity::Error => SeverityRepr::Error,
        pith_diag::Severity::Warning => SeverityRepr::Warning,
        pith_diag::Severity::Info => SeverityRepr::Info,
        pith_diag::Severity::Note => SeverityRepr::Note,
    }
}
