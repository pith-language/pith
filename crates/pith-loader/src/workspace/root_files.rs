//! The acquisition path that formats a project: the root project file and
//! the includes it names, and nothing else.

use std::path::{Path, PathBuf};

use pith_diag::{Diag, Severity, Span};
use pith_hir::{FrontendCode, ModuleSubject};

use super::acquire::{AcquireFailure, LocalDirectory, LocalFiles, ProjectStore};
use super::module::{ModuleFile, ProjectFiles};

/// The files `fmt` writes: the root project file and the files it
/// includes. Nothing else is read, since their canonical spelling is a
/// function of these files alone.
pub struct RootFiles {
    directory: PathBuf,
    subject: ModuleSubject,
    files: ProjectFiles,
}

impl RootFiles {
    #[must_use]
    pub fn subject(&self) -> &ModuleSubject {
        &self.subject
    }

    /// The canonical directory the project file was read from; each
    /// include's write path is this directory joined with its path.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    #[must_use]
    pub fn files(&self) -> &ProjectFiles {
        &self.files
    }

    /// Acquire the root project file and the includes it names.
    ///
    /// # Errors
    /// The project file's parse diagnostics, and acquisition refusals for
    /// its includes (symlinked, irregular, or unreadable files), each
    /// attached to the clause that named the file.
    pub fn acquire(root_project: &Path) -> Result<Self, Box<[Diag]>> {
        let directory = root_directory(root_project)?;
        let location = LocalDirectory::new(directory.clone());
        let mut store = LocalFiles;
        let mut diagnostics = Vec::new();
        let project = store
            .project(&location)
            .map_err(|failure| root_project_refused(root_project, &failure))?;
        let parsed = crate::source::parse_project_file(&crate::source::ProjectSource::new(
            pith_diag::SourceId::from_raw(0),
            project.label,
            project.text.clone(),
        ));
        diagnostics.extend(parsed.diagnostics().iter().cloned());
        let Some(view) = parsed.validated().ok() else {
            if parsed.diagnostics().is_empty() {
                diagnostics.push(Diag::new(
                    Severity::Error,
                    FrontendCode::MissingSubject.stable(),
                    Span::point(pith_diag::ByteOffset(
                        u32::try_from(parsed.source().source_text().len()).unwrap_or(0),
                    )),
                    "the root project declares no `module` clause, and a loaded project names \
                     its subject",
                ));
            }
            return Err(diagnostics.into());
        };
        let source = parsed.source().clone();
        let mut includes = Vec::new();
        for include in view.includes() {
            match store.include(&location, &include.path) {
                Ok(acquired) => includes.push(ModuleFile::new(acquired.path, acquired.text)),
                Err(failure) => {
                    let code = failure
                        .diagnostic_code()
                        .unwrap_or(FrontendCode::UnreadableSource);
                    diagnostics.push(
                        Diag::new(
                            Severity::Error,
                            code.stable(),
                            include.span,
                            format!(
                                "cannot read the include `{}`, which the project names: {}",
                                include.path,
                                failure.describe()
                            ),
                        )
                        .with_source(source.clone()),
                    );
                }
            }
        }
        let files = ProjectFiles::new(project.text, includes);
        let files = match files {
            Ok(files) => files,
            Err(error) => {
                diagnostics.push(Diag::new(
                    Severity::Error,
                    FrontendCode::UnreadableSource.stable(),
                    view.span(),
                    format!("the file set is not canonical: {error}"),
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
            files,
        })
    }
}

/// Why the root project file could not be read, as the one diagnostic that
/// names the path a person typed.
fn root_project_refused(root_project: &Path, failure: &AcquireFailure) -> Box<[Diag]> {
    Box::from([Diag::new(
        Severity::Error,
        FrontendCode::MissingProject.stable(),
        Span::none(),
        format!(
            "cannot read the root project `{}`: {}",
            root_project.display(),
            failure.describe()
        ),
    )])
}

/// The canonical directory the root project file's parent names. Loading
/// begins here; nothing searches upward from it.
pub(super) fn root_directory(root_project: &Path) -> Result<PathBuf, Box<[Diag]>> {
    let declared = root_project
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::canonicalize(declared).map_err(|error| {
        let failure = AcquireFailure::Unreadable {
            message: format!("cannot resolve `{}`: {error}", declared.display()).into(),
        };
        root_project_refused(root_project, &failure)
    })
}
