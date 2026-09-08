//! The acquisition path that formats a manifest: the root manifest and its
//! own source files, and nothing else.

use std::path::{Path, PathBuf};

use pith_diag::{Diag, Severity, Span};
use pith_hir::{FrontendCode, ModuleSubject};

use super::acquire::{AcquireFailure, LocalDirectory, LocalFiles, ModuleStore};
use super::module::{ModuleFile, SourceSet};

/// The files `fmt` writes: the root manifest and the root module's own
/// source set. Nothing else is read, since their canonical spelling is a
/// function of these files alone.
pub struct RootFiles {
    directory: PathBuf,
    subject: ModuleSubject,
    sources: SourceSet,
}

impl RootFiles {
    #[must_use]
    pub fn subject(&self) -> &ModuleSubject {
        &self.subject
    }

    /// The canonical directory the manifest was read from; each source
    /// file's write path is this directory joined with its module-relative
    /// path.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    #[must_use]
    pub fn sources(&self) -> &SourceSet {
        &self.sources
    }

    /// Acquire the root manifest and its own source files.
    ///
    /// # Errors
    /// The manifest's parse diagnostics, and acquisition refusals for its
    /// source tree (symlinked, irregular, or unreadable files), each
    /// attached to the manifest that owns the tree.
    pub fn acquire(root_manifest: &Path) -> Result<Self, Box<[Diag]>> {
        let directory = root_directory(root_manifest)?;
        let location = LocalDirectory::new(directory.clone());
        let mut store = LocalFiles;
        let mut diagnostics = Vec::new();
        let manifest = store
            .manifest(&location)
            .map_err(|failure| root_manifest_refused(root_manifest, &failure))?;
        let parsed = crate::source::parse_manifest(&crate::source::ManifestSource::new(
            pith_diag::SourceId::from_raw(0),
            manifest.label,
            manifest.text,
        ));
        diagnostics.extend(parsed.diagnostics().iter().cloned());
        let Some(view) = parsed.validated().ok() else {
            return Err(diagnostics.into());
        };
        let span = view.span();
        let source = parsed.source().clone();
        let files = match store.sources(&location) {
            Ok(files) => files,
            Err(failure) => {
                diagnostics.push(source_tree_refused(span, &source, &failure));
                return Err(diagnostics.into());
            }
        };
        let sources = SourceSet::new(
            files
                .into_iter()
                .map(|file| ModuleFile::new(file.path, file.text)),
        );
        let sources = match sources {
            Ok(sources) => sources,
            Err(error) => {
                diagnostics.push(Diag::new(
                    Severity::Error,
                    FrontendCode::UnreadableSource.stable(),
                    span,
                    format!("the source set is not canonical: {error}"),
                ));
                return Err(diagnostics.into());
            }
        };
        if !diagnostics.is_empty() {
            return Err(diagnostics.into());
        }
        Ok(Self {
            directory,
            subject: view.subject().clone(),
            sources,
        })
    }
}

/// Why the root manifest could not be read, as the one diagnostic that
/// names the path a person typed.
fn root_manifest_refused(root_manifest: &Path, failure: &AcquireFailure) -> Box<[Diag]> {
    Box::from([Diag::new(
        Severity::Error,
        FrontendCode::MissingManifest.stable(),
        Span::none(),
        format!(
            "cannot read the root manifest `{}`: {}",
            root_manifest.display(),
            failure.describe()
        ),
    )])
}

/// A source-tree refusal, attached to the manifest that owns the tree.
fn source_tree_refused(
    span: Span,
    source: &std::sync::Arc<pith_diag::SourceFile>,
    failure: &AcquireFailure,
) -> Diag {
    let code = failure
        .diagnostic_code()
        .unwrap_or(FrontendCode::UnreadableSource);
    Diag::new(
        Severity::Error,
        code.stable(),
        span,
        format!(
            "cannot acquire the source tree of the root manifest: {}",
            failure.describe()
        ),
    )
    .with_source(source.clone())
}

/// The canonical directory the root manifest's parent names. Loading begins
/// here; nothing searches upward from it.
pub(super) fn root_directory(root_manifest: &Path) -> Result<PathBuf, Box<[Diag]>> {
    let declared = root_manifest
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::canonicalize(declared).map_err(|error| {
        let failure = AcquireFailure::Unreadable {
            message: format!("cannot resolve `{}`: {error}", declared.display()).into(),
        };
        root_manifest_refused(root_manifest, &failure)
    })
}
