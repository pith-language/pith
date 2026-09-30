//! Input environments are module-local: an input name scopes inside one module,
//! a subject is what crosses a boundary. One alias may mean different
//! subjects in different modules, and a module may not name a subject its
//! own manifest did not bind, however its dependencies reached it.

use std::fs;
use std::path::Path;

use pith_loader::{ElaborateError, FrontendCode, Workspace};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn file(path: &Path, text: &str) -> TestResult {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

/// Root and its dependency both bind `helper`, to different subjects; each
/// names a declaration only its own binding provides.
fn shadowed_alias(directory: &Path) -> TestResult {
    file(
        &directory.join("pith.pi"),
        "module example/root 0.1.0\n\ninputs {\n  helper = path \"alpha\"\n}\n\nimport          helper\n\npure rule top(helper.Alpha) -> Text = host\n",
    )?;
    file(
        &directory.join("alpha").join("pith.pi"),
        "module example/alpha 0.1.0\n\ninputs {\n  helper = path \"beta\"\n}\n\nimport          helper\n\nnominal Alpha = Text\n\npure rule wrap(helper.Beta) -> Alpha = host\n",
    )?;
    file(
        &directory.join("alpha").join("beta").join("pith.pi"),
        "module example/beta 0.1.0\n\ninputs {\n}\n\nnominal Beta = Text\n",
    )?;
    Ok(())
}

fn load(directory: &Path) -> TestResult<Workspace> {
    Workspace::load(&directory.join("pith.pi")).map_err(
        |diagnostics| -> Box<dyn std::error::Error> {
            format!("the fixture resolves: {diagnostics:?}").into()
        },
    )
}

#[test]
fn one_alias_means_different_subjects_in_different_modules() -> TestResult {
    let directory = tempfile::tempdir()?;
    shadowed_alias(directory.path())?;

    let workspace = load(directory.path())?;
    let elaborated = workspace
        .elaborate()
        .map_err(|error| -> Box<dyn std::error::Error> { error.to_string().into() })?;

    // Each module resolved `helper` against its own manifest.
    let imports = |spelling: &str| {
        elaborated
            .modules()
            .find(|module| module.module() == spelling)
            .map(|module| {
                module
                    .imports()
                    .iter()
                    .map(|(subject, _)| subject.to_string())
                    .collect::<Vec<_>>()
            })
    };
    assert_eq!(
        imports("example/root"),
        Some(vec!["example/alpha".to_string()]),
        "the root's import basis names a subject it did not bind"
    );
    assert_eq!(
        imports("example/alpha"),
        Some(vec!["example/beta".to_string()]),
        "the dependency's import basis names a subject it did not bind"
    );
    Ok(())
}

/// The root reaches `example/beta` only through its dependency. Naming it
/// without a `use` clause of its own is the unknown-import refusal, not an
/// implicit re-export.
#[test]
fn an_undeclared_transitive_import_is_refused() -> TestResult {
    let directory = tempfile::tempdir()?;
    shadowed_alias(directory.path())?;
    file(
        &directory.path().join("pith.pi"),
        "module example/root 0.1.0\n\ninputs {\n  helper = path \"alpha\"\n}\n\nimport          helper\nimport beta\n\npure rule top(helper.Alpha) -> Text = host\n",
    )?;

    let workspace = load(directory.path())?;
    let Err(ElaborateError::Diagnostics(diagnostics)) = workspace.elaborate() else {
        return Err("the root named an unbound subject and elaborated anyway".into());
    };
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == FrontendCode::UnknownImport.stable()),
        "the transitive import was not refused: {diagnostics:?}"
    );
    Ok(())
}
