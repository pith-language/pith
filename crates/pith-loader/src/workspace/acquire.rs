//! The boundary between acquiring a project and deciding what it means:
//! [`ProjectStore`] is everything resolution needs from the world (where a
//! declared route leads, the project file there, the includes it names).
//! [`super::graph`] holds the other half and names no filesystem, so
//! validation refusals are a function of what was acquired.

use std::fs;
use std::path::{Path, PathBuf};

use pith_hir::FrontendCode;

/// The project file name a project directory is named by.
pub const PROJECT_NAME: &str = "pith.pi";

/// A project file as acquired: the bytes, and the label its diagnostics
/// carry.
pub struct AcquiredProject {
    pub label: Box<str>,
    pub text: Box<str>,
}

/// One acquired include: its project-relative path in forward slashes, and
/// its text.
pub struct AcquiredSource {
    pub path: Box<str>,
    pub text: Box<str>,
}

/// Why a store could not produce what a route asked for: one clause per
/// cause, so a caller attaching a span reads the diagnostic off the variant
/// instead of parsing a message.
pub enum AcquireFailure {
    /// The route names a source kind this store does not serve.
    Unsupported { kind: &'static str },
    /// The location could not be reached, listed, or read.
    Unreadable { message: Box<str> },
    /// A symlink inside a project's file set: following it would let a
    /// project silently own content its header does not claim.
    Symlink { path: Box<str> },
    /// Not a regular file where one was required. Reading a fifo blocks, so
    /// this is refused before the read rather than after.
    Irregular { path: Box<str> },
    /// A store with its own trust boundary refused the route; the code names
    /// which boundary.
    Refused {
        code: FrontendCode,
        message: Box<str>,
    },
}

impl AcquireFailure {
    pub(crate) fn unreadable(what: &str, path: &Path, error: &std::io::Error) -> Self {
        Self::Unreadable {
            message: format!("cannot {what} `{}`: {error}", path.display()).into(),
        }
    }

    /// The code this refusal names for itself, if any; a caller with no
    /// better code falls back to its generic one.
    #[must_use]
    pub(crate) const fn diagnostic_code(&self) -> Option<FrontendCode> {
        match self {
            Self::Unsupported { .. } => Some(FrontendCode::UnsupportedSource),
            Self::Symlink { .. } => Some(FrontendCode::SymlinkedSource),
            Self::Irregular { .. } => Some(FrontendCode::IrregularSource),
            Self::Refused { code, .. } => Some(*code),
            _ => None,
        }
    }

    /// The failure as a sentence, for the caller attaching the span: an
    /// acquisition failure carries no span of its own, because only the
    /// clause that reached for the file knows where to point.
    #[must_use]
    pub fn describe(&self) -> Box<str> {
        match self {
            Self::Unsupported { kind } => format!(
                "this loader acquires no `{kind}` source; the route names a source kind \
                 it does not serve"
            )
            .into(),
            Self::Unreadable { message } => message.clone(),
            Self::Symlink { path } => {
                format!("`{path}` is a symlink, and a project owns its own files").into()
            }
            Self::Irregular { path } => {
                format!("`{path}` is not a regular file, and a project owns regular files").into()
            }
            Self::Refused { message, .. } => message.clone(),
        }
    }
}

/// What resolution is asking a store to reach.
///
/// The registry is the only route that names a subject, because a registry
/// serves by subject; every other locator names content whose own project
/// file declares what it is. A workspace member is a directory reached the
/// same way a path input reaches one. A store that cannot serve a variant
/// says so; there is no fallback route.
pub enum Route<'a> {
    Registry(super::routing::RegistryRoute<'a>),
    Path {
        path: &'a str,
    },
    Git {
        url: &'a str,
        revision: &'a str,
        subpath: Option<&'a str>,
    },
    Archive {
        url: &'a str,
        digest: &'a str,
    },
    Member {
        path: &'a str,
    },
}

impl Route<'_> {
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Registry(_) => "registry",
            Self::Path { .. } | Self::Member { .. } => "path",
            Self::Git { .. } => "git",
            Self::Archive { .. } => "archive",
        }
    }
}

/// Where a project's bytes come from.
///
/// A store's `Location` is its canonical name for a project: routes reaching
/// the same project produce equal locations, which is how resolution detects
/// a diamond and a cycle without knowing what a route is. A location never
/// becomes a semantic key: identity is the declared subject plus the
/// project's file set, so where the bytes came from cannot enter a digest.
pub trait ProjectStore {
    type Location: Clone + Ord + std::fmt::Display;

    /// The canonical location selected by `route`, relative to the project
    /// at `base`. Every route is located before reuse. Locations must
    /// distinguish selected revisions, subpaths, archive content, and
    /// registries; equivalent mirrors and registry aliases produce equal
    /// locations. Source references are validated here: request spelling
    /// alone is not proof of source identity.
    ///
    /// # Errors
    /// [`AcquireFailure`] when the store does not serve the route or cannot
    /// reach what it names.
    fn locate(
        &self,
        base: &Self::Location,
        route: &Route<'_>,
    ) -> Result<Self::Location, AcquireFailure>;

    /// The project file at `location`.
    ///
    /// # Errors
    /// Returns why the project file could not be read.
    fn project(&mut self, location: &Self::Location) -> Result<AcquiredProject, AcquireFailure>;

    /// One include the project at `location` names, by its project-relative
    /// path.
    ///
    /// # Errors
    /// Returns why the include could not be read.
    fn include(
        &mut self,
        location: &Self::Location,
        path: &str,
    ) -> Result<AcquiredSource, AcquireFailure>;

    /// Check the file set assembled for `location`, after its project file
    /// and every include were read. A store with a trust boundary compares
    /// the set against what it pinned; the default admits anything.
    ///
    /// # Errors
    /// Returns why the acquired file set is not what the store pinned.
    fn verify(&mut self, _location: &Self::Location) -> Result<(), AcquireFailure> {
        Ok(())
    }
}

/// The local filesystem: the store for `path` locators and for the root a
/// person names on the command line.
pub struct LocalFiles;

impl ProjectStore for LocalFiles {
    type Location = LocalDirectory;

    fn locate(
        &self,
        base: &Self::Location,
        route: &Route<'_>,
    ) -> Result<Self::Location, AcquireFailure> {
        let path = match route {
            Route::Path { path } | Route::Member { path } => *path,
            route => return Err(AcquireFailure::Unsupported { kind: route.kind() }),
        };
        let joined = base.0.join(path);
        fs::canonicalize(&joined)
            .map(LocalDirectory)
            .map_err(|error| AcquireFailure::unreadable("resolve", &joined, &error))
    }

    fn project(&mut self, location: &Self::Location) -> Result<AcquiredProject, AcquireFailure> {
        let path = location.0.join(PROJECT_NAME);
        require_regular(&path, "the project file")?;
        let text = fs::read_to_string(&path)
            .map_err(|error| AcquireFailure::unreadable("read the project file", &path, &error))?;
        Ok(AcquiredProject {
            // The project file's own path, so two projects both named
            // `pith.pi` stay distinguishable in a diagnostic.
            label: path.display().to_string().into(),
            text: text.into(),
        })
    }

    fn include(
        &mut self,
        location: &Self::Location,
        path: &str,
    ) -> Result<AcquiredSource, AcquireFailure> {
        let on_disk = location.0.join(path);
        read_regular(&on_disk, path)
    }
}

/// One named include, read as a regular file: a symlink or a special file
/// spelling an include is refused before the read.
fn read_regular(on_disk: &Path, path: &str) -> Result<AcquiredSource, AcquireFailure> {
    let metadata = fs::symlink_metadata(on_disk)
        .map_err(|error| AcquireFailure::unreadable("inspect", on_disk, &error))?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        return Err(AcquireFailure::Symlink {
            path: on_disk.display().to_string().into(),
        });
    }
    if !file_type.is_file() {
        return Err(AcquireFailure::Irregular {
            path: on_disk.display().to_string().into(),
        });
    }
    let text = fs::read_to_string(on_disk)
        .map_err(|error| AcquireFailure::unreadable("read the include", on_disk, &error))?;
    Ok(AcquiredSource {
        path: path.into(),
        text: text.into(),
    })
}

/// A canonical directory holding a project file.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalDirectory(PathBuf);

impl LocalDirectory {
    #[must_use]
    pub const fn new(directory: PathBuf) -> Self {
        Self(directory)
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl std::fmt::Display for LocalDirectory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0.display().to_string())
    }
}

/// A path about to be read must name a regular file: anything else (a
/// symlink, a fifo, a device) either hides an external tree or blocks the
/// read.
pub(crate) fn require_regular(path: &Path, what: &str) -> Result<(), AcquireFailure> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| AcquireFailure::unreadable("inspect", path, &error))?;
    if metadata.file_type().is_file() {
        return Ok(());
    }
    Err(AcquireFailure::Unreadable {
        message: format!(
            "`{}` is not a regular file, and {what} is read from one",
            path.display()
        )
        .into(),
    })
}
