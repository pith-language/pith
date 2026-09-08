//! Validation and resolution over the acquired manifests: membership,
//! identity, cycles, and source sets. Every refusal here names the clause
//! that caused it, in the manifest that caused it; this is what separates
//! this half from [`super::acquire`], where failures carry no span of their
//! own and are attached by the caller that reached for the file.

mod bootstrap;
mod claims;
mod members;
mod sources;
mod walk;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use pith_diag::{Diag, Severity, SourceFile, Span};
use pith_hir::{FrontendCode, Manifest, ModuleSubject};

use super::acquire::{AcquireFailure, ModuleStore};
use super::module::{ResolvedModule, SourceSet};
use crate::source::{ManifestSource, ParsedManifestFile};

/// A parsed manifest shared by every route reaching it.
pub(super) struct ManifestFile {
    pub(super) parsed: ParsedManifestFile,
}

/// A module mid-resolution: manifest read and validated, source set not yet
/// acquired.
pub(super) struct PendingModule<L> {
    pub(super) location: L,
    pub(super) file: Arc<ManifestFile>,
    pub(super) view: Manifest,
    pub(super) sources: SourceSet,
}

/// The resolution state: which locations were read, which subjects they
/// declared, and the dependency route currently being walked.
pub(super) struct Resolution<S: ModuleStore> {
    pub(super) store: S,
    pub(super) routing: super::routing::Routing,
    claims: claims::Claims<S::Location>,
    /// Emitted in dependency order: every dependency precedes its consumer.
    pub(super) modules: Vec<PendingModule<S::Location>>,
    pub(super) by_location: BTreeMap<S::Location, usize>,
    pub(super) subjects: BTreeMap<ModuleSubject, S::Location>,
    /// The route from the root, as subjects, for cycle chains; the
    /// locations mirror it for membership.
    pub(super) stack: Vec<ModuleSubject>,
    pub(super) stack_locations: BTreeSet<S::Location>,
    /// Every manifest read so far, member or dependency, by location, so a
    /// diamond reads one.
    pub(super) read: BTreeMap<S::Location, Arc<ManifestFile>>,
    pub(super) next_source_id: u32,
    pub(super) diagnostics: Vec<Diag>,
}

impl<S: ModuleStore> Resolution<S> {
    /// Read and parse a directory's manifest. Parse diagnostics join the
    /// resolution's; the read failure is the caller's to attach, because
    /// only the caller knows which clause reached for the file.
    pub(super) fn read(
        &mut self,
        location: &S::Location,
    ) -> Result<Arc<ManifestFile>, AcquireFailure> {
        if let Some(read) = self.read.get(location) {
            return Ok(Arc::clone(read));
        }
        let acquired = self.store.manifest(location)?;
        let source_id = pith_diag::SourceId::from_raw(self.next_source_id);
        self.next_source_id = self.next_source_id.saturating_add(1);
        let file = Arc::new(ManifestFile {
            parsed: crate::source::parse_manifest(&ManifestSource::new(
                source_id,
                acquired.label,
                acquired.text,
            )),
        });
        self.diagnostics
            .extend(file.parsed.diagnostics().iter().cloned());
        self.read.insert(location.clone(), Arc::clone(&file));
        Ok(file)
    }

    pub(super) fn finish(self) -> Result<super::WorkspaceResolution, Box<[Diag]>> {
        if self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
        {
            return Err(self.diagnostics.into());
        }
        let modules = self
            .modules
            .into_iter()
            .map(|module| ResolvedModule {
                subject: module.view.subject().clone(),
                origin: module.location.to_string().into(),
                manifest: module.view,
                sources: module.sources,
            })
            .collect::<Vec<_>>();
        let workspace = super::Closure::from_dependency_order(modules).ok_or_else(|| {
            Box::from([sourceless(
                FrontendCode::MissingManifest,
                "the resolution reached no root manifest",
            )])
        })?;
        Ok(super::WorkspaceResolution {
            workspace,
            overrides: self.routing.into_overrides(),
        })
    }
}

/// The validated view of a read manifest, when its parse allows one. Both
/// failure halves are already diagnosed by the parse, so there is no second
/// diagnostic to push here.
pub(super) fn view_of(file: &ManifestFile) -> Option<Manifest> {
    file.parsed.validated().ok()
}

/// A diagnostic that carries no source, because nothing was read.
pub(crate) fn sourceless(code: FrontendCode, message: impl Into<Box<str>>) -> Diag {
    Diag::new(Severity::Error, code.stable(), Span::none(), message)
}

/// A diagnostic attached to the manifest that caused it.
pub(crate) fn at(
    code: FrontendCode,
    span: Span,
    source: &Arc<SourceFile>,
    message: impl Into<Box<str>>,
) -> Diag {
    Diag::new(Severity::Error, code.stable(), span, message).with_source(source.clone())
}
