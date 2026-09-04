//! A root module and the transitive file-relative modules it imports, loaded
//! once and ready to bind onto either engine authority.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pith_core::{Action, BodyRevision, Rule, RuleId, Value};
use pith_diag::{Diag, DiagnosticSink, EngineCode, PithResult, Text};
use pith_engine::{
    ActionExecution, ActionRule, Engine, EngineStateReader, PureRule, PureRuleFrame, PureStep,
    Resumption,
};
use pith_loader::{
    EntryDeclaration, ImportEnv, LoadedModule, ModuleSource, ParsedModule, RuleDeclaration,
    elaborate_module, parse_module,
};

use crate::error::QueryError;
use crate::source::read_module;

pub(crate) struct Program {
    root: LoadedModule,
    dependencies: Box<[LoadedModule]>,
}

impl Program {
    pub(crate) fn load(path: &Path) -> Result<Self, QueryError> {
        let source = read_module(path)?;
        let mut imports = builtin_environment()?;
        let mut loading = BTreeSet::from([path.to_path_buf()]);
        let mut dependencies = Vec::new();
        let parsed = populate_imports(
            path,
            &source,
            &mut imports,
            &mut loading,
            Some(&mut dependencies),
        )?;
        let root = elaborate_module(parsed, &imports).map_err(|diagnostics| {
            QueryError::user(format!("`{}` does not elaborate", path.display()))
                .with_diagnostics(diagnostics)
        })?;
        Ok(Self {
            root,
            dependencies: dependencies.into(),
        })
    }

    pub(crate) fn module(&self) -> &str {
        self.root.module()
    }

    pub(crate) fn entry(&self, name: &str) -> Result<&EntryDeclaration, QueryError> {
        self.root.entry(name).ok_or_else(|| {
            QueryError::user(format!(
                "module `{}` declares no entry `{name}`",
                self.root.module()
            ))
        })
    }

    pub(crate) fn bind<S>(&self, engine: &mut Engine<S>) -> Result<(), QueryError>
    where
        S: EngineStateReader + ?Sized,
    {
        for module in &self.dependencies {
            bind_module(module, engine)?;
        }
        bind_module(&self.root, engine)
    }

    pub(crate) fn register_entry<S>(
        &self,
        name: &str,
        engine: &mut Engine<S>,
    ) -> Result<RuleId, QueryError>
    where
        S: EngineStateReader + ?Sized,
    {
        self.entry(name)?.register(engine).map_err(|error| {
            QueryError::internal(format!("entry body no longer validates: {error}"))
        })
    }
}

/// The import environment `source` elaborates under, together with `source`'s
/// own parse, so the caller elaborates the root without parsing it twice.
pub(crate) fn prepared_imports(
    path: &Path,
    source: &ModuleSource,
) -> Result<(ImportEnv, ParsedModule), QueryError> {
    let mut imports = builtin_environment()?;
    let mut loading = BTreeSet::from([path.to_path_buf()]);
    let parsed = populate_imports(path, source, &mut imports, &mut loading, None)?;
    Ok((imports, parsed))
}

fn builtin_environment() -> Result<ImportEnv, QueryError> {
    ImportEnv::with_builtins()
        .map_err(|error| QueryError::internal(format!("cannot construct builtin types: {error}")))
}

/// Parse `source` once, resolve and load its transitive imports in dependency
/// order, and hand the caller the parse so the root elaborates from it. Every
/// module in the graph is parsed exactly once.
fn populate_imports(
    path: &Path,
    source: &ModuleSource,
    imports: &mut ImportEnv,
    loading: &mut BTreeSet<PathBuf>,
    mut loaded_modules: Option<&mut Vec<LoadedModule>>,
) -> Result<ParsedModule, QueryError> {
    let parsed = parse_module(source);
    for module in parsed.imports() {
        if imports.get(module).is_some() {
            continue;
        }
        let Some(import_path) = resolve_import(path, module)? else {
            continue;
        };
        if !loading.insert(import_path.clone()) {
            continue;
        }
        let imported_source = read_module(&import_path)?;
        let imported = populate_imports(
            &import_path,
            &imported_source,
            imports,
            loading,
            loaded_modules.as_deref_mut(),
        )?;
        let loaded = elaborate_module(imported, imports).map_err(|diagnostics| {
            QueryError::user(format!("imported module `{module}` does not elaborate"))
                .with_diagnostics(diagnostics)
        })?;
        imports.insert_loaded(&loaded);
        if let Some(modules) = loaded_modules.as_deref_mut() {
            modules.push(loaded);
        }
        let _ = loading.remove(&import_path);
    }
    Ok(parsed)
}

fn resolve_import(path: &Path, module: &str) -> Result<Option<PathBuf>, QueryError> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let filename = format!("{module}.pi");
    let mut candidates = vec![directory.join(&filename)];
    if let Some(parent) = directory.parent() {
        candidates.push(parent.join(module).join(&filename));
    }
    for candidate in candidates {
        match candidate.try_exists() {
            Ok(true) if candidate != path => return Ok(Some(candidate)),
            Ok(_) => {}
            Err(error) => {
                return Err(QueryError::user(format!(
                    "cannot inspect import path `{}`: {error}",
                    candidate.display()
                )));
            }
        }
    }
    Ok(None)
}

fn bind_module<S>(module: &LoadedModule, engine: &mut Engine<S>) -> Result<(), QueryError>
where
    S: EngineStateReader + ?Sized,
{
    for declaration in module.pure_rules() {
        match declaration {
            RuleDeclaration::Represented(rule) => {
                rule.register(engine).map_err(|error| {
                    QueryError::internal(format!("represented body no longer validates: {error}"))
                })?;
            }
            RuleDeclaration::Host(rule) => {
                rule.bind(
                    engine,
                    BodyRevision(0),
                    UnboundPure::new(unbound_host(
                        module,
                        rule.coordinate().spelling(),
                        rule.span(),
                    )),
                );
            }
        }
    }
    for declaration in module.action_rules() {
        match declaration {
            RuleDeclaration::Host(rule) => {
                rule.bind(
                    engine,
                    BodyRevision(0),
                    UnboundAction::new(unbound_host(
                        module,
                        rule.coordinate().spelling(),
                        rule.span(),
                    )),
                );
            }
            RuleDeclaration::Represented(rule) => {
                let metadata = Rule::<Action>::represented_action(
                    &rule.coordinate().module,
                    &rule.coordinate().name,
                    rule.body(),
                    rule.interface().clone(),
                    rule.span(),
                );
                engine.register_action_rule(
                    metadata,
                    UnboundAction::new(Diag::engine(
                        EngineCode::NoRuleForInterface,
                        rule.span(),
                        format!(
                            "represented action `{}` has no ActionSpec projection",
                            rule.coordinate().spelling()
                        ),
                    )),
                );
            }
        }
    }
    Ok(())
}

fn unbound_host(module: &LoadedModule, coordinate: String, span: pith_diag::Span) -> Diag {
    Diag::engine(
        EngineCode::NoRuleForInterface,
        span,
        format!("`{coordinate}` is `= host`; the CLI links no domain crate"),
    )
    .with_source(module.source().clone())
}

pub(crate) fn teach_entry_collision(mut diagnostic: Diag) -> Diag {
    if diagnostic.code == EngineCode::AmbiguousRule.into() {
        diagnostic.message = Text::new(format!(
            "{}; an entry name chooses a request, not a preferred rule, so give the entry and module rule distinct interfaces",
            diagnostic.message.0
        ));
    }
    diagnostic
}

struct UnboundPure {
    diagnostic: Diag,
}

impl UnboundPure {
    fn new(diagnostic: Diag) -> Self {
        Self { diagnostic }
    }
}

impl PureRule for UnboundPure {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(UnboundPureFrame {
            diagnostic: self.diagnostic.clone(),
        })
    }
}

struct UnboundPureFrame {
    diagnostic: Diag,
}

impl PureRuleFrame for UnboundPureFrame {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        Err(one_diagnostic(self.diagnostic.clone()))
    }
}

struct UnboundAction {
    diagnostic: Diag,
}

impl UnboundAction {
    fn new(diagnostic: Diag) -> Self {
        Self { diagnostic }
    }
}

impl ActionRule for UnboundAction {
    fn plan(&self, _inputs: &[Value]) -> PithResult<pith_core::ActionSpec> {
        Err(one_diagnostic(self.diagnostic.clone()))
    }

    fn complete(&self, _inputs: &[Value], _execution: &ActionExecution) -> PithResult<Value> {
        Err(one_diagnostic(self.diagnostic.clone()))
    }
}

fn one_diagnostic(diagnostic: Diag) -> DiagnosticSink {
    let mut diagnostics = DiagnosticSink::new();
    diagnostics.push(diagnostic);
    diagnostics
}
