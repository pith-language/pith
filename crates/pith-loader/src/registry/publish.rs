//! Publication: derive an entry from the project it releases.
//!
//! Every cached field is read from the project's header, never authored.
//! A project holding an input this registry cannot serve is refused: a
//! path makes no witnessed-content claim to publish, and a git or archive
//! route names content the index does not carry.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use pith_diag::{Diag, Severity, SourceFile};
use pith_hir::{FrontendCode, InputLocator, ModuleSubject, ModuleVersion};

use super::keys::Signing;
use super::line::{Pin, Release};
use super::{Measured, measure};
use crate::workspace::graph::sourceless;
use crate::{AcquiredProject, LocalDirectory, LocalFiles, ModuleRequirement, ProjectStore};

/// A release waiting on a signature: everything an entry caches, derived
/// from one project directory, with the measured tree that names the
/// revision.
pub struct Draft {
    subject: ModuleSubject,
    version: ModuleVersion,
    requires: Box<[ModuleRequirement]>,
    measured: Measured,
    project: Box<str>,
    includes: BTreeMap<Box<str>, Box<str>>,
}

impl Draft {
    #[must_use]
    pub const fn subject(&self) -> &ModuleSubject {
        &self.subject
    }

    #[must_use]
    pub const fn version(&self) -> &ModuleVersion {
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

    /// The project file's text, as acquired.
    #[must_use]
    pub fn project(&self) -> &str {
        &self.project
    }

    /// The project's includes in canonical order.
    #[must_use]
    pub fn includes(&self) -> &BTreeMap<Box<str>, Box<str>> {
        &self.includes
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

/// Derives a release draft from the project rooted at `directory`.
///
/// # Errors
/// Returns why the directory is not a publishable project: it cannot be
/// read, its project file does not parse, or it holds an input this
/// registry cannot serve.
pub fn derive(directory: &Path) -> Result<Draft, Box<[Diag]>> {
    let canonical = std::fs::canonicalize(directory).map_err(|error| {
        Box::from([sourceless(
            FrontendCode::UnreadableSource,
            format!(
                "cannot resolve the project directory `{}`: {error}",
                directory.display()
            ),
        )])
    })?;
    let location = LocalDirectory::new(canonical);
    let mut store = LocalFiles;
    let acquired = store.project(&location).map_err(|failure| {
        Box::from([sourceless(
            FrontendCode::UnreadableSource,
            failure.describe(),
        )])
    })?;
    let (project, source) = parse_project(&acquired)?;
    let refusals = unservable_inputs(&project, &source);
    if !refusals.is_empty() {
        return Err(refusals.into());
    }
    let mut includes = BTreeMap::new();
    for include in project.includes() {
        let acquired = store.include(&location, &include.path).map_err(|failure| {
            Box::from([sourceless(
                FrontendCode::UnreadableSource,
                failure.describe(),
            )])
        })?;
        includes.insert(acquired.path, acquired.text);
    }
    Ok(Draft {
        subject: project.subject().clone(),
        version: project.version().clone(),
        requires: project
            .inputs()
            .iter()
            .filter_map(|input| match &input.locator {
                InputLocator::Registry { subject, range, .. } => Some(ModuleRequirement {
                    subject: subject.clone(),
                    range: range.clone(),
                }),
                _ => None,
            })
            .collect(),
        measured: measure(&acquired.text, &includes),
        project: acquired.text,
        includes,
    })
}

/// The project of a module about to be published, with the source its
/// refusals attach to.
///
/// # Errors
/// Returns the parse's diagnostics.
fn parse_project(
    acquired: &AcquiredProject,
) -> Result<(pith_hir::Project, Arc<SourceFile>), Box<[Diag]>> {
    let parsed = crate::source::parse_project_file(&crate::source::ProjectSource::new(
        pith_diag::SourceId::from_raw(0),
        acquired.label.as_ref(),
        acquired.text.as_ref(),
    ));
    match parsed.validated() {
        Ok(project) => Ok((project, Arc::clone(parsed.source()))),
        Err(_) => Err(parsed.diagnostics().to_vec().into()),
    }
}

fn unservable_inputs(project: &pith_hir::Project, source: &Arc<SourceFile>) -> Vec<Diag> {
    project
        .inputs()
        .iter()
        .filter(|input| !input.locator.resolves_versions())
        .map(|input| {
            Diag::new(
                Severity::Error,
                FrontendCode::UnpublishablePath.stable(),
                input.locator.span(),
                format!(
                    "the input `{}` comes from a {} route, and a release resolves its inputs \
                     through the registry: a {} route is a live or unwitnessed reference, not \
                     witnessed content",
                    input.name,
                    input.locator.kind(),
                    input.locator.kind()
                ),
            )
            .with_source(Arc::clone(source))
        })
        .collect()
}
