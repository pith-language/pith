mod graph;

use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::sync::Arc;

use pith_core::{Action, BodyRevision, Coordinate, DeclarationTable, Interface, Pure, Rule};
use pith_diag::{ByteOffset, Diag, Severity, SourceFile, SourceId};
use pith_elaborator::{RuleSignature, abi_digest, elaborate, scope_imports};
use pith_engine::{ActionRule, Engine, PureRule};
use pith_hir::{ModuleFiles, ParsedSurface, SurfaceBody};
use pith_ids::{ContentId, ModuleAbiDigest};

pub use graph::{
    ELABORATOR_SEMANTIC_VERSION, FrontendImport, FrontendImportEnv, FrontendInputError,
    FrontendSource, InterfaceSurface, RegisterFrontend, bodies_of_request, index_of_request,
    interface_of_request,
};
pub use pith_elaborator::{GRAMMAR_VERSION, ImportedModule};
pub use pith_hir::{
    DefinitionKind, DefinitionLocation, FrontendCode, PositionSidecar, ReferenceSite, RuleCategory,
};

pub struct ModuleSource {
    pub module: Box<str>,
    pub source_id: SourceId,
    pub label: Box<str>,
    pub text: Box<str>,
}

impl ModuleSource {
    #[must_use]
    pub fn new(
        module: impl Into<Box<str>>,
        source_id: SourceId,
        label: impl Into<Box<str>>,
        text: impl Into<Box<str>>,
    ) -> Self {
        Self {
            module: module.into(),
            source_id,
            label: label.into(),
            text: text.into(),
        }
    }
}

pub struct ParsedModule {
    module: Box<str>,
    artifact_id: ContentId,
    source: Arc<SourceFile>,
    surface: ParsedSurface,
    diagnostics: Vec<Diag>,
    positions: PositionSidecar,
}

impl ParsedModule {
    #[must_use]
    pub fn module(&self) -> &str {
        &self.module
    }

    #[must_use]
    pub const fn artifact_id(&self) -> ContentId {
        self.artifact_id
    }

    #[must_use]
    pub fn source(&self) -> &Arc<SourceFile> {
        &self.source
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[Diag] {
        &self.diagnostics
    }

    #[must_use]
    pub fn positions(&self) -> &PositionSidecar {
        &self.positions
    }
}

#[must_use]
pub fn parse_module(source: &ModuleSource) -> ParsedModule {
    let source_file = Arc::new(SourceFile::new(
        source.source_id,
        source.label.clone(),
        source.text.clone(),
    ));
    let artifact_id = ContentId::of_blob(source_file.source_text().as_bytes());
    let (surface, diagnostics) = pith_syntax::parse(&source_file);
    let definitions = definition_locations(&source.module, &surface, &source_file);
    ParsedModule {
        module: source.module.clone(),
        artifact_id,
        source: source_file,
        surface,
        diagnostics,
        positions: PositionSidecar::new(definitions, Vec::new()),
    }
}

#[derive(Default)]
pub struct ImportEnv {
    inner: pith_elaborator::ImportEnv,
}

impl ImportEnv {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_loaded(&mut self, loaded: &LoadedModule) {
        self.inner.insert(
            loaded.module.clone(),
            loaded.abi_digest,
            loaded.table.clone(),
            loaded.positions.definitions(),
        );
    }

    pub(crate) fn insert_surface(&mut self, binding: &str, surface: &InterfaceSurface) {
        self.inner.insert(
            binding,
            surface.abi_digest(),
            surface.table.clone(),
            Box::<[DefinitionLocation]>::default(),
        );
    }

    #[must_use]
    pub fn get(&self, module: &str) -> Option<&ImportedModule> {
        self.inner.get(module)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

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
        match rule.category {
            RuleCategory::Pure => pure_rules.push(HostRuleDeclaration::new(
                &module,
                rule.label,
                rule.interface,
                rule.span,
            )),
            RuleCategory::Action => action_rules.push(HostRuleDeclaration::new(
                &module,
                rule.label,
                rule.interface,
                rule.span,
            )),
        }
    }
    let visible_imports = scoped
        .iter()
        .map(|(name, imported)| (Box::from(name), imported.definitions().into()))
        .collect();
    Ok(LoadedModule {
        module,
        artifact_id,
        source,
        table: elaborated.table,
        imports: ordered_imports.into(),
        pure_rules: pure_rules.into(),
        action_rules: action_rules.into(),
        abi_digest,
        positions: PositionSidecar::new(positions.definitions().to_vec(), elaborated.references),
        visible_imports,
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

pub struct HostRuleDeclaration<K> {
    coordinate: Coordinate,
    interface: Interface,
    span: pith_diag::Span,
    effect: PhantomData<fn() -> K>,
}

impl<K> HostRuleDeclaration<K> {
    fn new(module: &str, label: Box<str>, interface: Interface, span: pith_diag::Span) -> Self {
        Self {
            coordinate: Coordinate::new(module, label),
            interface,
            span,
            effect: PhantomData,
        }
    }

    #[must_use]
    pub fn coordinate(&self) -> &Coordinate {
        &self.coordinate
    }

    #[must_use]
    pub fn interface(&self) -> &Interface {
        &self.interface
    }

    #[must_use]
    pub const fn span(&self) -> pith_diag::Span {
        self.span
    }
}

impl HostRuleDeclaration<Pure> {
    #[must_use]
    pub fn rule(&self, body_revision: BodyRevision) -> Rule<Pure> {
        Rule::declared(
            &self.coordinate.module,
            &self.coordinate.name,
            body_revision,
            self.interface.clone(),
            self.span,
        )
    }

    pub fn bind<B>(
        &self,
        engine: &mut Engine,
        body_revision: BodyRevision,
        body: B,
    ) -> pith_core::RuleId
    where
        B: PureRule + 'static,
    {
        engine.register_rule(self.rule(body_revision), body)
    }
}

impl HostRuleDeclaration<Action> {
    #[must_use]
    pub fn rule(&self, body_revision: BodyRevision) -> Rule<Action> {
        Rule::declared(
            &self.coordinate.module,
            &self.coordinate.name,
            body_revision,
            self.interface.clone(),
            self.span,
        )
    }

    pub fn bind<B>(
        &self,
        engine: &mut Engine,
        body_revision: BodyRevision,
        body: B,
    ) -> pith_core::RuleId
    where
        B: ActionRule + 'static,
    {
        engine.register_action_rule(self.rule(body_revision), body)
    }
}

pub struct LoadedModule {
    module: Box<str>,
    artifact_id: ContentId,
    source: Arc<SourceFile>,
    table: DeclarationTable,
    imports: Box<[(Box<str>, ModuleAbiDigest)]>,
    pure_rules: Box<[HostRuleDeclaration<Pure>]>,
    action_rules: Box<[HostRuleDeclaration<Action>]>,
    abi_digest: ModuleAbiDigest,
    positions: PositionSidecar,
    visible_imports: BTreeMap<Box<str>, Box<[DefinitionLocation]>>,
}

impl LoadedModule {
    #[must_use]
    pub fn module(&self) -> &str {
        &self.module
    }

    #[must_use]
    pub const fn artifact_id(&self) -> ContentId {
        self.artifact_id
    }

    #[must_use]
    pub fn source(&self) -> &Arc<SourceFile> {
        &self.source
    }

    #[must_use]
    pub fn table(&self) -> &DeclarationTable {
        &self.table
    }

    #[must_use]
    pub fn imports(&self) -> &[(Box<str>, ModuleAbiDigest)] {
        &self.imports
    }

    #[must_use]
    pub fn pure_rules(&self) -> &[HostRuleDeclaration<Pure>] {
        &self.pure_rules
    }

    #[must_use]
    pub fn action_rules(&self) -> &[HostRuleDeclaration<Action>] {
        &self.action_rules
    }

    #[must_use]
    pub fn pure_rule(&self, label: &str) -> Option<&HostRuleDeclaration<Pure>> {
        self.pure_rules
            .iter()
            .find(|rule| rule.coordinate.name.as_ref() == label)
    }

    #[must_use]
    pub fn action_rule(&self, label: &str) -> Option<&HostRuleDeclaration<Action>> {
        self.action_rules
            .iter()
            .find(|rule| rule.coordinate.name.as_ref() == label)
    }

    #[must_use]
    pub const fn abi_digest(&self) -> ModuleAbiDigest {
        self.abi_digest
    }

    #[must_use]
    pub fn interface_surface(&self) -> InterfaceSurface {
        InterfaceSurface::of_module(self)
    }

    #[must_use]
    pub fn positions(&self) -> &PositionSidecar {
        &self.positions
    }

    #[must_use]
    pub fn go_to_definition(&self, offset: ByteOffset) -> Option<&DefinitionLocation> {
        self.positions.definition_at(offset)
    }

    #[must_use]
    pub fn completions(&self, module: Option<&str>) -> Vec<&DefinitionLocation> {
        match module {
            None => self.positions.definitions().iter().collect(),
            Some(module) if module == self.module.as_ref() => {
                self.positions.definitions().iter().collect()
            }
            Some(module) => self
                .visible_imports
                .get(module)
                .map_or_else(Vec::new, |definitions| definitions.iter().collect()),
        }
    }
}

fn definition_locations(
    module: &str,
    surface: &ParsedSurface,
    source: &Arc<SourceFile>,
) -> Vec<DefinitionLocation> {
    let declaration_definitions = surface.declarations.iter().map(|declaration| {
        let kind = match declaration.body {
            SurfaceBody::Nominal(_) => DefinitionKind::Nominal,
            SurfaceBody::Sum(_) => DefinitionKind::Sum,
            SurfaceBody::Alias(_) => DefinitionKind::Alias,
        };
        DefinitionLocation::new(
            Coordinate::new(module, declaration.name.clone()),
            kind,
            source.clone(),
            declaration.name_span,
            declaration.documentation.clone(),
        )
    });
    let rule_definitions = surface.rules.iter().map(|rule| {
        DefinitionLocation::new(
            Coordinate::new(module, rule.label.clone()),
            DefinitionKind::HostRule(rule.category),
            source.clone(),
            rule.label_span,
            rule.documentation.clone(),
        )
    });
    declaration_definitions.chain(rule_definitions).collect()
}

fn declaration_definitions(positions: &PositionSidecar) -> BTreeMap<Box<str>, DefinitionLocation> {
    positions
        .definitions()
        .iter()
        .filter(|definition| !matches!(definition.kind(), DefinitionKind::HostRule(_)))
        .map(|definition| (definition.coordinate().name.clone(), definition.clone()))
        .collect()
}
