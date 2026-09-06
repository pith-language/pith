//! Publication: derive an entry from the module it releases.
//!
//! Every cached field of an entry — the subject, the version, the
//! requirements — is read from the module's own manifest, never authored,
//! so an entry that disagrees with the revision it pins can only be a bug
//! in whoever signed it. A module holding a path dependency is refused
//! here: a path is a live local input, and a release naming one would
//! claim requirements no consumer can satisfy.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use pith_diag::{Diag, Severity, SourceFile, SourceId, Span};
use pith_hir::{FrontendCode, Manifest, ManifestVersion, ModuleSubject};

use super::keys::Signing;
use super::line::{Pin, Release};
use super::tree::{Measured, measure};
use crate::{AcquiredSource, LocalDirectory, LocalFiles, ModuleRequirement, ModuleStore};

/// A release waiting on a signature: everything an entry caches, derived
/// from one module directory, with the measured tree that names the
/// revision.
pub struct Draft {
    subject: ModuleSubject,
    version: ManifestVersion,
    requires: Box<[ModuleRequirement]>,
    measured: Measured,
    manifest: Box<str>,
    sources: BTreeMap<Box<str>, Box<str>>,
}

impl Draft {
    #[must_use]
    pub const fn subject(&self) -> &ModuleSubject {
        &self.subject
    }

    #[must_use]
    pub const fn version(&self) -> &ManifestVersion {
        &self.version
    }

    #[must_use]
    pub fn requires(&self) -> &[ModuleRequirement] {
        &self.requires
    }

    #[must_use]
    pub const fn measured(&self) -> Measured {
        self.measured
    }

    /// The module's manifest text, as acquired.
    #[must_use]
    pub fn manifest(&self) -> &str {
        &self.manifest
    }

    /// The module's source files in canonical module-relative order.
    #[must_use]
    pub fn sources(&self) -> &BTreeMap<Box<str>, Box<str>> {
        &self.sources
    }

    /// The release this draft becomes, signed by `publisher`, admitted at
    /// `admitted`.
    #[must_use]
    pub fn release(&self, publisher: &Signing, admitted: u64) -> Release {
        Release::signed(
            self.version.clone(),
            self.requires.clone(),
            Pin::new(self.measured.revision()),
            self.measured.digest(),
            publisher,
            admitted,
        )
    }
}

/// Derives a release draft from the module rooted at `directory`.
///
/// # Errors
/// Returns why the directory is not a publishable module: it cannot be
/// read, its manifest does not parse, or it holds a path dependency.
pub fn derive(directory: &Path) -> Result<Draft, Box<[Diag]>> {
    let canonical = std::fs::canonicalize(directory).map_err(|error| {
        Box::from([sourceless(
            FrontendCode::UnreadableSource,
            format!(
                "cannot resolve the module directory `{}`: {error}",
                directory.display()
            ),
        )])
    })?;
    let location = LocalDirectory::new(canonical);
    let mut store = LocalFiles;
    let acquired = store.manifest(&location).map_err(|failure| {
        Box::from([sourceless(
            FrontendCode::UnreadableSource,
            failure.describe(),
        )])
    })?;
    let sources = store.sources(&location).map_err(|failure| {
        Box::from([sourceless(
            FrontendCode::UnreadableSource,
            failure.describe(),
        )])
    })?;
    let (manifest, source) = parse_manifest(&acquired.label, &acquired.text)?;
    let refusals = path_dependencies(&manifest, &source);
    if !refusals.is_empty() {
        return Err(refusals.into());
    }
    let sources = source_map(&sources);
    Ok(Draft {
        subject: manifest.subject().clone(),
        version: manifest.version().clone(),
        requires: manifest
            .uses()
            .iter()
            .map(|use_| ModuleRequirement {
                subject: use_.subject.clone(),
                range: use_.range.clone(),
            })
            .collect(),
        measured: measure(&acquired.text, &sources),
        manifest: acquired.text,
        sources,
    })
}

/// The manifest of a module about to be published, with the source its
/// refusals attach to.
///
/// # Errors
/// Returns the parse's diagnostics.
fn parse_manifest(label: &str, text: &str) -> Result<(Manifest, Arc<SourceFile>), Box<[Diag]>> {
    let source = Arc::new(SourceFile::new(
        SourceId::from_raw(0),
        label.to_string(),
        text.to_string(),
    ));
    let parsed = crate::source::parse_manifest(&crate::source::ManifestSource::new(
        SourceId::from_raw(0),
        label.to_string(),
        text.to_string(),
    ));
    match parsed.validated() {
        Ok(manifest) => Ok((manifest, source)),
        Err(_) => Err(parsed.diagnostics().to_vec().into()),
    }
}

fn path_dependencies(manifest: &Manifest, source: &Arc<SourceFile>) -> Vec<Diag> {
    manifest
        .uses()
        .iter()
        .filter(|use_| use_.path().is_some())
        .map(|use_| {
            Diag::new(
                Severity::Error,
                FrontendCode::UnpublishablePath.stable(),
                use_.source_span(),
                format!(
                    "the dependency `{}` comes from a path, and a release cannot carry one: a \
                     path is a live local input, not witnessed content",
                    use_.alias
                ),
            )
            .with_source(Arc::clone(source))
        })
        .collect()
}

fn source_map(sources: &[AcquiredSource]) -> BTreeMap<Box<str>, Box<str>> {
    sources
        .iter()
        .map(|source| (source.path.clone(), source.text.clone()))
        .collect()
}

fn sourceless(code: FrontendCode, message: impl Into<Box<str>>) -> Diag {
    Diag::new(Severity::Error, code.stable(), Span::none(), message)
}
