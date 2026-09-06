//! The registry as a module store: a verified index in, admitted source
//! sets out.
//!
//! This store serves exact selections — the one release the index carries
//! for a subject — because choosing among versions is the resolver's
//! computation, not a store's. Around [`super::trust`] it adds the content
//! half of the model: the bytes a revision names are read, normalized,
//! and measured against the digest the entry pins, and the manifest those
//! bytes carry is checked field by field against what the entry caches,
//! so a wrong cache and a wrong tree are refused at their own boundaries
//! rather than discovered by elaboration.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pith_hir::{FrontendCode, ModuleSubject, RootKey};

use super::SOURCES_DIRECTORY;
use super::line::Release;
use super::trust::{self, Record, Verified};
use crate::workspace::acquire::{
    self, AcquireFailure, AcquiredManifest, AcquiredSource, ModuleStore, Route,
};

/// A verified index serving modules from the directory that hosts it.
pub struct RegistryStore {
    directory: PathBuf,
    verified: Verified,
    /// Each located release's manifest text, kept for the tree check when
    /// its sources arrive.
    manifests: BTreeMap<Location, Box<str>>,
}

/// Where a registry route arrives: the subject, its selected version, the
/// revision the entry pins, and the root key whose trust chain admitted
/// it — the spelling two routes to one registry must agree on, with the
/// local configuration name absent.
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
    /// Returns every refusal the index earned; nothing is served from an
    /// index that failed any check.
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
                manifests: BTreeMap::new(),
            },
            record,
        ))
    }

    /// The location of the one release the index carries for `subject`:
    /// the entry point for a root this registry serves, which no route
    /// selected and whose root key no clause pinned.
    ///
    /// # Errors
    /// Returns why `subject` names nothing this store serves exactly.
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

impl ModuleStore for RegistryStore {
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

    fn manifest(&mut self, location: &Self::Location) -> Result<AcquiredManifest, AcquireFailure> {
        let release = self
            .release(location)
            .ok_or_else(|| AcquireFailure::Unreadable {
                message: format!("the index no longer carries {location}").into(),
            })?;
        let path = self
            .sources_directory(location)
            .join(acquire::MANIFEST_NAME);
        acquire::require_regular(&path, "the manifest")?;
        let text = std::fs::read_to_string(&path)
            .map_err(|error| AcquireFailure::unreadable("read the manifest", &path, &error))?;
        check_cached_fields(release, &text, location)?;
        self.manifests.insert(location.clone(), text.clone().into());
        Ok(AcquiredManifest {
            label: format!("{location}/module.pi").into(),
            text: text.into(),
        })
    }

    fn sources(
        &mut self,
        location: &Self::Location,
    ) -> Result<Vec<AcquiredSource>, AcquireFailure> {
        let release = self
            .release(location)
            .ok_or_else(|| AcquireFailure::Unreadable {
                message: format!("the index no longer carries {location}").into(),
            })?;
        let directory = self.sources_directory(location);
        let discovered = acquire::discover(&directory)?;
        let mut sources = Vec::new();
        let mut measured = std::collections::BTreeMap::new();
        for (path, on_disk) in discovered {
            let text = std::fs::read_to_string(&on_disk).map_err(|error| {
                AcquireFailure::unreadable("read the source file", &on_disk, &error)
            })?;
            measured.insert(path.clone(), text.clone().into());
            sources.push(AcquiredSource {
                path,
                text: text.into(),
            });
        }
        let manifest = self
            .manifests
            .get(location)
            .ok_or_else(|| AcquireFailure::Unreadable {
                message: format!("the manifest at {location} was never read").into(),
            })?;
        let measured = super::tree::measure(manifest, &measured);
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
        Ok(sources)
    }
}

/// The manifest a revision served, checked field by field against the
/// fields the entry caches. A disagreement is a wrong cache, named with
/// both sides, never reconciled.
fn check_cached_fields(
    release: &Release,
    text: &str,
    location: &Location,
) -> Result<(), AcquireFailure> {
    let parsed = crate::source::parse_manifest(&crate::source::ManifestSource::new(
        pith_diag::SourceId::from_raw(0),
        format!("{location}/module.pi"),
        text.to_string(),
    ));
    let Ok(manifest) = parsed.validated() else {
        // An unparsable manifest is reported by the loader under the
        // store's own file identity; the cache check has nothing to
        // compare against.
        return Ok(());
    };
    if manifest.subject() != &location.subject {
        return Err(refused(
            FrontendCode::WrongIndexCache,
            format!(
                "the entry for {location} caches the subject {}, and the manifest at its \
                 revision declares {}: the cache is wrong",
                location.subject,
                manifest.subject()
            ),
        ));
    }
    if manifest.version() != release.version() {
        return Err(refused(
            FrontendCode::WrongIndexCache,
            format!(
                "the entry for {location} caches the version {}, and the manifest at its \
                 revision declares {}: the cache is wrong",
                release.version().canonical_spelling(),
                manifest.version().canonical_spelling()
            ),
        ));
    }
    let declared = manifest
        .uses()
        .iter()
        .map(|use_| (use_.subject.spelling(), use_.range.to_string()))
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
                "the entry for {location} caches the requirements [{}], and the manifest at \
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
