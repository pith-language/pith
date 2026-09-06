//! The local workspace loader: the resolved graph of a two-module project,
//! the diamond and its single load, member isolation, and the refusal table
//! — each refusal checked for its specific diagnostic code and the manifest
//! it points at.
//!
//! What a resolved workspace holds is [`resolution`]; what it refuses is
//! [`refusals`]. Both build on the fixture helpers here.

mod refusals;
mod resolution;

use std::fs;
use std::path::{Path, PathBuf};

use pith_hir::ModuleSubject;
use pith_loader::{FrontendCode, Workspace};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const HELLO: &str = "module example/hello 0.1.0";
const GREETING: &str = "module example/greeting 0.1.0";

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

/// A module directory: manifest plus the given `src/` files.
fn module(directory: &Path, manifest: &str, sources: &[(&str, &str)]) -> TestResult {
    file(&directory.join("module.pi"), manifest)?;
    for (relative, text) in sources {
        file(&directory.join("src").join(relative), text)?;
    }
    Ok(())
}

/// The two-module project of the acceptance fixture.
fn fixture() -> TestResult<(tempfile::TempDir, PathBuf)> {
    let root = tempfile::tempdir()?;
    let directory = root.path().join("local-workspace");
    module(
        &directory,
        &format!(
            "{HELLO}\n\nworkspace {{\n  members: [\"modules/greeting\"],\n}}\n\nuse greeting = \
             example/greeting from path \"modules/greeting\"\n"
        ),
        &[(
            "main.pi",
            "import greeting\nnominal Message = greeting.Message\n",
        )],
    )?;
    module(
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
    match Workspace::load(&directory.join("module.pi")) {
        Ok(workspace) => workspace,
        Err(diagnostics) => unreachable!("the workspace loads: {diagnostics:?}"),
    }
}

fn load_err(directory: &Path) -> Box<[pith_diag::Diag]> {
    match Workspace::load(&directory.join("module.pi")) {
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
