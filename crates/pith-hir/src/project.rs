//! The project header: the clauses that precede a project file's
//! declarations. A project is one file; the header names its identity, its
//! inputs, its host component, its workspace membership, and the files it
//! includes. An included file holds none of these, only declarations, so
//! the header is what makes a file a project.

use std::fmt;

use pith_diag::Span;

use crate::identity::{ModuleSubject, SubjectSegment};
use crate::version::{ModuleVersion, VersionRange};

/// The header a parsed file carries: a project's clauses, or nothing,
/// because the file is an include.
#[derive(Clone)]
pub enum DocumentHeader {
    Project(Box<ProjectHeader>),
    Include,
}

/// The header clauses of a project file as they parsed, singleton clauses
/// still optional.
#[derive(Clone)]
pub struct ProjectHeader {
    pub module: Option<ProjectModule>,
    pub inputs: InputsBlock,
    pub host: Option<ProjectHost>,
    pub workspace: Option<ProjectWorkspace>,
    pub includes: Box<[ProjectInclude]>,
}

/// The `inputs { ... }` block: the project's input bindings and the
/// registry bindings and domain routes that serve them.
#[derive(Clone)]
pub struct InputsBlock {
    /// The `inputs` keyword's span, where the block's diagnostics land.
    pub span: Span,
    pub inputs: Box<[ProjectInput]>,
    /// Registry bindings and domain routes: consumer configuration, which
    /// parses in any project and carries authority only in a root's.
    pub registries: Box<[ProjectRegistry]>,
    pub domains: Box<[ProjectDomain]>,
}

/// The `module domain/name version` clause: the project's declared subject
/// and version.
#[derive(Clone)]
pub struct ProjectModule {
    pub subject: ModuleSubject,
    pub subject_span: Span,
    pub version: ModuleVersion,
    pub span: Span,
}

/// One `name = locator` entry in the inputs block: a binding the project's
/// own `import` clauses may name, and the route that satisfies it.
#[derive(Clone)]
pub struct ProjectInput {
    pub name: Box<str>,
    pub name_span: Span,
    pub locator: InputLocator,
    pub span: Span,
}

/// Where an input's project comes from. A locator is a literal, so tools
/// rewrite it without evaluating anything; a registry locator is the one
/// that names its subject, because a registry serves by subject.
#[derive(Clone)]
pub enum InputLocator {
    /// A directory holding a project file, relative to the declaring
    /// project. Live and unwitnessed.
    Path { path: Box<str>, span: Span },
    /// One project at one immutable revision. Never an index: a written
    /// range is checked against the acquired project rather than used to
    /// search.
    Git {
        url: Box<str>,
        revision: Box<str>,
        /// The subpath holding the project root, when the repository holds
        /// more than the project.
        subpath: Option<Box<str>>,
        span: Span,
    },
    /// One project's bytes, admitted against a digest written beside them.
    Archive {
        url: Box<str>,
        digest: Box<str>,
        span: Span,
    },
    /// The subject's configured registry: the only route that consults a
    /// version universe, so the only one that carries a subject and a range.
    Registry {
        subject: ModuleSubject,
        subject_span: Span,
        range: VersionRange,
        range_span: Span,
        /// A named registry, when the entry selects one explicitly rather
        /// than taking the domain's configured route.
        registry: Option<Box<str>>,
        span: Span,
    },
}

impl InputLocator {
    /// The span the route is diagnosed at.
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Path { span, .. }
            | Self::Git { span, .. }
            | Self::Archive { span, .. }
            | Self::Registry { span, .. } => *span,
        }
    }

    /// The word a diagnostic names this route by.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Registry { .. } => "registry",
            Self::Path { .. } => "path",
            Self::Git { .. } => "git",
            Self::Archive { .. } => "archive",
        }
    }

    /// Whether the route selects among versions. Only a registry does; the
    /// others name one project, so a range on them constrains nothing.
    #[must_use]
    pub const fn resolves_versions(&self) -> bool {
        matches!(self, Self::Registry { .. })
    }

    /// The route's path and its span, for the one locator kind that
    /// resolves against the local filesystem.
    #[must_use]
    pub fn path(&self) -> Option<(&str, Span)> {
        match self {
            Self::Path { path, span } => Some((path, *span)),
            Self::Git { .. } | Self::Archive { .. } | Self::Registry { .. } => None,
        }
    }
}

/// The `host adapter ./artifact` clause: the component that serves the
/// project's `= host` rules. Parsed and carried; binding a component to it
/// is refused until host adapters exist.
#[derive(Clone)]
pub struct ProjectHost {
    pub adapter: Box<str>,
    pub artifact: Box<str>,
    pub span: Span,
}

/// The `workspace { members: [...] }` clause. At most one per project, and
/// an input target declares none.
#[derive(Clone)]
pub struct ProjectWorkspace {
    pub members: Box<[ProjectMember]>,
    pub span: Span,
}

#[derive(Clone)]
pub struct ProjectMember {
    /// A quoted path, relative to the declaring project, naming a directory
    /// that holds a project file.
    pub path: Box<str>,
    pub span: Span,
}

/// An `include "path"` clause: one file, relative to the project file,
/// holding declarations only.
#[derive(Clone)]
pub struct ProjectInclude {
    pub path: Box<str>,
    pub span: Span,
}

/// A `registry name = "locator" root "key"` clause: consumer configuration
/// appointing a registry and pinning the root key that vouches for its
/// domain key sets.
#[derive(Clone)]
pub struct ProjectRegistry {
    pub name: Box<str>,
    pub name_span: Span,
    /// Where the index is read from. A hint: overridable, and outside every
    /// identity.
    pub locator: Box<str>,
    pub locator_span: Span,
    pub root_key: RootKey,
    pub root_key_span: Span,
    pub span: Span,
}

/// A `domain <domain> from <registry>` clause: which registry answers for a
/// domain. There is no search order and no fallback, so a domain with no
/// clause does not resolve.
#[derive(Clone)]
pub struct ProjectDomain {
    pub domain: SubjectSegment,
    pub domain_span: Span,
    pub registry: Box<str>,
    pub registry_span: Span,
    pub span: Span,
}

/// A project header whose module clause is settled: the subject, version,
/// inputs, membership, and file set a loader resolves against.
pub struct Project {
    module: ProjectModule,
    inputs: Box<[ProjectInput]>,
    host: Option<ProjectHost>,
    workspace: Option<ProjectWorkspace>,
    includes: Box<[ProjectInclude]>,
    registries: Box<[ProjectRegistry]>,
    domains: Box<[ProjectDomain]>,
}

impl Project {
    /// The validated project this header holds once its module clause is
    /// settled.
    ///
    /// # Errors
    /// Returns [`MissingSubject`] when the header declares no module
    /// clause, which a loaded project cannot: its subject is what every
    /// binding and digest keys on.
    pub fn from_header(header: &ProjectHeader) -> Result<Self, MissingSubject> {
        Ok(Self {
            module: header.module.clone().ok_or(MissingSubject)?,
            inputs: header.inputs.inputs.clone(),
            host: header.host.clone(),
            workspace: header.workspace.clone(),
            includes: header.includes.clone(),
            registries: header.inputs.registries.clone(),
            domains: header.inputs.domains.clone(),
        })
    }

    #[must_use]
    pub fn subject(&self) -> &ModuleSubject {
        &self.module.subject
    }

    #[must_use]
    pub fn version(&self) -> &ModuleVersion {
        &self.module.version
    }

    #[must_use]
    pub fn inputs(&self) -> &[ProjectInput] {
        &self.inputs
    }

    #[must_use]
    pub fn host(&self) -> Option<&ProjectHost> {
        self.host.as_ref()
    }

    #[must_use]
    pub fn workspace(&self) -> Option<&ProjectWorkspace> {
        self.workspace.as_ref()
    }

    #[must_use]
    pub fn includes(&self) -> &[ProjectInclude] {
        &self.includes
    }

    /// The registry bindings this project declares. They are authority only
    /// in a root; a dependency's are diagnosed and ignored.
    #[must_use]
    pub fn registries(&self) -> &[ProjectRegistry] {
        &self.registries
    }

    /// The domain routes this project declares, under the same rule as
    /// [`Self::registries`].
    #[must_use]
    pub fn domains(&self) -> &[ProjectDomain] {
        &self.domains
    }

    /// The span of the module clause, where diagnostics about ownership of
    /// the project's file set land.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.module.span
    }
}

/// The parse held no module clause, so the file names no subject.
#[derive(Debug, PartialEq, Eq)]
pub struct MissingSubject;

impl fmt::Display for MissingSubject {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the project declares no `module` clause")
    }
}

impl std::error::Error for MissingSubject {}

/// A pinned root key: an algorithm name and its encoded material, spelled
/// `algorithm:material`. Constructed only through validation, so a clean
/// parse cannot hold a key nothing can interpret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootKey {
    algorithm: Box<str>,
    material: Box<str>,
}

impl RootKey {
    #[must_use]
    pub fn algorithm(&self) -> &str {
        &self.algorithm
    }

    #[must_use]
    pub fn material(&self) -> &str {
        &self.material
    }

    /// # Errors
    /// Returns why `spelling` is not a root key: a missing separator, or an
    /// empty half.
    pub fn parse(spelling: &str) -> Result<Self, RootKeyError> {
        let (algorithm, material) = spelling.split_once(':').ok_or(RootKeyError::Separator)?;
        if algorithm.is_empty() {
            return Err(RootKeyError::EmptyAlgorithm);
        }
        if material.is_empty() {
            return Err(RootKeyError::EmptyMaterial);
        }
        Ok(Self {
            algorithm: algorithm.into(),
            material: material.into(),
        })
    }
}

impl fmt::Display for RootKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.algorithm, self.material)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RootKeyError {
    Separator,
    EmptyAlgorithm,
    EmptyMaterial,
}

impl fmt::Display for RootKeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Separator => formatter.write_str("a root key is spelled `algorithm:material`"),
            Self::EmptyAlgorithm => formatter.write_str("a root key names a signature algorithm"),
            Self::EmptyMaterial => formatter.write_str("a root key carries key material"),
        }
    }
}
