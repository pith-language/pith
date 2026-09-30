//! What a resolved workspace holds: dependency order, the diamond loaded
//! once, member isolation, and the identities relocation and enumeration
//! order cannot move.

use std::fs;

use pith_hir::ModuleSubject;
use pith_loader::Workspace;

use super::{PROJECT_NAME, TestResult, file, fixture, load, project, subject};

#[test]
fn a_two_module_project_resolves_dependencies_first_and_sorts_includes() -> TestResult {
    let (_guard, directory) = fixture()?;
    let workspace = load(&directory);

    assert_eq!(workspace.len().get(), 2, "the root and its one dependency");
    assert_eq!(
        workspace
            .dependencies()
            .first()
            .map(|module| module.subject().clone()),
        Some(subject("example/greeting"))
    );
    assert_eq!(workspace.root().subject(), &subject("example/hello"));
    assert_eq!(
        workspace.module(&subject("example/greeting")).map(|m| m
            .files()
            .includes()
            .files()
            .iter()
            .map(|f| f.path())
            .collect::<Vec<_>>()),
        Some(vec!["rules.pi", "types.pi"])
    );
    let bindings = workspace
        .root()
        .bindings()
        .map(|(alias, subject)| (alias.to_owned(), subject.clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        bindings,
        vec![(String::from("greeting"), subject("example/greeting"))]
    );
    Ok(())
}

#[test]
fn a_diamond_loads_the_shared_dependency_once() -> TestResult {
    let root = tempfile::tempdir()?;
    let directory = root.path();
    project(
        directory,
        "module example/root 0.1.0\n\ninputs {\n  b = path \"b\"\n  c = path \"c\"\n}",
        &[("main.pi", "import b\nimport c\n")],
    )?;
    project(
        &directory.join("b"),
        "module example/b 0.1.0\n\ninputs {\n  shared = path \"../shared\"\n}",
        &[("b.pi", "import shared\n")],
    )?;
    project(
        &directory.join("c"),
        "module example/c 0.1.0\n\ninputs {\n  shared = path \"../shared\"\n}",
        &[("c.pi", "import shared\n")],
    )?;
    project(
        &directory.join("shared"),
        "module example/shared 0.1.0\n\ninputs {\n}",
        &[("s.pi", "nominal T = Text\n")],
    )?;

    let workspace = load(directory);
    let subjects = workspace
        .modules()
        .map(|module| module.subject().clone())
        .collect::<Vec<_>>();
    assert_eq!(
        subjects,
        [
            subject("example/shared"),
            subject("example/b"),
            subject("example/c"),
            subject("example/root")
        ]
    );
    Ok(())
}

#[test]
fn an_unrelated_member_is_validated_but_its_sources_are_never_acquired() -> TestResult {
    let (_guard, directory) = fixture()?;
    // A member the root does not input contributes no rules and no error.
    project(
        &directory.join("modules/unused"),
        "module example/unused 0.1.0\n\ninputs {\n}",
        &[],
    )?;
    let root = fs::read_to_string(directory.join(PROJECT_NAME))?;
    file(
        &directory.join(PROJECT_NAME),
        &root.replace(
            "members: [\"modules/greeting\"],",
            "members: [\"modules/greeting\", \"modules/unused\"],",
        ),
    )?;

    let workspace = load(&directory);
    assert_eq!(workspace.len().get(), 2, "the unused member stays out");
    Ok(())
}

#[test]
fn relocation_keeps_subjects_and_module_relative_file_sets() -> TestResult {
    let (_guard, directory) = fixture()?;
    let first = load(&directory);

    let relocated = tempfile::tempdir()?;
    let other = relocated.path().join("elsewhere/deeper");
    fs::create_dir_all(&other)?;
    fs::copy(directory.join(PROJECT_NAME), other.join(PROJECT_NAME))?;
    fs::create_dir_all(other.join("modules/greeting"))?;
    fs::copy(
        directory.join("modules/greeting").join(PROJECT_NAME),
        other.join("modules/greeting").join(PROJECT_NAME),
    )?;
    for name in ["types.pi", "rules.pi"] {
        fs::copy(
            directory.join("modules/greeting").join(name),
            other.join("modules/greeting").join(name),
        )?;
    }
    let second = load(&other);

    type Identity = (ModuleSubject, Vec<(String, pith_ids::ContentId)>);
    let identities = |workspace: &Workspace| -> Vec<Identity> {
        workspace
            .modules()
            .map(|module| {
                (
                    module.subject().clone(),
                    module
                        .files()
                        .includes()
                        .files()
                        .iter()
                        .map(|file| (file.path().to_string(), file.content_id()))
                        .collect(),
                )
            })
            .collect()
    };
    assert_eq!(identities(&first), identities(&second));
    Ok(())
}

#[test]
fn member_enumeration_order_moves_no_resolved_identity() -> TestResult {
    let (_guard, directory) = fixture()?;
    let first = load(&directory);
    project(
        &directory.join("modules/unused"),
        "module example/unused 0.1.0\n\ninputs {\n}",
        &[("u.pi", "nominal U = Text\n")],
    )?;
    let root = fs::read_to_string(directory.join(PROJECT_NAME))?;
    file(
        &directory.join(PROJECT_NAME),
        &root.replace(
            "members: [\"modules/greeting\"],",
            "members: [\"modules/unused\", \"modules/greeting\"],",
        ),
    )?;
    let second = load(&directory);

    let identities = |workspace: &Workspace| {
        workspace
            .modules()
            .map(|module| {
                (
                    module.subject().clone(),
                    module.files().includes().files().len(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(identities(&first), identities(&second));
    Ok(())
}
