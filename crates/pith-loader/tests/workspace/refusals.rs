//! The loader's refusal table: membership, identity, cycles, source-tree
//! shape, and authority, each at its own diagnostic code and pointed at
//! the manifest that caused it.

use std::fs;
use std::path::Path;

use pith_loader::{FrontendCode, Workspace};

use super::{GREETING, TestResult, file, fixture, has_code, load_err, message_of, module};

#[test]
fn a_member_listed_twice_is_refused_at_the_second_entry() -> TestResult {
    let (_guard, directory) = fixture()?;
    let root = fs::read_to_string(directory.join("module.pi"))?;
    file(
        &directory.join("module.pi"),
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
    module(
        &directory.join("modules/other"),
        GREETING,
        &[("o.pi", "nominal Message = Text\n")],
    )?;
    let root = fs::read_to_string(directory.join("module.pi"))?;
    file(
        &directory.join("module.pi"),
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
fn a_dependency_declaring_another_subject_is_refused() -> TestResult {
    let root = tempfile::tempdir()?;
    let directory = root.path();
    module(
        directory,
        "module example/root 0.1.0\nuse dep = example/dep from path \"dep\"\n",
        &[("main.pi", "import dep\n")],
    )?;
    module(
        &directory.join("dep"),
        "module example/other 0.1.0",
        &[("d.pi", "nominal T = Text\n")],
    )?;

    let diagnostics = load_err(directory);
    assert!(has_code(&diagnostics, FrontendCode::UnexpectedSubject));
    Ok(())
}

#[test]
fn a_dependency_cycle_names_the_chain() -> TestResult {
    let root = tempfile::tempdir()?;
    let directory = root.path();
    module(
        directory,
        "module example/a 0.1.0\nuse b = example/b from path \"b\"\n",
        &[("a.pi", "import b\n")],
    )?;
    module(
        &directory.join("b"),
        "module example/b 0.1.0\nuse a = example/a from path \"..\"\n",
        &[("b.pi", "import a\n")],
    )?;

    let diagnostics = load_err(directory);
    assert!(has_code(&diagnostics, FrontendCode::DependencyCycle));
    let message = message_of(&diagnostics, FrontendCode::DependencyCycle);
    assert!(
        message.contains("example/a") && message.contains("example/b"),
        "the chain names both subjects: {message}"
    );
    Ok(())
}

#[test]
fn a_member_or_dependency_without_a_manifest_is_refused() -> TestResult {
    let (_guard, directory) = fixture()?;
    fs::create_dir_all(directory.join("modules/empty"))?;
    let root = fs::read_to_string(directory.join("module.pi"))?;
    file(
        &directory.join("module.pi"),
        &root.replace(
            "\"modules/greeting\"],",
            "\"modules/greeting\", \"modules/empty\"],",
        ),
    )?;
    assert!(has_code(
        &load_err(&directory),
        FrontendCode::MissingManifest
    ));

    let root = tempfile::tempdir()?;
    let directory = root.path();
    module(
        directory,
        "module example/a 0.1.0\nuse ghost = example/ghost from path \"ghost\"\n",
        &[("a.pi", "import ghost\n")],
    )?;
    assert!(has_code(
        &load_err(directory),
        FrontendCode::MissingManifest
    ));
    Ok(())
}

#[test]
fn a_member_or_dependency_declaring_a_workspace_is_refused() -> TestResult {
    let (_guard, directory) = fixture()?;
    let greeting = directory.join("modules/greeting");
    let manifest = fs::read_to_string(greeting.join("module.pi"))?;
    file(
        &greeting.join("module.pi"),
        &format!("{manifest}\nworkspace {{ members: [\"../greeting\"] }}\n"),
    )?;
    assert!(has_code(
        &load_err(&directory),
        FrontendCode::NestedWorkspace
    ));
    Ok(())
}

#[test]
fn a_symlink_inside_the_source_set_is_refused() -> TestResult {
    let (_guard, directory) = fixture()?;
    let outside = directory.parent().unwrap_or_else(|| Path::new("."));
    file(&outside.join("outside.pi"), "nominal Outside = Text\n")?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        outside.join("outside.pi"),
        directory.join("modules/greeting/src/linked.pi"),
    )?;

    assert!(has_code(
        &load_err(&directory),
        FrontendCode::SymlinkedSource
    ));
    Ok(())
}

#[test]
fn an_empty_source_set_is_refused_for_the_selected_closure() -> TestResult {
    let root = tempfile::tempdir()?;
    let directory = root.path();
    module(
        directory,
        "module example/root 0.1.0\nuse dep = example/dep from path \"dep\"\n",
        &[("main.pi", "import dep\n")],
    )?;
    module(&directory.join("dep"), "module example/dep 0.1.0", &[])?;

    assert!(has_code(&load_err(directory), FrontendCode::EmptySourceSet));
    Ok(())
}

#[test]
fn the_builtin_alias_stays_bound_to_the_builtin_module() -> TestResult {
    let root = tempfile::tempdir()?;
    let directory = root.path();
    module(
        directory,
        "module example/root 0.1.0\nuse pith = example/dep from path \"dep\"\n",
        &[("main.pi", "import pith\n")],
    )?;
    module(
        &directory.join("dep"),
        "module example/dep 0.1.0",
        &[("d.pi", "nominal T = Text\n")],
    )?;

    assert!(has_code(
        &load_err(directory),
        FrontendCode::BuiltinShadowed
    ));
    Ok(())
}

/// A fifo spelling a source name is refused rather than opened: reading one
/// blocks, and a loader that tried would hang. A symlink replacing the
/// whole `src/` directory is refused too: it smuggles in an implicit
/// external tree.
#[cfg(unix)]
#[test]
fn special_files_and_a_symlinked_source_root_are_refused() -> TestResult {
    use std::os::unix::fs::symlink;
    use std::process::Command;

    let (_guard, directory) = fixture()?;
    let hanging = directory.join("modules/greeting/src/hang.pi");
    let status = Command::new("mkfifo").arg(&hanging).status()?;
    assert!(status.success(), "mkfifo created the fifo");
    let diagnostics = load_err(&directory);
    assert!(has_code(&diagnostics, FrontendCode::IrregularSource));

    let root = tempfile::tempdir()?;
    let elsewhere = root.path().join("elsewhere");
    module(
        &elsewhere,
        "module example/greeting 0.1.0",
        &[("types.pi", "nominal Message = Text\n")],
    )?;
    module(
        &root.path().join("hello"),
        "module example/hello 0.1.0\nuse greeting = example/greeting from path \"../elsewhere\"\n",
        &[("main.pi", "import greeting\n")],
    )?;
    let hello = root.path().join("hello");
    fs::remove_dir_all(hello.join("src"))?;
    symlink(elsewhere.join("src"), hello.join("src"))?;
    let diagnostics = load_err(&hello);
    assert!(
        has_code(&diagnostics, FrontendCode::SymlinkedSource),
        "a symlinked src/ passed: {diagnostics:?}"
    );

    let plain = root.path().join("plain");
    module(
        &plain,
        "module example/plain 0.1.0",
        &[("main.pi", "nominal T = Text\n")],
    )?;
    let src_file = plain.join("src");
    fs::remove_dir_all(&src_file)?;
    fs::write(&src_file, "nominal T = Text\n")?;
    assert!(has_code(&load_err(&plain), FrontendCode::IrregularSource));
    Ok(())
}

/// A manifest that is not a regular file is refused before anything reads
/// it: the same hang, one directory up.
#[cfg(unix)]
#[test]
fn a_manifest_that_is_not_a_regular_file_is_refused() -> TestResult {
    use std::process::Command;

    let root = tempfile::tempdir()?;
    let directory = root.path();
    fs::create_dir_all(directory.join("src"))?;
    fs::write(directory.join("src/main.pi"), "nominal T = Text\n")?;
    let status = Command::new("mkfifo")
        .arg(directory.join("module.pi"))
        .status()?;
    assert!(status.success(), "mkfifo created the fifo");

    match Workspace::load(&directory.join("module.pi")) {
        Err(diagnostics) => assert!(has_code(&diagnostics, FrontendCode::MissingManifest)),
        Ok(_) => unreachable!("a fifo manifest loads"),
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
    module(
        directory,
        "module example/root 0.1.0\n\nuse dep = example/dep from path \"dep\"\n",
        &[("main.pi", "nominal Root = Text\n")],
    )?;
    module(
        &directory.join("dep"),
        "module example/dep 0.1.0\n\
         registry other = \"https://elsewhere/index\" root \"ed25519:AAAA\"\n\
         domain example from other\n",
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
