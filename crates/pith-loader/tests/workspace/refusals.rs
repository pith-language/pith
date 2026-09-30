//! The loader's refusal table: membership, identity, cycles, include shape,
//! and authority, each at its own diagnostic code and pointed at the
//! project file that caused it.

use std::fs;
use std::path::Path;

use pith_loader::{FrontendCode, PROJECT_NAME, Workspace};

use super::{GREETING, TestResult, file, fixture, has_code, load_err, message_of, project};

#[test]
fn a_member_listed_twice_is_refused_at_the_second_entry() -> TestResult {
    let (_guard, directory) = fixture()?;
    let root = fs::read_to_string(directory.join(PROJECT_NAME))?;
    file(
        &directory.join(PROJECT_NAME),
        &root.replace(
            "members: [\"modules/greeting\"],",
            "members: [\"modules/greeting\", \"modules/greeting\"],",
        ),
    )?;
    let diagnostics = load_err(&directory);
    assert!(has_code(&diagnostics, FrontendCode::DuplicateMember));
    Ok(())
}

#[test]
fn two_locations_declaring_one_subject_are_refused() -> TestResult {
    let (_guard, directory) = fixture()?;
    project(
        &directory.join("modules/other"),
        GREETING,
        &[("o.pi", "nominal Message = Text\n")],
    )?;
    let root = fs::read_to_string(directory.join(PROJECT_NAME))?;
    file(
        &directory.join(PROJECT_NAME),
        &root.replace(
            "members: [\"modules/greeting\"],",
            "members: [\"modules/greeting\", \"modules/other\"],",
        ),
    )?;
    let diagnostics = load_err(&directory);
    assert!(has_code(&diagnostics, FrontendCode::DuplicateSubject));
    Ok(())
}

#[test]
fn an_input_target_that_declares_no_subject_is_refused() -> TestResult {
    let root = tempfile::tempdir()?;
    let directory = root.path();
    project(
        directory,
        "module example/root 0.1.0\n\ninputs {\n  dep = path \"dep\"\n}",
        &[("main.pi", "import dep\n")],
    )?;
    file(
        &directory.join("dep").join(PROJECT_NAME),
        "inputs {\n}\n\nnominal T = Text\n",
    )?;

    let diagnostics = load_err(directory);
    assert!(has_code(&diagnostics, FrontendCode::MissingSubject));
    Ok(())
}

/// A route that stays inside its directory can only descend, so a cycle
/// among local routes is a project naming its own directory; the registry
/// routes, which address by subject, carry the general case.
#[test]
fn a_dependency_cycle_names_the_chain() -> TestResult {
    let root = tempfile::tempdir()?;
    let directory = root.path();
    project(
        directory,
        "module example/a 0.1.0\n\ninputs {\n  self = path \".\"\n}",
        &[("a.pi", "import self\n")],
    )?;

    let diagnostics = load_err(directory);
    assert!(has_code(&diagnostics, FrontendCode::DependencyCycle));
    let message = message_of(&diagnostics, FrontendCode::DependencyCycle);
    assert!(
        message.contains("example/a"),
        "the chain names the subject on the route: {message}"
    );
    Ok(())
}

#[test]
fn a_member_or_dependency_without_a_project_file_is_refused() -> TestResult {
    let (_guard, directory) = fixture()?;
    fs::create_dir_all(directory.join("modules/empty"))?;
    let root = fs::read_to_string(directory.join(PROJECT_NAME))?;
    file(
        &directory.join(PROJECT_NAME),
        &root.replace(
            "\"modules/greeting\"],",
            "\"modules/greeting\", \"modules/empty\"],",
        ),
    )?;
    assert!(has_code(
        &load_err(&directory),
        FrontendCode::MissingProject
    ));

    let root = tempfile::tempdir()?;
    let directory = root.path();
    project(
        directory,
        "module example/a 0.1.0\n\ninputs {\n  ghost = path \"ghost\"\n}",
        &[("a.pi", "import ghost\n")],
    )?;
    assert!(has_code(&load_err(directory), FrontendCode::MissingProject));
    Ok(())
}

#[test]
fn a_member_or_dependency_declaring_a_workspace_is_refused() -> TestResult {
    let (_guard, directory) = fixture()?;
    let greeting = directory.join("modules/greeting");
    let project_text = fs::read_to_string(greeting.join(PROJECT_NAME))?;
    file(
        &greeting.join(PROJECT_NAME),
        &project_text.replace(
            "inputs {\n}",
            "inputs {\n}\n\nworkspace { members: [\"../greeting\"] }",
        ),
    )?;
    assert!(has_code(
        &load_err(&directory),
        FrontendCode::NestedWorkspace
    ));
    Ok(())
}

#[test]
fn a_symlinked_include_is_refused() -> TestResult {
    let (_guard, directory) = fixture()?;
    let outside = directory.parent().unwrap_or_else(|| Path::new("."));
    file(&outside.join("outside.pi"), "nominal Outside = Text\n")?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        outside.join("outside.pi"),
        directory.join("modules/greeting/linked.pi"),
    )?;
    let _ = outside;
    let greeting = directory.join("modules/greeting");
    let project_text = fs::read_to_string(greeting.join(PROJECT_NAME))?;
    file(
        &greeting.join(PROJECT_NAME),
        &format!("{project_text}\n\ninclude \"linked.pi\"\n"),
    )?;

    assert!(has_code(
        &load_err(&directory),
        FrontendCode::SymlinkedSource
    ));
    Ok(())
}

#[test]
fn a_project_with_no_includes_loads() -> TestResult {
    let root = tempfile::tempdir()?;
    let directory = root.path();
    project(directory, "module example/root 0.1.0\n\ninputs {\n}", &[])?;
    let workspace = super::load(directory);
    assert_eq!(workspace.len().get(), 1);
    assert!(
        workspace.root().files().includes().is_empty(),
        "the project file is the whole file set"
    );
    Ok(())
}

#[test]
fn the_builtin_input_name_stays_bound_to_the_builtin_module() -> TestResult {
    let root = tempfile::tempdir()?;
    let directory = root.path();
    project(
        directory,
        "module example/root 0.1.0\n\ninputs {\n  pith = path \"dep\"\n}",
        &[("main.pi", "import pith\n")],
    )?;
    project(
        &directory.join("dep"),
        "module example/dep 0.1.0\n\ninputs {\n}",
        &[("d.pi", "nominal T = Text\n")],
    )?;

    assert!(has_code(
        &load_err(directory),
        FrontendCode::BuiltinShadowed
    ));
    Ok(())
}

/// A fifo spelling an include is refused rather than opened: reading one
/// blocks, and a loader that tried would hang.
#[cfg(unix)]
#[test]
fn special_files_are_refused() -> TestResult {
    use std::process::Command;

    let (_guard, directory) = fixture()?;
    let hanging = directory.join("modules/greeting/hang.pi");
    let status = Command::new("mkfifo").arg(&hanging).status()?;
    assert!(status.success(), "mkfifo created the fifo");
    let greeting = directory.join("modules/greeting");
    let project_text = fs::read_to_string(greeting.join(PROJECT_NAME))?;
    file(
        &greeting.join(PROJECT_NAME),
        &format!("{project_text}\n\ninclude \"hang.pi\"\n"),
    )?;
    let diagnostics = load_err(&directory);
    assert!(has_code(&diagnostics, FrontendCode::IrregularSource));
    Ok(())
}

/// A project file that is not a regular file is refused before anything
/// reads it: the same hang, one directory up.
#[cfg(unix)]
#[test]
fn a_project_file_that_is_not_a_regular_file_is_refused() -> TestResult {
    use std::process::Command;

    let root = tempfile::tempdir()?;
    let directory = root.path();
    fs::create_dir_all(directory)?;
    let status = Command::new("mkfifo")
        .arg(directory.join(PROJECT_NAME))
        .status()?;
    assert!(status.success(), "mkfifo created the fifo");

    match Workspace::load(&directory.join(PROJECT_NAME)) {
        Err(diagnostics) => assert!(has_code(&diagnostics, FrontendCode::MissingProject)),
        Ok(_) => unreachable!("a fifo project file loads"),
    }
    Ok(())
}

/// The authority boundary, at the load side: a dependency's routing clauses
/// parse (it is a root in its own checkout) and stop carrying authority the
/// moment something else selects it.
#[test]
fn a_dependency_declaring_routing_is_diagnosed_and_carries_no_authority() -> TestResult {
    let root = tempfile::tempdir()?;
    let directory = root.path();
    project(
        directory,
        "module example/root 0.1.0\n\ninputs {\n  dep = path \"dep\"\n}",
        &[("main.pi", "nominal Root = Text\n")],
    )?;
    project(
        &directory.join("dep"),
        "module example/dep 0.1.0\n\ninputs {\n  registry other = \"https://elsewhere/index\" \
         root \"ed25519:AAAA\"\n  domain example from other\n}",
        &[("dep.pi", "nominal Dep = Text\n")],
    )?;

    let diagnostics = load_err(directory);
    assert!(
        has_code(&diagnostics, FrontendCode::DependencySuppliedAuthority),
        "the dependency's routing was accepted: {diagnostics:?}"
    );
    let message = message_of(&diagnostics, FrontendCode::DependencySuppliedAuthority);
    assert!(
        message.contains("registry `other`"),
        "the diagnostic does not name the clause: {message}"
    );
    Ok(())
}
