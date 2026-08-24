mod abi;
mod elaborate;
mod graph;
mod lex;
mod merge;
mod parse;
mod position;
mod surface;

use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::sync::Arc;

use pith_core::{Action, BodyRevision, Coordinate, DeclarationTable, Interface, Pure, Rule};
use pith_diag::{ByteOffset, Diag, Severity, SourceFile, SourceId, StableCode};
use pith_engine::{ActionRule, Engine, PureRule};
use pith_ids::{ContentId, ModuleAbiDigest};

pub use abi::{GRAMMAR_VERSION, RuleCategory};
pub use graph::{
    ELABORATOR_SEMANTIC_VERSION, FrontendImport, FrontendImportEnv, FrontendInputError,
    FrontendSource, InterfaceSurface, RegisterFrontend, bodies_of_request, index_of_request,
    interface_of_request,
};
pub use position::{DefinitionKind, DefinitionLocation, PositionSidecar, ReferenceSite};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FrontendCode {
    UnexpectedToken = 1,
    InvalidString = 2,
    DuplicateDeclaration = 3,
    DuplicateField = 4,
    RecursiveAlias = 5,
    CyclicDeclaration = 6,
    UnknownName = 7,
    UnknownImport = 8,
    MissingRule = 9,
    DuplicateImport = 10,
    DuplicateRule = 11,
    DuplicateInterface = 12,
    UndeclaredQualifiedAccess = 13,
    SourceNotUtf8 = 14,
    MalformedSurface = 15,
}

impl FrontendCode {
    #[must_use]
    pub const fn stable(self) -> StableCode {
        StableCode::frontend(self as u32)
    }
}

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
    surface: surface::ParsedSurface,
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
    let (surface, diagnostics) = parse::parse(&source_file);
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

#[derive(Clone)]
pub struct ImportedModule {
    abi_digest: ModuleAbiDigest,
    table: DeclarationTable,
    definitions: Box<[DefinitionLocation]>,
}

impl ImportedModule {
    #[must_use]
    pub fn of(loaded: &LoadedModule) -> Self {
        Self {
            abi_digest: loaded.abi_digest,
            table: loaded.table.clone(),
            definitions: loaded.positions.definitions().into(),
        }
    }

    fn declaration_definition(&self, name: &str) -> Option<&DefinitionLocation> {
        self.definitions.iter().find(|definition| {
            definition.coordinate().name.as_ref() == name
                && !matches!(definition.kind(), DefinitionKind::HostRule(_))
        })
    }
}

#[derive(Default)]
pub struct ImportEnv {
    modules: BTreeMap<Box<str>, ImportedModule>,
}

impl ImportEnv {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_loaded(&mut self, loaded: &LoadedModule) {
        self.modules
            .insert(loaded.module.clone(), ImportedModule::of(loaded));
    }

    pub(crate) fn insert_surface(&mut self, binding: &str, surface: &InterfaceSurface) {
        self.modules.insert(
            binding.into(),
            ImportedModule {
                abi_digest: surface.abi_digest(),
                table: surface.table.clone(),
                definitions: Box::new([]),
            },
        );
    }

    #[must_use]
    pub fn get(&self, module: &str) -> Option<&ImportedModule> {
        self.modules.get(module)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }
}

pub(crate) struct ScopedImports<'a> {
    modules: BTreeMap<&'a str, &'a ImportedModule>,
}

impl<'a> ScopedImports<'a> {
    pub fn get(&self, module: &str) -> Option<&'a ImportedModule> {
        self.modules.get(module).copied()
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
    let files = merge::ModuleFiles::one(&source);
    let scoped = scope_imports(&surface, imports, &files, &mut diagnostics);
    let definitions = declaration_definitions(&positions);
    let elaborated = elaborate::elaborate(
        &module,
        &surface,
        &scoped,
        &definitions,
        &files,
        &mut diagnostics,
    );
    let ordered_imports = scoped
        .modules
        .iter()
        .map(|(name, imported)| (Box::from(*name), imported.abi_digest))
        .collect::<Vec<_>>();
    let abi_signatures = elaborated
        .rules
        .iter()
        .map(|rule| abi::RuleSignature {
            category: rule.category,
            interface: rule.interface.clone(),
        })
        .collect::<Vec<_>>();
    let abi_digest = abi::abi_digest(
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
        .modules
        .iter()
        .map(|(name, imported)| (Box::from(*name), imported.definitions.clone()))
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

fn scope_imports<'a>(
    surface: &'a surface::ParsedSurface,
    environment: &'a ImportEnv,
    files: &merge::ModuleFiles,
    diagnostics: &mut Vec<Diag>,
) -> ScopedImports<'a> {
    let mut modules = BTreeMap::new();
    for import in &surface.imports {
        if modules.contains_key(import.module.as_ref()) {
            diagnostics.push(files.error(
                FrontendCode::DuplicateImport,
                import.span,
                format!("module `{}` is imported twice", import.module),
            ));
            continue;
        }
        let Some(imported) = environment.get(&import.module) else {
            diagnostics.push(files.error(
                FrontendCode::UnknownImport,
                import.span,
                format!("module `{}` is not available to import", import.module),
            ));
            continue;
        };
        modules.insert(import.module.as_ref(), imported);
    }
    ScopedImports { modules }
}

fn definition_locations(
    module: &str,
    surface: &surface::ParsedSurface,
    source: &Arc<SourceFile>,
) -> Vec<DefinitionLocation> {
    let declaration_definitions = surface.declarations.iter().map(|declaration| {
        let kind = match declaration.body {
            surface::SurfaceBody::Nominal(_) => DefinitionKind::Nominal,
            surface::SurfaceBody::Sum(_) => DefinitionKind::Sum,
            surface::SurfaceBody::Alias(_) => DefinitionKind::Alias,
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
