//! Design probes for module distribution: executable measurements of the
//! assumptions the registry and compatibility records rest on.

use std::fs;
use std::path::Path;

use pith_loader::{ElaboratedWorkspace, LoadedModule, Workspace};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const ROOT_MANIFEST: &str =
    "module example/root 0.1.0\n\nuse dep = example/dep from path \"dep\"\n";
const DEP_MANIFEST: &str = "module example/dep 0.1.0\n";
const DEP_TYPES: &str = "nominal Message = Text\n";
const DEP_RULES: &str = "pure rule speak(who: Message) -> Text = { \"first\" }\n";
const ROOT_MAIN: &str = "import dep\n\nentry hello : Text = ask (Message(\"hello\"))\n";

fn file(path: &Path, text: &str) -> TestResult {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    fs::write(path, text)?;
    Ok(())
}

fn project(directory: &Path) -> TestResult {
    file(&directory.join("module.pi"), ROOT_MANIFEST)?;
    file(&directory.join("src/main.pi"), ROOT_MAIN)?;
    file(&directory.join("dep/module.pi"), DEP_MANIFEST)?;
    file(&directory.join("dep/src/types.pi"), DEP_TYPES)?;
    file(&directory.join("dep/src/rules.pi"), DEP_RULES)?;
    Ok(())
}

fn elaborate(directory: &Path) -> TestResult<ElaboratedWorkspace> {
    let workspace = Workspace::load(&directory.join("module.pi")).map_err(
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

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// The encoding with every occurrence of `needle` removed, so two surfaces
/// that differ only in an embedded digest compare equal.
fn redact(haystack: &[u8], needle: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(haystack.len());
    let mut rest = haystack;
    while let Some(at) = rest
        .windows(needle.len())
        .position(|window| window == needle)
    {
        out.extend_from_slice(rest.get(..at).unwrap_or_default());
        rest = rest
            .get(at.saturating_add(needle.len())..)
            .unwrap_or_default();
    }
    out.extend_from_slice(rest);
    out
}

/// A published ABI is a claim about one dependency context, not about the
/// module's source: the consumer's own bytes never change here, yet its ABI
/// moves with the dependency's public surface, because a module's ABI
/// covers the imported subject/ABI pairs it elaborated against. A registry
/// carrying one ABI per release therefore means "the ABI under the context
/// this release recorded".
#[test]
fn an_unchanged_consumer_gets_a_new_abi_when_its_dependency_surface_moves() -> TestResult {
    let root = tempfile::tempdir()?;
    project(root.path())?;

    let before = elaborate(root.path())?;
    let dep_abi_before = module(&before, "example/dep").abi_digest();
    let root_abi_before = module(&before, "example/root").abi_digest();
    let root_surface_before = module(&before, "example/root").interface_surface().encode();
    let root_imports_before = module(&before, "example/root").imports().to_vec();
    drop(before);

    // A public addition to the dependency that adds no competing provider:
    // a new nominal, not a new rule.
    file(
        &root.path().join("dep/src/types.pi"),
        "nominal Message = Text\nnominal Other = Text\n",
    )?;
    assert_eq!(
        fs::read_to_string(root.path().join("src/main.pi"))?,
        ROOT_MAIN,
        "the probe edited the consumer; it must only edit the dependency"
    );

    let after = elaborate(root.path())?;
    assert_ne!(
        dep_abi_before,
        module(&after, "example/dep").abi_digest(),
        "a public nominal addition left the dependency's ABI unmoved"
    );
    assert_ne!(
        root_abi_before,
        module(&after, "example/root").abi_digest(),
        "the consumer's ABI did not follow its dependency's; a published ABI \
         would then be context-free, and the registry could carry one per version"
    );
    assert_ne!(
        root_imports_before,
        module(&after, "example/root").imports().to_vec(),
        "the consumer's recorded imported ABIs did not move"
    );

    // The consumer's interface surface is context-dependent too: the surface
    // a registry would publish for an unedited module is not stable across
    // dependency contexts.
    let root_surface_after = module(&after, "example/root").interface_surface().encode();
    assert_ne!(
        root_surface_before, root_surface_after,
        "the consumer's interface surface did not follow its dependency's ABI"
    );

    // What moved is exactly the dependency's ABI digest embedded in the
    // consumer's surface, so `pith diff` cannot attribute a surface
    // difference to the edited module without subtracting the imported-ABI
    // region: two surfaces of byte-identical source differ whenever their
    // contexts do.
    let dep_abi_after = module(&after, "example/dep").abi_digest();
    assert!(
        contains(&root_surface_before, dep_abi_before.digest().as_bytes()),
        "the consumer's surface did not embed its dependency's old ABI digest"
    );
    assert!(
        contains(&root_surface_after, dep_abi_after.digest().as_bytes()),
        "the consumer's surface did not embed its dependency's new ABI digest"
    );
    assert_eq!(
        redact(&root_surface_before, dep_abi_before.digest().as_bytes()),
        redact(&root_surface_after, dep_abi_after.digest().as_bytes()),
        "the consumer's surface moved somewhere other than its imported ABI region; \
         the differ cannot recover the module's own change by subtracting imports"
    );
    Ok(())
}

/// The same probe from the other side: a dependency edit that does not touch
/// its public surface leaves the consumer's ABI alone, so the probe above is
/// not satisfied by an ABI that moves on any dependency edit.
#[test]
fn a_dependency_body_edit_leaves_the_consumer_abi_alone() -> TestResult {
    let root = tempfile::tempdir()?;
    project(root.path())?;

    let before = elaborate(root.path())?;
    let root_abi_before = module(&before, "example/root").abi_digest();
    let dep_abi_before = module(&before, "example/dep").abi_digest();
    drop(before);

    file(
        &root.path().join("dep/src/rules.pi"),
        "pure rule speak(who: Message) -> Text = { \"second\" }\n",
    )?;

    let after = elaborate(root.path())?;
    assert_eq!(
        dep_abi_before,
        module(&after, "example/dep").abi_digest(),
        "a body edit moved the dependency's ABI"
    );
    assert_eq!(
        root_abi_before,
        module(&after, "example/root").abi_digest(),
        "a dependency body edit moved the consumer's ABI"
    );
    Ok(())
}
