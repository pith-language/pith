//! The boundary between acquiring a module and deciding what it means.
//!
//! [`ModuleStore`] is everything resolution needs from the world: where a
//! declared route leads, the manifest text there, and the source files that
//! location owns. [`super::graph`] holds the other half — membership,
//! identity, cycles — and it names no filesystem, so every validation
//! refusal is a function of what was acquired rather than of how.
//!
//! The split exists for the source kinds that do not exist yet. A registry,
//! a git revision, and an archive each reach the same resolution as a local
//! directory, with the same diagnostic file identity and the same frontend
//! inputs, by implementing this trait rather than by growing a second path
//! beside it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use pith_hir::{FrontendCode, ModuleSubject};

/// The manifest file name a module directory is named by.
pub const MANIFEST_NAME: &str = "module.pi";

/// The directory under which a module owns its source files.
pub const SOURCE_DIRECTORY: &str = "src";

/// The suffix of the source files a module owns.
pub const SOURCE_SUFFIX: &str = ".pi";

/// A manifest as acquired: the bytes, and the label its diagnostics carry.
pub struct AcquiredManifest {
    pub label: Box<str>,
    pub text: Box<str>,
}

/// One acquired source file: its module-relative path in forward slashes,
/// and its text.
pub struct AcquiredSource {
    pub path: Box<str>,
    pub text: Box<str>,
}

/// Why a store could not produce what a route asked for.
///
/// One failure type with a clause per cause, so a caller that must attach a
/// span picks the diagnostic from the clause rather than from a message it
/// has to interpret.
pub enum AcquireFailure {
    /// The route names a source kind this store does not serve.
    Unsupported { kind: &'static str },
    /// The location could not be reached, listed, or read.
    Unreadable { message: Box<str> },
    /// A symlink inside a module's tree. Following it would let a module
    /// silently own content its manifest does not claim.
    Symlink { path: Box<str> },
    /// Something that is not a regular file where one was required. Reading
    /// a fifo blocks, so this is refused before the read rather than after.
    Irregular { path: Box<str> },
    /// A store with its own trust boundaries refused the route: the code
    /// names which boundary, so the refusal lands as itself rather than
    /// as a generic unreadable source.
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

    /// The code this refusal lands as, when it names its own boundary;
    /// a caller with no better code falls back to its generic one.
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

    /// The failure as a sentence, for the caller that attaches the span:
    /// an acquisition failure carries no source of its own, because only
    /// the clause that reached for the file knows where to point.
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
                format!("`{path}` is a symlink, and a module owns its own source tree").into()
            }
            Self::Irregular { path } => format!(
                "`{path}` is not a regular file, and a module owns regular files under src/"
            )
            .into(),
            Self::Refused { message, .. } => message.clone(),
        }
    }
}

/// What resolution is asking a store to reach.
///
/// The split is the registry's requirement: a `use` clause selects a
/// subject and then names a route, and a registry serves by subject — the
/// route alone names nothing there — while a workspace member is a
/// directory whose subject its own manifest declares. A store that cannot
/// serve a variant says so; there is no fallback route.
pub enum Route<'a> {
    Registry(super::routing::RegistryRoute<'a>),
    Path {
        subject: &'a ModuleSubject,
        path: &'a str,
    },
    Git {
        subject: &'a ModuleSubject,
        url: &'a str,
        revision: &'a str,
        subpath: Option<&'a str>,
    },
    Archive {
        subject: &'a ModuleSubject,
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

/// Where a module's bytes come from.
///
/// A store's `Location` is its own canonical name for a module: two routes
/// reaching one module produce equal locations, which is what lets
/// resolution detect a diamond and a cycle without knowing what a route is.
/// A location never becomes a semantic key — under 0067 a module's identity
/// is its declared subject and its module-relative source set, so where the
/// bytes came from cannot enter a digest.
pub trait ModuleStore {
    type Location: Clone + Ord + std::fmt::Display;

    /// The canonical source selected by `route`, relative to the manifest at `base`.
    /// Every route is located before reuse. Locations must distinguish selected
    /// revisions, subpaths, archive content, and registries; equivalent mirrors
    /// and registry aliases produce equal locations. The adapter validates source
    /// references here: request spelling alone is not proof of source identity.
    ///
    /// # Errors
    /// Returns [`AcquireFailure::Unsupported`] for a route this store does
    /// not serve, or [`AcquireFailure::Unreadable`] when the route names
    /// nothing this store can reach.
    fn locate(
        &self,
        base: &Self::Location,
        route: &Route<'_>,
    ) -> Result<Self::Location, AcquireFailure>;

    /// The manifest at `location`.
    ///
    /// # Errors
    /// Returns why the manifest could not be read.
    fn manifest(&mut self, location: &Self::Location) -> Result<AcquiredManifest, AcquireFailure>;

    /// The source files `location` owns, in canonical module-relative
    /// order. An empty result is not a failure here: whether a module may
    /// own nothing is a validation question.
    ///
    /// # Errors
    /// Returns why the source set could not be listed or read.
    fn sources(&mut self, location: &Self::Location)
    -> Result<Vec<AcquiredSource>, AcquireFailure>;
}

/// The local filesystem: the store for `from path` routes and for the root
/// a person names on the command line.
pub struct LocalFiles;

impl ModuleStore for LocalFiles {
    type Location = LocalDirectory;

    fn locate(
        &self,
        base: &Self::Location,
        route: &Route<'_>,
    ) -> Result<Self::Location, AcquireFailure> {
        let path = match route {
            Route::Path { path, .. } | Route::Member { path } => *path,
            route => return Err(AcquireFailure::Unsupported { kind: route.kind() }),
        };
        let joined = base.0.join(path);
        fs::canonicalize(&joined)
            .map(LocalDirectory)
            .map_err(|error| AcquireFailure::unreadable("resolve", &joined, &error))
    }

    fn manifest(&mut self, location: &Self::Location) -> Result<AcquiredManifest, AcquireFailure> {
        let path = location.0.join(MANIFEST_NAME);
        require_regular(&path, "the manifest")?;
        let text = fs::read_to_string(&path)
            .map_err(|error| AcquireFailure::unreadable("read the manifest", &path, &error))?;
        Ok(AcquiredManifest {
            // The manifest's own path, so two members both named `module.pi`
            // stay distinguishable in a diagnostic.
            label: path.display().to_string().into(),
            text: text.into(),
        })
    }

    fn sources(
        &mut self,
        location: &Self::Location,
    ) -> Result<Vec<AcquiredSource>, AcquireFailure> {
        let discovered = discover(&location.0)?;
        discovered
            .into_iter()
            .map(|(path, on_disk)| {
                fs::read_to_string(&on_disk)
                    .map(|text| AcquiredSource {
                        path,
                        text: text.into(),
                    })
                    .map_err(|error| {
                        AcquireFailure::unreadable("read the source file", &on_disk, &error)
                    })
            })
            .collect()
    }
}

/// A canonical directory holding a `module.pi`.
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

/// Every regular `.pi` file under a regular `src/`, recursively, keyed by
/// module-relative path in forward slashes and so sorted canonically before
/// any parse.
pub(crate) fn discover(directory: &Path) -> Result<BTreeMap<Box<str>, PathBuf>, AcquireFailure> {
    let mut files = BTreeMap::new();
    let source_root = directory.join(SOURCE_DIRECTORY);
    let metadata = match fs::symlink_metadata(&source_root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(files),
        Err(error) => {
            return Err(AcquireFailure::unreadable(
                "inspect the source directory",
                &source_root,
                &error,
            ));
        }
    };
    let root_type = metadata.file_type();
    if root_type.is_symlink() {
        return Err(symlink(&source_root));
    }
    if !root_type.is_dir() {
        return Err(irregular(&source_root));
    }
    walk(&source_root, &format!("{SOURCE_DIRECTORY}/"), &mut files)?;
    Ok(files)
}

fn walk(
    directory: &Path,
    prefix: &str,
    files: &mut BTreeMap<Box<str>, PathBuf>,
) -> Result<(), AcquireFailure> {
    let entries = fs::read_dir(directory)
        .map_err(|error| AcquireFailure::unreadable("list", directory, &error))?;
    for entry in entries {
        let entry = entry.map_err(|error| AcquireFailure::unreadable("list", directory, &error))?;
        let file_type = entry
            .file_type()
            .map_err(|error| AcquireFailure::unreadable("inspect", &entry.path(), &error))?;
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if file_type.is_symlink() {
            return Err(symlink(&entry.path()));
        }
        if file_type.is_dir() {
            walk(&entry.path(), &format!("{prefix}{name}/"), files)?;
        } else if file_type.is_file() {
            if name.ends_with(SOURCE_SUFFIX) {
                files.insert(format!("{prefix}{name}").into(), entry.path());
            }
        } else if name.ends_with(SOURCE_SUFFIX) {
            // Not a directory, not a symlink, not a regular file: a fifo, a
            // socket, or a device spelling a source name.
            return Err(irregular(&entry.path()));
        }
    }
    Ok(())
}

/// A path about to be read must name a regular file. Anything else — a
/// symlink, a fifo, a device — either hides an external tree or blocks the
/// read; neither is a manifest.
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

fn symlink(path: &Path) -> AcquireFailure {
    AcquireFailure::Symlink {
        path: path.display().to_string().into(),
    }
}

fn irregular(path: &Path) -> AcquireFailure {
    AcquireFailure::Irregular {
        path: path.display().to_string().into(),
    }
}
