//! The local workspace loader: the resolved graph of a two-module project,
//! the diamond and its single load, member isolation, and the refusal
//! table. [`resolution`] holds what a resolved workspace contains,
//! [`refusals`] what it refuses.

mod refusals;
mod resolution;

use std::fs;
use std::path::{Path, PathBuf};

use pith_hir::ModuleSubject;
use pith_loader::{FrontendCode, PROJECT_NAME, Workspace};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const HELLO: &str =
    "module example/hello 0.1.0\n\ninputs {\n  greeting = path \"modules/greeting\"\n}";
const GREETING: &str = "module example/greeting 0.1.0\n\ninputs {\n}";

fn subject(spelling: &str) -> ModuleSubject {
    ModuleSubject::parse(spelling).unwrap_or_else(|error| unreachable!("{error}"))
}

fn file(path: &Path, text: &str) -> TestResult {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    fs::write(path, text)?;
    Ok(())
}

/// A project directory: the project file plus the includes it names.
fn project(directory: &Path, header: &str, includes: &[(&str, &str)]) -> TestResult {
    file(
        &directory.join(PROJECT_NAME),
        &format!("{header}\n\n{}\n", include_clauses(includes)),
    )?;
    for (relative, text) in includes {
        file(&directory.join(relative), text)?;
    }
    Ok(())
}

fn include_clauses(includes: &[(&str, &str)]) -> String {
    includes
        .iter()
        .map(|(path, _)| format!("include \"{path}\"\n"))
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// The two-module project of the acceptance fixture.
fn fixture() -> TestResult<(tempfile::TempDir, PathBuf)> {
    let root = tempfile::tempdir()?;
    let directory = root.path().join("local-workspace");
    file(
        &directory.join(PROJECT_NAME),
        &format!(
            "{HELLO}\n\nworkspace {{\n  members: [\"modules/greeting\"],\n}}\n\nimport \
             greeting\nnominal Message = greeting.Message\n"
        ),
    )?;
    project(
        &directory.join("modules/greeting"),
        GREETING,
        &[
            ("types.pi", "nominal Message = Text\n"),
            ("rules.pi", "pure rule greet(Message) -> Text = host\n"),
        ],
    )?;
    Ok((root, directory))
}

fn load(directory: &Path) -> Workspace {
    match Workspace::load(&directory.join(PROJECT_NAME)) {
        Ok(workspace) => workspace,
        Err(diagnostics) => unreachable!("the workspace loads: {diagnostics:?}"),
    }
}

fn load_err(directory: &Path) -> Box<[pith_diag::Diag]> {
    match Workspace::load(&directory.join(PROJECT_NAME)) {
        Err(diagnostics) => diagnostics,
        Ok(_) => unreachable!("the workspace refuses this project"),
    }
}

fn has_code(diagnostics: &[pith_diag::Diag], code: FrontendCode) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == code.stable())
}

fn message_of(diagnostics: &[pith_diag::Diag], code: FrontendCode) -> String {
    diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == code.stable())
        .map_or_else(String::new, |diagnostic| diagnostic.message.0.to_string())
}
