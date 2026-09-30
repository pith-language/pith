//! The registry as a project store: a verified index in, admitted file
//! sets out. It serves exact selections (the one release the index carries
//! for a subject); choosing among versions is the resolver's computation.
//! A project that disagrees with its cached entry, or bytes that miss the
//! pinned tree, are refused here, not discovered by elaboration.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pith_hir::{FrontendCode, ModuleSubject, RootKey};

use super::SOURCES_DIRECTORY;
use super::line::Release;
use super::trust::{self, Record, Verified};
use crate::workspace::acquire::{
    self, AcquireFailure, AcquiredProject, AcquiredSource, ProjectStore, Route,
};

/// A verified index serving modules from the directory that hosts it.
pub struct RegistryStore {
    directory: PathBuf,
    verified: Verified,
    /// Each located release's project text and includes, kept for the tree
    /// check once the file set is assembled.
    file_sets: BTreeMap<Location, FileSet>,
}

/// A located release's acquired file set, measured against the entry's pin
/// once complete.
#[derive(Default)]
struct FileSet {
    project: Option<Box<str>>,
    includes: BTreeMap<Box<str>, Box<str>>,
}

/// Where a registry route arrives: subject, selected version, pinned
/// revision, admitting root key. Two routes to one registry must spell
/// this the same, so no local configuration name enters it.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Location {
    root: Box<str>,
    subject: ModuleSubject,
    version: Box<str>,
    revision: Box<str>,
}

impl std::fmt::Display for Location {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "registry/{}@{} rooted {}",
            self.subject, self.version, self.root
        )
    }
}

impl RegistryStore {
    /// Reads and verifies the index at `directory` under `pinned`,
    /// against the state `prior` admits.
    ///
    /// # Errors
    /// Every refusal the index earned; nothing is served from an index
    /// that failed any check.
    pub fn open(
        directory: PathBuf,
        pinned: &RootKey,
        prior: &Record,
    ) -> Result<(Self, Record), Box<[pith_diag::Diag]>> {
        let (verified, record) = trust::read(&directory, pinned, prior)?;
        Ok((
            Self {
                directory,
                verified,
                file_sets: BTreeMap::new(),
            },
            record,
        ))
    }

    /// The location of the one release the index carries for `subject`:
    /// the entry point for a root no route selected and whose key no
    /// clause pinned.
    ///
    /// # Errors
    /// Why `subject` names nothing this store serves exactly.
    pub fn locate_subject(&self, subject: &ModuleSubject) -> Result<Location, AcquireFailure> {
        let release = self.checked_selection(subject)?;
        Ok(self.location_of(subject, release))
    }

    /// The one release this index carries for `subject`, after the
    /// withdrawal and pin checks every selection passes.
    fn checked_selection(&self, subject: &ModuleSubject) -> Result<&Release, AcquireFailure> {
        let release = self.selection(subject)?;
        if let Some(withdrawal) = self
            .verified
            .subject(subject)
            .and_then(|admitted| admitted.withdrawal_for(release.version()))
        {
            return Err(refused(
                FrontendCode::WithdrawnSelection,
                format!(
                    "the release {subject} {} is withdrawn by `{}` — {}",
                    release.version().canonical_spelling(),
                    withdrawal.signer().spelling(),
                    withdrawal.reason()
                ),
            ));
        }
        if release.pin().subpath().is_some() {
            return Err(refused(
                FrontendCode::UnsupportedSource,
                format!(
                    "the release of {subject} pins a subpath, and this host serves module \
                     roots at their revision roots"
                ),
            ));
        }
        Ok(release)
    }

    /// The one release this index carries for `subject`.
    fn selection(&self, subject: &ModuleSubject) -> Result<&Release, AcquireFailure> {
        let admitted =
            self.verified
                .subject(subject)
                .ok_or_else(|| AcquireFailure::Unreadable {
                    message: format!("the index carries no release of {subject}").into(),
                })?;
        let releases = admitted.releases().collect::<Vec<_>>();
        match releases.as_slice() {
            [only] => Ok(only),
            [] => Err(AcquireFailure::Unreadable {
                message: format!("the index carries no release of {subject}").into(),
            }),
            _ => Err(refused(
                FrontendCode::UnsupportedSource,
                format!(
                    "the index carries {} releases of {subject}, and selecting among versions \
                     is the resolver's computation: this store serves exact selections",
                    releases.len()
                ),
            )),
        }
    }

    fn location_of(&self, subject: &ModuleSubject, release: &Release) -> Location {
        Location {
            root: self.verified.root().spelling(),
            subject: subject.clone(),
            version: release.version().canonical_spelling(),
            revision: release.pin().revision().into(),
        }
    }

    /// The release a location names, when it still names one.
    fn release(&self, location: &Location) -> Option<&Release> {
        let admitted = self.verified.subject(&location.subject)?;
        admitted
            .releases()
            .find(|release| release.version().canonical_spelling() == location.version)
    }

    fn sources_directory(&self, location: &Location) -> PathBuf {
        self.directory
            .join(SOURCES_DIRECTORY)
            .join(location.revision.as_ref())
    }
}

impl ProjectStore for RegistryStore {
    type Location = Location;

    fn locate(
        &self,
        _base: &Self::Location,
        route: &Route<'_>,
    ) -> Result<Self::Location, AcquireFailure> {
        let registry = match route {
            Route::Registry(registry) => registry,
            route => return Err(AcquireFailure::Unsupported { kind: route.kind() }),
        };
        let subject = registry.subject().clone();
        if registry.root_key().to_string() != self.verified.root().spelling().as_ref() {
            return Err(refused(
                FrontendCode::UnsignedKeySet,
                format!(
                    "the route pins the root key `{}`, and this index serves `{}`",
                    registry.root_key(),
                    self.verified.root().spelling()
                ),
            ));
        }
        let release = self.checked_selection(&subject)?;
        Ok(self.location_of(&subject, release))
    }

    fn project(&mut self, location: &Self::Location) -> Result<AcquiredProject, AcquireFailure> {
        let release = self
            .release(location)
            .ok_or_else(|| AcquireFailure::Unreadable {
                message: format!("the index no longer carries {location}").into(),
            })?;
        let path = self.sources_directory(location).join(acquire::PROJECT_NAME);
        acquire::require_regular(&path, "the project file")?;
        let text = std::fs::read_to_string(&path)
            .map_err(|error| AcquireFailure::unreadable("read the project file", &path, &error))?;
        check_cached_fields(release, &text, location)?;
        self.file_sets.entry(location.clone()).or_default().project = Some(text.clone().into());
        Ok(AcquiredProject {
            label: format!("{location}/{project}", project = acquire::PROJECT_NAME).into(),
            text: text.into(),
        })
    }

    fn include(
        &mut self,
        location: &Self::Location,
        path: &str,
    ) -> Result<AcquiredSource, AcquireFailure> {
        let directory = self.sources_directory(location);
        let on_disk = acquire::resolve_within(&directory, path, acquire::Target::File)?;
        let text = std::fs::read_to_string(&on_disk)
            .map_err(|error| AcquireFailure::unreadable("read the include", &on_disk, &error))?;
        self.file_sets
            .entry(location.clone())
            .or_default()
            .includes
            .insert(path.into(), text.clone().into());
        Ok(AcquiredSource {
            path: path.into(),
            text: text.into(),
        })
    }

    fn verify(&mut self, location: &Self::Location) -> Result<(), AcquireFailure> {
        let release = self
            .release(location)
            .ok_or_else(|| AcquireFailure::Unreadable {
                message: format!("the index no longer carries {location}").into(),
            })?;
        let Some(project) = self
            .file_sets
            .get(location)
            .and_then(|set| set.project.as_ref())
        else {
            return Err(AcquireFailure::Unreadable {
                message: format!("the project file at {location} was never read").into(),
            });
        };
        let includes = self
            .file_sets
            .get(location)
            .map(|set| set.includes.clone())
            .unwrap_or_default();
        let measured = super::measure(project, &includes);
        if measured.digest() != release.tree() {
            return Err(refused(
                FrontendCode::ContentMismatch,
                format!(
                    "the tree at revision {} measures {}, and the entry for {location} pins {}",
                    release.pin().revision(),
                    measured.digest(),
                    release.tree()
                ),
            ));
        }
        Ok(())
    }
}

/// The project a revision served, checked field by field against the
/// entry's cached fields. A disagreement is a wrong cache, named with
/// both sides, never reconciled.
fn check_cached_fields(
    release: &Release,
    text: &str,
    location: &Location,
) -> Result<(), AcquireFailure> {
    let parsed = crate::source::parse_project_file(&crate::source::ProjectSource::new(
        pith_diag::SourceId::from_raw(0),
        format!("{location}/{}", acquire::PROJECT_NAME),
        text.to_string(),
    ));
    let Ok(project) = parsed.validated() else {
        // An unparsable project file is reported by the loader under the
        // store's own file identity; the cache check has nothing to compare
        // against.
        return Ok(());
    };
    if project.subject() != &location.subject {
        return Err(refused(
            FrontendCode::WrongIndexCache,
            format!(
                "the entry for {location} caches the subject {}, and the project at its \
                 revision declares {}: the cache is wrong",
                location.subject,
                project.subject()
            ),
        ));
    }
    if project.version() != release.version() {
        return Err(refused(
            FrontendCode::WrongIndexCache,
            format!(
                "the entry for {location} caches the version {}, and the project at its \
                 revision declares {}: the cache is wrong",
                release.version().canonical_spelling(),
                project.version().canonical_spelling()
            ),
        ));
    }
    let declared = project
        .inputs()
        .iter()
        .filter_map(|input| match &input.locator {
            pith_hir::InputLocator::Registry { subject, range, .. } => {
                Some((subject.spelling(), range.to_string()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let cached = release
        .requires()
        .iter()
        .map(|requirement| {
            (
                requirement.subject.spelling(),
                requirement.range.to_string(),
            )
        })
        .collect::<Vec<_>>();
    if declared != cached {
        return Err(refused(
            FrontendCode::WrongIndexCache,
            format!(
                "the entry for {location} caches the requirements [{}], and the project at \
                 its revision declares [{}]: the cache is wrong",
                cached
                    .iter()
                    .map(|(subject, range)| format!("{subject} {range}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                declared
                    .iter()
                    .map(|(subject, range)| format!("{subject} {range}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    Ok(())
}

fn refused(code: FrontendCode, message: impl Into<Box<str>>) -> AcquireFailure {
    AcquireFailure::Refused {
        code,
        message: message.into(),
    }
}
