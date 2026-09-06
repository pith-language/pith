//! Alias environments are module-local.
//!
//! 0067 makes an alias a scoping name inside one module and a subject the
//! thing that crosses a boundary. Two properties follow, and neither is
//! implied by the loader resolving a closure: one alias may mean different
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

fn module(directory: &Path, manifest: &str, sources: &[(&str, &str)]) -> TestResult {
    file(&directory.join("module.pi"), manifest)?;
    for (path, text) in sources {
        file(&directory.join("src").join(path), text)?;
    }
    Ok(())
}

/// Root and its dependency both bind the alias `helper`, to different
/// subjects, and each names a declaration only its own binding provides.
fn shadowed_alias(directory: &Path) -> TestResult {
    module(
        directory,
        "module example/root 0.1.0\n\nuse helper = example/alpha from path \"alpha\"\n",
        &[(
            "main.pi",
            "import helper\n\npure rule top(helper.Alpha) -> Text = host\n",
        )],
    )?;
    module(
        &directory.join("alpha"),
        "module example/alpha 0.1.0\n\nuse helper = example/beta from path \"../beta\"\n",
        &[(
            "alpha.pi",
            "import helper\n\nnominal Alpha = Text\n\npure rule wrap(helper.Beta) -> Alpha = host\n",
        )],
    )?;
    module(
        &directory.join("beta"),
        "module example/beta 0.1.0\n",
        &[("beta.pi", "nominal Beta = Text\n")],
    )?;
    Ok(())
}

fn load(directory: &Path) -> TestResult<Workspace> {
    Workspace::load(&directory.join("module.pi")).map_err(
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

    // Each module elaborated, so each resolved `helper` against its own
    // manifest: the root's rule names `helper.Alpha` and the dependency's
    // names `helper.Beta`, and neither declaration exists in the other's
    // binding.
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
        &directory.path().join("src/main.pi"),
        "import helper\nimport beta\n\npure rule top(helper.Alpha) -> Text = host\n",
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
