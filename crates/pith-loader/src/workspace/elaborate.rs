//! Elaboration of a resolved workspace: every module of the closure under
//! its own bindings. One traversal produces one outcome per module;
//! `check` and `elaborate` are projections of that single pass.

use pith_diag::Diag;
use pith_hir::ModuleSubject;
use pith_ids::ModuleAbiDigest;

use super::{Closure, ResolvedModule, Workspace};
use crate::loaded::LoadedModule;

/// A workspace's modules, elaborated in dependency order.
pub type ElaboratedWorkspace = Closure<LoadedModule>;

impl ElaboratedWorkspace {
    #[must_use]
    pub fn module(&self, subject: &ModuleSubject) -> Option<&LoadedModule> {
        self.modules()
            .find(|module| module.module() == subject.spelling().as_ref())
    }
}

/// Each module of the closure paired with the outcome the pass produced.
pub type WorkspacePass<'workspace> = Closure<(&'workspace ResolvedModule, Outcome)>;

/// What the pass made of one module: it elaborated, it was attempted and
/// refused, or it was never attempted because a binding's dependency did
/// not elaborate. Only the elaborated state carries an ABI; only the
/// refused state carries diagnostics of its own.
pub enum Outcome {
    /// Boxed so the outcome stays pointer-sized regardless of module size.
    Elaborated(Box<LoadedModule>),
    Refused(Box<[Diag]>),
    Blocked,
}

impl Outcome {
    #[must_use]
    pub fn abi_digest(&self) -> Option<ModuleAbiDigest> {
        match self {
            Self::Elaborated(module) => Some(module.abi_digest()),
            Self::Refused(_) | Self::Blocked => None,
        }
    }

    /// The diagnostics this module produced: a refusal's own, or the
    /// warnings an elaborated module carried with it.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diag] {
        match self {
            Self::Elaborated(module) => module.diagnostics(),
            Self::Refused(diagnostics) => diagnostics,
            Self::Blocked => &[],
        }
    }

    /// The refusal, when the module was attempted and refused.
    #[must_use]
    pub fn refusal(&self) -> &[Diag] {
        match self {
            Self::Refused(diagnostics) => diagnostics,
            Self::Elaborated(_) | Self::Blocked => &[],
        }
    }

    fn into_elaborated(self) -> Result<LoadedModule, Box<[Diag]>> {
        match self {
            Self::Elaborated(module) => Ok(*module),
            Self::Refused(diagnostics) => Err(diagnostics),
            Self::Blocked => Err(Box::from([])),
        }
    }
}

/// Why a workspace did not elaborate.
#[derive(Debug)]
pub enum ElaborateError {
    /// Every parse and elaboration diagnostic collected.
    Diagnostics(Box<[Diag]>),
    /// The builtin module table itself is invalid: a kernel invariant, not
    /// a property of the workspace.
    Builtins(pith_core::DeclarationError),
}

impl std::fmt::Display for ElaborateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Diagnostics(_) => formatter.write_str("a module of the workspace failed"),
            Self::Builtins(error) => write!(formatter, "cannot construct builtin types: {error}"),
        }
    }
}

impl std::error::Error for ElaborateError {}

impl Workspace {
    /// Attempt every module of the selected closure in dependency order,
    /// each under the bindings its own manifest declared.
    ///
    /// # Errors
    /// [`ElaborateError`] when the builtin table is invalid; a module's own
    /// refusal is an outcome, not an error.
    pub fn attempt(&self) -> Result<WorkspacePass<'_>, ElaborateError> {
        self.try_scan(|module, resolved| {
            let Some(environment) = binding_environment(module, resolved)? else {
                return Ok(Outcome::Blocked);
            };
            Ok(match elaborate_one(module, &environment) {
                Ok(elaborated) => Outcome::Elaborated(Box::new(elaborated)),
                Err(diagnostics) => Outcome::Refused(diagnostics),
            })
        })
    }

    /// Elaborate every module of the selected closure.
    ///
    /// # Errors
    /// [`ElaborateError`] with every collected refusal, or when the builtin
    /// table is invalid.
    pub fn elaborate(&self) -> Result<ElaboratedWorkspace, ElaborateError> {
        let pass = self.attempt()?;
        let refusals = pass
            .modules()
            .flat_map(|(_, outcome)| outcome.refusal())
            .cloned()
            .collect::<Vec<_>>();
        if !refusals.is_empty() {
            return Err(ElaborateError::Diagnostics(refusals.into()));
        }
        pass.try_map(|(_, outcome)| outcome.into_elaborated())
            .map_err(ElaborateError::Diagnostics)
    }

    /// Elaborate tolerantly: report every module's outcome instead of
    /// raising the first refusal.
    ///
    /// # Errors
    /// [`ElaborateError`] when the builtin table is invalid; diagnostic
    /// outcomes are values, not errors.
    pub fn check(&self) -> Result<WorkspaceCheck, ElaborateError> {
        let pass = self.attempt()?;
        Ok(WorkspaceCheck {
            root: pass.root().0.subject().spelling(),
            root_abi: pass.root().1.abi_digest(),
            modules: pass
                .modules()
                .map(|(module, outcome)| CheckedModule {
                    subject: module.subject().spelling(),
                    diagnostics: outcome.diagnostics().to_vec().into(),
                    abi_digest: outcome.abi_digest(),
                })
                .collect(),
        })
    }
}

/// Elaborate one module under the environment its own inputs declared.
fn elaborate_one(
    module: &ResolvedModule,
    environment: &crate::ImportEnv,
) -> Result<LoadedModule, Box<[Diag]>> {
    let parsed = crate::source::parse_project_files(&module.subject().spelling(), module.files());
    crate::load::elaborate_module(parsed, environment)
}

/// The environment a module elaborates under: its own inputs resolved
/// against its dependencies. `None` when a dependency did not elaborate,
/// so the consumer is not blamed for an upstream refusal.
fn binding_environment(
    module: &ResolvedModule,
    resolved: &[(&ResolvedModule, Outcome)],
) -> Result<Option<crate::ImportEnv>, ElaborateError> {
    let mut environment = crate::ImportEnv::with_builtins().map_err(ElaborateError::Builtins)?;
    for (alias, subject) in module.bindings() {
        let Some(Outcome::Elaborated(dependency)) = outcome_of(resolved, subject) else {
            return Ok(None);
        };
        environment.insert_alias(alias, dependency);
    }
    Ok(Some(environment))
}

fn outcome_of<'pass>(
    resolved: &'pass [(&ResolvedModule, Outcome)],
    subject: &ModuleSubject,
) -> Option<&'pass Outcome> {
    resolved
        .iter()
        .find(|(module, _)| module.subject() == subject)
        .map(|(_, outcome)| outcome)
}

/// One module's outcome in a tolerant elaboration pass.
pub struct CheckedModule {
    pub subject: Box<str>,
    pub diagnostics: Box<[Diag]>,
    /// The module's ABI digest, present when it elaborated.
    pub abi_digest: Option<ModuleAbiDigest>,
}

/// What a tolerant pass over a workspace reports.
pub struct WorkspaceCheck {
    pub modules: Vec<CheckedModule>,
    pub root: Box<str>,
    pub root_abi: Option<ModuleAbiDigest>,
}
