//! The incremental-edit properties of a resolved workspace: a body-only
//! dependency edit keeps the consumer's elaboration while a changed called
//! result must revalidate it at run time, and a public representation edit
//! moves the dependency's ABI and invalidates the consumer's use of it.

use std::fs;
use std::path::Path;

use pith_loader::{ElaboratedWorkspace, FrontendCode, LoadedModule, Workspace};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const ROOT_PROJECT: &str = "module example/root 0.1.0\n\ninputs {\n  dep = path \"dep\"\n}\n\nimport dep\n\nentry \
     hello : Text = ask (Message(\"hello\"))\n";
const DEP_PROJECT: &str =
    "module example/dep 0.1.0\n\ninputs {\n}\n\ninclude \"types.pi\"\ninclude \"rules.pi\"\n";
const DEP_TYPES: &str = "nominal Message = Text\n";
const DEP_RULES: &str = "pure rule speak(who: Message) -> Text = { \"first\" }\n";

fn file(path: &Path, text: &str) -> TestResult {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    fs::write(path, text)?;
    Ok(())
}

/// A two-module project whose dependency owns two source files and whose
/// root asks, through its entry, the dependency's one rule.
fn project(directory: &Path) -> TestResult {
    file(&directory.join("pith.pi"), ROOT_PROJECT)?;
    file(&directory.join("dep").join("pith.pi"), DEP_PROJECT)?;
    file(&directory.join("dep/types.pi"), DEP_TYPES)?;
    file(&directory.join("dep/rules.pi"), DEP_RULES)?;
    Ok(())
}

fn elaborate(directory: &Path) -> Result<ElaboratedWorkspace, Box<dyn std::error::Error>> {
    let workspace = Workspace::load(&directory.join("pith.pi")).map_err(
        |diagnostics| -> Box<dyn std::error::Error> {
            format!("the fixture resolves: {diagnostics:?}").into()
        },
    )?;
    workspace
        .elaborate()
        .map_err(|error| -> Box<dyn std::error::Error> { error.to_string().into() })
}

fn module<'a>(workspace: &'a ElaboratedWorkspace, spelling: &str) -> &'a LoadedModule {
    workspace
        .modules()
        .find(|module| module.module() == spelling)
        .unwrap_or_else(|| unreachable!("the fixture elaborates {spelling}"))
}

fn entry_revision(
    workspace: &ElaboratedWorkspace,
) -> (pith_core::RuleIdentity, pith_core::RuleRevision) {
    workspace
        .root()
        .entries()
        .first()
        .map(|entry| {
            let rule = entry.rule();
            (rule.identity, rule.revision)
        })
        .unwrap_or_else(|| unreachable!("the root declares its entry"))
}

#[test]
fn a_body_edit_in_a_dependency_keeps_the_consumer_elaboration() -> TestResult {
    let root = tempfile::tempdir()?;
    project(root.path())?;

    let before = elaborate(root.path())?;
    file(
        &root.path().join("dep/rules.pi"),
        "pure rule speak(who: Message) -> Text = { \"second\" }\n",
    )?;
    let after = elaborate(root.path())?;

    assert_eq!(
        module(&before, "example/dep").abi_digest(),
        module(&after, "example/dep").abi_digest(),
        "a body edit moved the dependency's ABI"
    );
    assert_eq!(
        module(&before, "example/dep").interface_surface().encode(),
        module(&after, "example/dep").interface_surface().encode(),
        "a body edit moved the dependency's interface surface"
    );
    assert_ne!(
        module(&before, "example/dep")
            .represented_pure_rule("speak")
            .map(|rule| rule.digest()),
        module(&after, "example/dep")
            .represented_pure_rule("speak")
            .map(|rule| rule.digest()),
        "the body edit did not move the edited rule's digest"
    );
    assert_eq!(
        module(&before, "example/root").abi_digest(),
        module(&after, "example/root").abi_digest(),
        "a dependency body edit moved the consumer's ABI"
    );
    assert_eq!(
        entry_revision(&before),
        entry_revision(&after),
        "a dependency body edit moved the consumer's entry revision"
    );
    Ok(())
}

#[test]
fn a_public_representation_edit_invalidates_the_consumer() -> TestResult {
    let root = tempfile::tempdir()?;
    project(root.path())?;

    let before = elaborate(root.path())?;
    let dep_before = module(&before, "example/dep");
    let abi_before = dep_before.abi_digest();
    drop(before);

    file(
        &root.path().join("dep/types.pi"),
        "nominal Message = Bytes\n",
    )?;

    // One tolerant pass reports both halves: the dependency elaborates with
    // a moved ABI, and the consumer's entry refuses, constructing the
    // imported nominal with a literal the new representation rejects.
    let workspace = Workspace::load(&root.path().join("module.pi")).map_err(
        |diagnostics| -> Box<dyn std::error::Error> {
            format!("the fixture resolves: {diagnostics:?}").into()
        },
    )?;
    let checked = workspace
        .check()
        .map_err(|error| -> Box<dyn std::error::Error> { error.to_string().into() })?;
    let dependency = checked
        .modules
        .iter()
        .find(|module| module.subject.as_ref() == "example/dep")
        .unwrap_or_else(|| unreachable!("the dependency is checked"));
    let moved = dependency
        .abi_digest
        .unwrap_or_else(|| unreachable!("the dependency elaborates after its own edit"));
    assert_ne!(abi_before, moved, "the representation edit left the ABI");
    let refused = checked
        .modules
        .iter()
        .flat_map(|module| module.diagnostics.iter())
        .collect::<Vec<_>>();
    let offending = refused
        .iter()
        .find(|diagnostic| diagnostic.code == FrontendCode::TypeMismatch.stable())
        .unwrap_or_else(|| unreachable!("the incompatible use is diagnosed: {refused:?}"));
    let source = offending
        .source
        .as_ref()
        .unwrap_or_else(|| unreachable!("the diagnostic carries its source"));
    assert_eq!(
        source.label.as_ref(),
        "pith.pi",
        "the refusal points at the consumer's use, not the dependency's edit"
    );
    Ok(())
}
