//! A project owns its own directory and nothing else: every path a
//! project names, an include or an input route or a workspace member,
//! resolves inside the location that named it, and a `..` component, an
//! absolute spelling, or a symlinked component is refused before anything
//! is read through it.

use std::fs;
use std::path::Path;

use pith_loader::{FrontendCode, PROJECT_NAME, Workspace};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn file(path: &Path, text: &str) -> TestResult {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

fn has_code(diagnostics: &[pith_diag::Diag], code: FrontendCode) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == code.stable())
}

/// The consumer of the escaping dependency.
const ROOT: &str = "module example/root 0.1.0\n\ninputs {\n  dep = path \"dep\",\n}\n\nimport \
         dep\n\nentry hello : Text = ask (Message(\"hello\"))\n";

/// An include spelling `..` reaches a readable declaration file outside the
/// project directory; the load refuses it rather than elaborating the
/// outside file's declarations into the module.
#[test]
fn an_include_cannot_read_outside_the_project_directory() -> TestResult {
    let root = tempfile::tempdir()?;
    let directory = root.path().join("project");
    file(
        &directory.join(PROJECT_NAME),
        "module example/dep 0.1.0\n\ninputs {\n}\n\ninclude \"../outside/types.pi\"\n",
    )?;
    file(
        &root.path().join("outside/types.pi"),
        "nominal Stolen = Text\n",
    )?;

    let diagnostics = match Workspace::load(&directory.join(PROJECT_NAME)) {
        Ok(_) => unreachable!("an escaping include loaded"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        has_code(&diagnostics, FrontendCode::EscapingPath),
        "the escape is refused by name: {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.0.contains("Stolen")),
        "the outside file's declarations reached the module: {diagnostics:?}"
    );
    Ok(())
}

/// A symlink as an intermediate component carries the path outside the
/// project even when the final component is a regular file; every
/// component is checked, which is the invariant 0067 kept through its
/// `src/` walk.
#[test]
#[cfg(unix)]
fn an_include_cannot_pass_through_a_symlinked_directory() -> TestResult {
    let root = tempfile::tempdir()?;
    let directory = root.path().join("project");
    fs::create_dir_all(&directory)?;
    file(
        &directory.join(PROJECT_NAME),
        "module example/dep 0.1.0\n\ninputs {\n}\n\ninclude \"link/types.pi\"\n",
    )?;
    let outside = root.path().join("outside");
    fs::create_dir_all(&outside)?;
    file(&outside.join("types.pi"), "nominal Stolen = Text\n")?;
    std::os::unix::fs::symlink(&outside, directory.join("link"))?;

    let diagnostics = match Workspace::load(&directory.join(PROJECT_NAME)) {
        Ok(_) => unreachable!("an include through a symlinked directory loaded"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        has_code(&diagnostics, FrontendCode::SymlinkedSource),
        "the symlinked component is refused: {diagnostics:?}"
    );
    Ok(())
}

/// A dependency's own inputs resolve inside the dependency's directory: a
/// fetched or untrusted project cannot direct the loader at directories its
/// consumer never named.
#[test]
fn a_dependency_input_cannot_leave_the_dependency_directory() -> TestResult {
    let root = tempfile::tempdir()?;
    let directory = root.path();
    file(&directory.join(PROJECT_NAME), ROOT)?;
    file(
        &directory.join("dep").join(PROJECT_NAME),
        "module example/dep 0.1.0\n\ninputs {\n  steal = path \"../outside\"\n}",
    )?;
    let outside = directory.join("outside");
    file(
        &outside.join(PROJECT_NAME),
        "module example/stolen 0.1.0\n\ninputs {\n}\n\nnominal Stolen = Text\n",
    )?;

    let diagnostics = match Workspace::load(&directory.join(PROJECT_NAME)) {
        Ok(_) => unreachable!("a dependency routed the loader outside its directory"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        has_code(&diagnostics, FrontendCode::EscapingPath),
        "the escaping route is refused by name: {diagnostics:?}"
    );
    Ok(())
}

/// A workspace member path is a route like any other: an absolute spelling
/// discards the root entirely if joined unchecked, and is refused.
#[test]
fn an_absolute_member_path_is_refused() -> TestResult {
    let elsewhere = tempfile::tempdir()?;
    file(
        &elsewhere.path().join(PROJECT_NAME),
        "module example/elsewhere 0.1.0\n\ninputs {\n}\n\nnominal Elsewhere = Text\n",
    )?;
    let root = tempfile::tempdir()?;
    let directory = root.path();
    file(
        &directory.join(PROJECT_NAME),
        &format!(
            "module example/root 0.1.0\n\ninputs {{\n}}\n\nworkspace {{\n  members: \
             [\"{}\"],\n}}\n",
            elsewhere.path().display()
        ),
    )?;

    let diagnostics = match Workspace::load(&directory.join(PROJECT_NAME)) {
        Ok(_) => unreachable!("an absolute member path loaded"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        has_code(&diagnostics, FrontendCode::EscapingPath),
        "the absolute member is refused by name: {diagnostics:?}"
    );
    Ok(())
}

/// A `..` member is refused the same way, at the member's clause.
#[test]
fn a_member_path_leaving_the_root_is_refused() -> TestResult {
    let root = tempfile::tempdir()?;
    let directory = root.path();
    file(
        &directory.join(PROJECT_NAME),
        "module example/root 0.1.0\n\ninputs {\n}\n\nworkspace {\n  members: [\"../sibling\"],\n}\n",
    )?;
    file(
        &root.path().join("sibling").join(PROJECT_NAME),
        "module example/sibling 0.1.0\n\ninputs {\n}\n\nnominal Sibling = Text\n",
    )?;

    let diagnostics = match Workspace::load(&directory.join(PROJECT_NAME)) {
        Ok(_) => unreachable!("an escaping member loaded"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        has_code(&diagnostics, FrontendCode::EscapingPath),
        "the escaping member is refused by name: {diagnostics:?}"
    );
    Ok(())
}
