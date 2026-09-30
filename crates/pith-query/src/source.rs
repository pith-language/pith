//! Loading for the person-facing queries: what `check`, `explore`, and
//! `fmt` read, and the files `fmt` writes.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use pith_diag::{Diag, Severity, SourceId};
use pith_loader::{
    ModuleSource, ProjectSource, RootFiles, format_module, format_project_file, parse_project_file,
};
use pith_output::dto::{CheckReport, FmtReport, FmtStatus};

use crate::error::QueryError;
use crate::program::{Program, check_project};
use crate::view::module_view;

/// Whether `format` writes the canonical spelling back or only verifies it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FormatMode {
    Write,
    Check,
}

/// Format the project at `path`: write the canonical spelling of its
/// project file and the files it includes, or under [`FormatMode::Check`]
/// name what a write would change without touching anything. A file with
/// no header is an include; formatting one formats that file alone.
///
/// A project must parse (there is no canonical spelling otherwise) but need
/// not elaborate, which `fmt` shares with `check`. No input route is
/// resolved and no dependency acquired, since the canonical spelling is a
/// function of these files alone.
///
/// # Errors
/// [`QueryError`] when the file cannot be read, does not parse, or cannot
/// be written.
pub fn format(path: &Path, mode: FormatMode) -> Result<Vec<FmtReport>, QueryError> {
    let text = fs::read_to_string(path)
        .map_err(|error| QueryError::user(format!("cannot read `{}`: {error}", path.display())))?;
    let project = parse_project_file(&ProjectSource::new(
        SourceId::from_raw(0),
        path.display().to_string(),
        text.clone(),
    ));
    if project.diagnostics().is_empty() {
        return format_project(path, mode);
    }
    if let Some(report) = format_include(path, &text, mode)? {
        return Ok(vec![report]);
    }
    Err(QueryError::user(format!(
        "`{}` does not parse, so it has no canonical spelling",
        path.display()
    ))
    .with_diagnostics(project.diagnostics().to_vec().into_boxed_slice()))
}

/// The project at `path` and the includes its header names.
fn format_project(path: &Path, mode: FormatMode) -> Result<Vec<FmtReport>, QueryError> {
    let root = RootFiles::acquire(path).map_err(|diagnostics| {
        QueryError::user(format!(
            "`{}` does not parse or its includes are not acquirable, so it has no canonical \
             spelling",
            path.display()
        ))
        .with_diagnostics(diagnostics)
    })?;
    let identity = root.subject().spelling();

    let mut reports = Vec::new();
    let project_path = root.directory().join(pith_loader::PROJECT_NAME);
    let project_text = root.files().project().text().to_string();
    let canonical = format_project_file(&ProjectSource::new(
        SourceId::from_raw(0),
        project_path.display().to_string(),
        project_text.clone(),
    ))
    .map_err(|diagnostics| {
        QueryError::user(format!(
            "`{}` does not parse, so it has no canonical spelling",
            project_path.display()
        ))
        .with_diagnostics(diagnostics)
    })?;
    reports.push(FmtReport {
        module: identity.clone(),
        path: project_path.display().to_string().into(),
        status: write_or_check(&project_path, mode, &canonical, &project_text)?,
    });

    for file in root.files().includes().files() {
        let file_path = root.directory().join(file.path());
        let canonical = format_module(&ModuleSource::new(
            identity.clone(),
            SourceId::from_raw(0),
            file.path(),
            file.text(),
        ))
        .map_err(|diagnostics| {
            QueryError::user(format!(
                "`{}` does not parse, so it has no canonical spelling",
                file_path.display()
            ))
            .with_diagnostics(diagnostics)
        })?;
        reports.push(FmtReport {
            module: identity.clone(),
            path: file_path.display().to_string().into(),
            status: write_or_check(&file_path, mode, &canonical, file.text())?,
        });
    }
    Ok(reports)
}

/// One include file, formatted alone. `None` when the file does not parse
/// as one either.
fn format_include(
    path: &Path,
    text: &str,
    mode: FormatMode,
) -> Result<Option<FmtReport>, QueryError> {
    let module = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("include");
    let source = ModuleSource::new(
        module,
        SourceId::from_raw(0),
        path.display().to_string(),
        text,
    );
    let canonical = match format_module(&source) {
        Ok(canonical) => canonical,
        Err(_) => return Ok(None),
    };
    let status = write_or_check(path, mode, &canonical, text)?;
    Ok(Some(FmtReport {
        module: module.into(),
        path: path.display().to_string().into(),
        status,
    }))
}

fn write_or_check(
    path: &Path,
    mode: FormatMode,
    canonical: &str,
    current: &str,
) -> Result<FmtStatus, QueryError> {
    Ok(match (mode, canonical == current) {
        (_, true) => FmtStatus::Unchanged,
        (FormatMode::Check, false) => FmtStatus::WouldFormat,
        (FormatMode::Write, false) => {
            replace_file(path, canonical.as_bytes()).map_err(|error| {
                QueryError::user(format!("cannot write `{}`: {error}", path.display()))
            })?;
            FmtStatus::Formatted
        }
    })
}

/// Elaborate the project at `path` and report what elaboration said, whether
/// or not it succeeded.
///
/// # Errors
/// [`QueryError`] when the file cannot be read.
pub fn check(path: &Path) -> Result<CheckReport, QueryError> {
    let checked = check_project(path)?;
    let errors = count(&checked.diagnostics, Severity::Error);
    let warnings = count(&checked.diagnostics, Severity::Warning);
    Ok(CheckReport {
        module: checked.root,
        path: path.display().to_string().into(),
        abi_digest: checked
            .abi_digest
            .map(|digest| digest.digest().to_string().into()),
        diagnostics: checked
            .diagnostics
            .iter()
            .map(crate::view::diagnostic)
            .collect(),
        errors,
        warnings,
    })
}

/// What the project at `path` declares: its types, its rules, the
/// interface each rule provides, and which tier answers it.
///
/// # Errors
/// [`QueryError`] when the file cannot be read or does not elaborate.
/// Unlike `check`, this one needs a project that loaded: there is nothing
/// to explore about a project with no declarations.
pub fn explore(path: &Path) -> Result<pith_output::dto::ModuleView, QueryError> {
    let program = Program::load(path)?;
    Ok(module_view(path, program.root()))
}

fn count(diagnostics: &[Diag], severity: Severity) -> u64 {
    let matching = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == severity)
        .count();
    u64::try_from(matching).unwrap_or(u64::MAX)
}

fn replace_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

    let permissions = fs::metadata(path)?.permissions();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let (temporary_path, mut temporary) = loop {
        let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".pith-format-{}-{sequence}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => break (candidate, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    };

    let publish = (|| {
        temporary.set_permissions(permissions)?;
        temporary.write_all(bytes)?;
        temporary.sync_all()?;
        drop(temporary);
        fs::rename(&temporary_path, path)?;
        File::open(parent)?.sync_all()
    })();
    if publish.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    publish
}
