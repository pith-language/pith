//! The workspace loader: explicit manifests in, a validated dependency
//! graph out. Loading begins from the supplied root (no upward search, no
//! ambient configuration), resolves routes, and refuses ambiguity; whether
//! a module elaborates is not this loader's question. Acquisition is
//! [`acquire`]'s and elaboration is [`elaborate`]'s, and every store a
//! route can name serves the same [`ModuleStore`] surface.

pub(crate) mod acquire;
mod admission;
mod closure;
mod elaborate;
mod frontend;
pub(crate) mod graph;
mod module;
mod root_files;
mod routing;

use std::path::Path;

use pith_diag::Diag;
use pith_hir::ModuleSubject;

pub use acquire::{
    AcquireFailure, AcquiredManifest, AcquiredSource, LocalDirectory, LocalFiles, MANIFEST_NAME,
    Route, SOURCE_DIRECTORY, SOURCE_SUFFIX,
};
pub use admission::{
    Admitted as AdmittedSource, Clause as AdmissionClause, Refusal as AdmissionRefusal,
    Request as AdmissionRequest, admit as admit_source,
};
pub use closure::Closure;
pub use elaborate::{
    CheckedModule, ElaborateError, ElaboratedWorkspace, Outcome, WorkspaceCheck, WorkspacePass,
};
pub use frontend::{
    FrontendInputs, FrontendProjection, ProjectedModule, ProjectionError, PublishedInterface,
};
pub use module::{ModuleFile, ResolvedModule, SourceSet};
pub use root_files::RootFiles;
pub use routing::{
    BindingOrigin, BindingOverride, BindingPolicy, BindingSite, RegistryRoute, UserBindings,
};

/// The store a workspace resolves against: where a route leads, and what
/// bytes live there. Public so source kinds that do not exist yet (a
/// registry client, a git checkout, an archive) feed the same loading path
/// the local filesystem does.
pub use acquire::ModuleStore;

/// A resolved workspace: the root module and its selected dependency
/// closure, dependencies first.
pub type Workspace = Closure<ResolvedModule>;

impl Workspace {
    /// Load the workspace whose root manifest is `root_manifest`, from the
    /// local filesystem.
    ///
    /// # Errors
    /// Returns every acquisition failure and validation refusal, attached
    /// to the clause or file that caused it.
    pub fn load(root_manifest: &Path) -> Result<Self, Box<[Diag]>> {
        let directory = LocalDirectory::new(root_files::root_directory(root_manifest)?);
        Self::resolve(LocalFiles, &directory)
    }

    /// Resolve the workspace whose root manifest `root` names, acquiring
    /// every module through `store`.
    ///
    /// # Errors
    /// Returns every acquisition failure and validation refusal, each
    /// attached to the clause or file that caused it.
    pub fn resolve<S: ModuleStore>(store: S, root: &S::Location) -> Result<Self, Box<[Diag]>> {
        Self::resolve_configured(store, root, None, BindingPolicy::ProjectOnly)
            .map(WorkspaceResolution::into_workspace)
    }

    /// Resolve with explicitly supplied user bindings and a binding policy;
    /// the result retains the binding overrides its caller must report.
    ///
    /// # Errors
    /// Returns configuration, acquisition, or manifest refusals with their
    /// sources.
    pub fn resolve_configured<S: ModuleStore>(
        store: S,
        root: &S::Location,
        user: Option<UserBindings>,
        policy: BindingPolicy,
    ) -> Result<WorkspaceResolution, Box<[Diag]>> {
        let mut resolution = graph::Resolution::prepare(store, root, user, policy)?;
        resolution.acquire_source_sets();
        resolution.finish()
    }

    #[must_use]
    pub fn module(&self, subject: &ModuleSubject) -> Option<&ResolvedModule> {
        self.modules().find(|module| module.subject() == subject)
    }
}

/// A resolved workspace with the authority collisions its caller must report.
#[must_use]
pub struct WorkspaceResolution {
    pub(crate) workspace: Workspace,
    pub(crate) overrides: Box<[BindingOverride]>,
}

impl WorkspaceResolution {
    #[must_use]
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    #[must_use]
    pub fn overrides(&self) -> &[BindingOverride] {
        &self.overrides
    }

    #[must_use]
    pub fn into_parts(self) -> (Workspace, Box<[BindingOverride]>) {
        (self.workspace, self.overrides)
    }

    fn into_workspace(self) -> Workspace {
        self.workspace
    }
}
