//! A root project and its transitive dependencies, loaded once and ready
//! to bind onto either engine authority. Loading begins from the supplied
//! project file; there is no file-relative import resolution and no
//! fallback mode.

use pith_core::{Action, BodyRevision, Rule, RuleId, Value};
use pith_diag::{Diag, DiagnosticSink, EngineCode, PithResult, Text};
use pith_engine::{
    ActionExecution, ActionRule, Engine, EngineStateReader, PureRule, PureRuleFrame, PureStep,
    Resumption,
};
use pith_loader::{EntryDeclaration, LoadedModule, RuleDeclaration, Workspace, write_lock};

use crate::error::QueryError;

pub(crate) struct Program {
    root: LoadedModule,
    dependencies: Box<[LoadedModule]>,
}

impl Program {
    pub(crate) fn load(path: &std::path::Path) -> Result<Self, QueryError> {
        let workspace = load_workspace(path)?;
        let (root, dependencies) = workspace
            .elaborate()
            .map_err(elaborate_failure)?
            .into_parts();
        Ok(Self { root, dependencies })
    }

    pub(crate) fn root(&self) -> &LoadedModule {
        &self.root
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

/// Resolve the workspace whose root project file is `path`, then write the
/// lock beside it: the first command that resolves inputs is the one that
/// records them.
pub(crate) fn load_workspace(path: &std::path::Path) -> Result<Workspace, QueryError> {
    let workspace = Workspace::load(path).map_err(|diagnostics| {
        QueryError::user(format!(
            "`{}` does not resolve as a project root",
            path.display()
        ))
        .with_diagnostics(diagnostics)
    })?;
    write_lock(&workspace, path).map_err(|error| {
        QueryError::user(format!(
            "cannot write the lock beside `{}`: {error}",
            path.display()
        ))
    })?;
    Ok(workspace)
}

/// What `check` reports for a project-rooted program: the root's subject,
/// the root's ABI when it elaborated, and every module's diagnostics. A
/// module whose dependency failed reports no diagnostics of its own, so one
/// refusal cannot cascade into missing-binding noise.
pub(crate) struct WorkspaceCheck {
    pub(crate) root: Box<str>,
    pub(crate) abi_digest: Option<pith_ids::ModuleAbiDigest>,
    pub(crate) diagnostics: Vec<Diag>,
}

pub(crate) fn check_project(root_project: &std::path::Path) -> Result<WorkspaceCheck, QueryError> {
    let workspace = match Workspace::load(root_project) {
        Ok(workspace) => workspace,
        Err(diagnostics) => {
            // A root that does not resolve still reports: the check command's
            // contract is a record either way.
            return Ok(WorkspaceCheck {
                root: root_project
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default()
                    .into(),
                abi_digest: None,
                diagnostics: diagnostics.into_vec(),
            });
        }
    };
    write_lock(&workspace, root_project).map_err(|error| {
        QueryError::user(format!(
            "cannot write the lock beside `{}`: {error}",
            root_project.display()
        ))
    })?;
    let checked = workspace.check().map_err(elaborate_failure)?;
    Ok(WorkspaceCheck {
        root: checked.root,
        abi_digest: checked.root_abi,
        diagnostics: checked
            .modules
            .into_iter()
            .flat_map(|module| module.diagnostics.into_vec())
            .collect(),
    })
}

fn elaborate_failure(error: pith_loader::ElaborateError) -> QueryError {
    match error {
        pith_loader::ElaborateError::Diagnostics(diagnostics) => {
            QueryError::user("a module of the workspace does not elaborate")
                .with_diagnostics(diagnostics)
        }
        pith_loader::ElaborateError::Builtins(error) => {
            QueryError::internal(format!("cannot construct builtin types: {error}"))
        }
    }
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
    let (source, local) = module.files().file_of(span);
    Diag::engine(
        EngineCode::NoRuleForInterface,
        local,
        format!("`{coordinate}` is `= host`; the CLI links no domain crate"),
    )
    .with_source(source.clone())
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
