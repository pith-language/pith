//! Elaboration of a resolved workspace: every module of the closure under
//! its own bindings.
//!
//! One traversal produces one outcome per module; `check` and `elaborate`
//! are two projections of it rather than two walks that could disagree
//! about dependency order or about what a module may name.

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

/// One traversal's result: every module of the closure paired with what the
/// pass made of it.
pub type WorkspacePass<'workspace> = Closure<(&'workspace ResolvedModule, Outcome)>;

/// What one traversal made of one module.
///
/// The three states are distinct facts, not degrees of failure: the module
/// elaborated, it was attempted and refused, or it was never attempted
/// because something it binds did not elaborate. Only the first carries an
/// ABI, and only the second carries a refusal of its own — a blocked module
/// has nothing to answer for.
pub enum Outcome {
    /// Boxed so a pass over a large closure carries one pointer per
    /// refused or blocked module rather than a module-sized hole.
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
    /// warnings an elaborated module carried out with it.
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
    /// Every parse and elaboration diagnostic collected, each attached to
    /// its source.
    Diagnostics(Box<[Diag]>),
    /// The builtin module table is invalid: a kernel invariant, not a
    /// property of the workspace.
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
    /// Returns [`ElaborateError::Builtins`] when the builtin table is
    /// invalid. A module's own refusal is an outcome, not an error.
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
    /// Returns [`ElaborateError::Diagnostics`] with every refusal the pass
    /// collected, or [`ElaborateError::Builtins`] when the builtin table is
    /// invalid.
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

    /// Elaborate tolerantly: report every module's outcome rather than
    /// raising the first refusal.
    ///
    /// # Errors
    /// Returns [`ElaborateError::Builtins`] when the builtin table is
    /// invalid; diagnostic outcomes are values, not errors.
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

/// One module of the closure, elaborated under the environment its own
/// manifest declared.
fn elaborate_one(
    module: &ResolvedModule,
    environment: &crate::ImportEnv,
) -> Result<LoadedModule, Box<[Diag]>> {
    let parsed =
        crate::source::parse_module_sources(&module.subject().spelling(), module.sources());
    crate::load::elaborate_module(parsed, environment)
}

/// The environment a module elaborates under: its own `use` clauses and
/// nothing else, resolved against the dependencies that precede it.
///
/// `None` when a binding did not elaborate. Attempting the module anyway
/// would report the consumer for its dependency's refusal, at a name that
/// is missing only because something upstream failed.
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
