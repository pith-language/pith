//! A resolved workspace projected onto the frontend graph tier:
//! [`super::Workspace`]'s modules re-asked by content identity, under an
//! import environment naming what each dependency published. This module is
//! the one place that translation happens, and nothing here re-derives a
//! module's identity: a file's content identity comes from the store that
//! will serve it, and a dependency's ABI from the surface it published.

use pith_core::{Pure, Request};
use pith_diag::DiagnosticSink;
use pith_engine::Engine;
use pith_ids::{ContentId, ModuleAbiDigest};

use super::{Closure, ResolvedModule};
use crate::graph::{
    FrontendImport, FrontendImportEnv, FrontendInputError, FrontendSource, InterfaceSurface,
    bodies_of_request, index_of_request, interface_of_request, values,
};

/// What a dependency published: the pair a consumer's import environment
/// names it by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublishedInterface {
    abi: ModuleAbiDigest,
    surface: ContentId,
}

impl PublishedInterface {
    #[must_use]
    pub const fn abi(self) -> ModuleAbiDigest {
        self.abi
    }

    /// The content identity of the encoded surface, as stored.
    #[must_use]
    pub const fn surface(self) -> ContentId {
        self.surface
    }
}

/// What the frontend graph rules are asked about one module: its source set
/// by content identity, and the interfaces its bindings published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontendInputs {
    source: FrontendSource,
    imports: FrontendImportEnv,
}

impl FrontendInputs {
    #[must_use]
    pub const fn source(&self) -> &FrontendSource {
        &self.source
    }

    #[must_use]
    pub const fn imports(&self) -> &FrontendImportEnv {
        &self.imports
    }

    #[must_use]
    pub fn interface_of(&self) -> Request<Pure> {
        interface_of_request(self.source.clone(), self.imports.clone())
    }

    #[must_use]
    pub fn bodies_of(&self) -> Request<Pure> {
        bodies_of_request(self.source.clone(), self.imports.clone())
    }

    #[must_use]
    pub fn index_of(&self) -> Request<Pure> {
        index_of_request(self.source.clone(), self.imports.clone())
    }
}

/// One module's place in the projection: what to ask about it, and what it
/// published for its own consumers.
pub struct ProjectedModule {
    inputs: FrontendInputs,
    interface: PublishedInterface,
}

impl ProjectedModule {
    #[must_use]
    pub const fn inputs(&self) -> &FrontendInputs {
        &self.inputs
    }

    #[must_use]
    pub const fn interface(&self) -> PublishedInterface {
        self.interface
    }
}

/// Every module of a workspace, projected in dependency order.
pub type FrontendProjection<'workspace> = Closure<(&'workspace ResolvedModule, ProjectedModule)>;

/// Why a workspace could not be projected onto the graph tier.
#[derive(Debug)]
pub enum ProjectionError {
    /// The content store could not hold a module's source or surface.
    Store(pith_store::StoreError),
    /// A module's source set or import environment is not canonical.
    Inputs {
        subject: Box<str>,
        error: FrontendInputError,
    },
    /// `interface-of` refused the module.
    Refused {
        subject: Box<str>,
        diagnostics: DiagnosticSink,
    },
    /// `interface-of` returned a value the surface cannot be read out of:
    /// the rule's declared output shape and this reader disagree, a defect
    /// in the graph tier rather than in the workspace.
    MalformedInterface { subject: Box<str> },
    /// A `use` clause named a subject the closure did not resolve, so no
    /// interface was published for it.
    UnresolvedBinding { subject: Box<str>, alias: Box<str> },
}

impl std::fmt::Display for ProjectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "the content store refused a write: {error}"),
            Self::Inputs { subject, error } => {
                write!(
                    formatter,
                    "{subject} has no canonical frontend inputs: {error}"
                )
            }
            Self::Refused { subject, .. } => write!(formatter, "{subject} does not elaborate"),
            Self::MalformedInterface { subject } => {
                write!(formatter, "interface-of returned no surface for {subject}")
            }
            Self::UnresolvedBinding { subject, alias } => write!(
                formatter,
                "{subject} binds `{alias}` to a subject the closure did not resolve"
            ),
        }
    }
}

impl std::error::Error for ProjectionError {}

impl super::Workspace {
    /// Project every module of the closure onto the frontend graph tier,
    /// publishing each module's sources and elaborated surface into
    /// `engine`'s content store on the way. Dependency order makes this one
    /// pass: a module is projected only after every subject it binds has
    /// published an interface, so no import environment is assembled from a
    /// placeholder.
    ///
    /// # Errors
    /// Returns [`ProjectionError`] for a store failure, non-canonical
    /// inputs, a module `interface-of` refused, or a binding the closure
    /// did not resolve.
    pub fn project_onto_frontend(
        &self,
        engine: &mut Engine,
    ) -> Result<FrontendProjection<'_>, ProjectionError> {
        self.try_scan(|module, projected| project_module(module, projected, engine))
    }
}

fn project_module(
    module: &ResolvedModule,
    projected: &[(&ResolvedModule, ProjectedModule)],
    engine: &mut Engine,
) -> Result<ProjectedModule, ProjectionError> {
    let subject = module.subject().spelling();
    let inputs = FrontendInputs {
        source: publish_sources(module, engine)?,
        imports: import_environment(module, projected)?,
    };
    let interface = publish_interface(&subject, &inputs, engine)?;
    Ok(ProjectedModule { inputs, interface })
}

/// The module's source set, as the graph tier names it: every owned file
/// written to the store, keyed by the identity the store returned.
fn publish_sources(
    module: &ResolvedModule,
    engine: &mut Engine,
) -> Result<FrontendSource, ProjectionError> {
    let subject = module.subject().spelling();
    let mut files = Vec::with_capacity(module.sources().files().len());
    for file in module.sources().files() {
        let content = engine
            .put_blob(file.text().as_bytes())
            .map_err(ProjectionError::Store)?;
        files.push((Box::from(file.path()), content));
    }
    FrontendSource::new(subject.clone(), files)
        .map_err(|error| ProjectionError::Inputs { subject, error })
}

/// The module's import environment: one entry per `use` clause, naming the
/// alias it scopes under and the interface its subject published.
fn import_environment(
    module: &ResolvedModule,
    projected: &[(&ResolvedModule, ProjectedModule)],
) -> Result<FrontendImportEnv, ProjectionError> {
    let subject = module.subject().spelling();
    let mut entries = Vec::new();
    for (alias, bound) in module.bindings() {
        let published = projected
            .iter()
            .find(|(dependency, _)| dependency.subject() == bound)
            .map(|(_, dependency)| dependency.interface)
            .ok_or_else(|| ProjectionError::UnresolvedBinding {
                subject: subject.clone(),
                alias: alias.into(),
            })?;
        entries.push(FrontendImport::new(
            alias,
            bound.spelling(),
            published.abi,
            published.surface,
        ));
    }
    FrontendImportEnv::new(entries).map_err(|error| ProjectionError::Inputs { subject, error })
}

/// Elaborate the module's interface through the graph tier and store the
/// surface it produced, so its consumers can name it by content identity.
fn publish_interface(
    subject: &str,
    inputs: &FrontendInputs,
    engine: &mut Engine,
) -> Result<PublishedInterface, ProjectionError> {
    let evaluation = engine
        .evaluate_with_content(&inputs.interface_of())
        .map_err(|diagnostics| ProjectionError::Refused {
            subject: subject.into(),
            diagnostics,
        })?;
    let encoded = values::read_interface_surface(&evaluation.value).ok_or_else(|| {
        ProjectionError::MalformedInterface {
            subject: subject.into(),
        }
    })?;
    let surface =
        InterfaceSurface::decode(encoded).map_err(|_| ProjectionError::MalformedInterface {
            subject: subject.into(),
        })?;
    let stored = engine.put_blob(encoded).map_err(ProjectionError::Store)?;
    Ok(PublishedInterface {
        abi: surface.abi_digest(),
        surface: stored,
    })
}
