//! The project file's own loading: a project's source content is exactly
//! its file plus its includes, so the same declarations elaborate to the
//! same digests however they are split, a file no include names is
//! invisible, and an include carrying a header clause is refused by name.

use std::fs;
use std::path::Path;

use pith_loader::{
    FrontendCode, ImportEnv, LOCK_NAME, ModuleFile, PROJECT_NAME, ProjectFiles, SourceSet,
    Workspace, elaborate_module, parse_module_sources, parse_project_files, write_lock,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const TYPES: &str = "nominal Message = Text\n";
const RULES: &str = "pure rule speak(who: Message) -> Text = { unwrap who }\n";
const ENTRY: &str = "import dep\n\nentry hello : Text = ask (Message(\"hello\"))\n";

fn file(path: &Path, text: &str) -> TestResult {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

/// The dependency as one project file that includes its two declaration
/// files.
const DEP_PROJECT: &str =
    "module example/dep 0.1.0\n\ninputs {\n}\n\ninclude \"types.pi\"\ninclude \"rules.pi\"\n";

/// The same dependency with the same declarations sitting in the project
/// file itself: the split must not move any digest.
const DEP_INLINE: &str = "module example/dep 0.1.0\n\ninputs {\n}\n\nnominal Message = Text\n";

fn dep_project_files() -> ProjectFiles {
    ProjectFiles::new(
        DEP_PROJECT,
        [
            ModuleFile::new("types.pi", TYPES),
            ModuleFile::new("rules.pi", RULES),
        ],
    )
    .unwrap_or_else(|error| unreachable!("distinct include paths: {error}"))
}

/// The 0067-shaped baseline: the same declaration files, loaded as one
/// module-relative source set with no project file above them.
fn dep_source_set() -> SourceSet {
    SourceSet::new([
        ModuleFile::new("types.pi", TYPES),
        ModuleFile::new("rules.pi", RULES),
    ])
    .unwrap_or_else(|error| unreachable!("distinct paths: {error}"))
}

fn elaborate_module_of(files: &ProjectFiles) -> pith_loader::LoadedModule {
    let parsed = parse_project_files("example/dep", files);
    elaborate_module(parsed, &ImportEnv::new())
        .unwrap_or_else(|diagnostics| unreachable!("the project elaborates: {diagnostics:?}"))
}

fn elaborate_source_set(sources: &SourceSet) -> pith_loader::LoadedModule {
    let parsed = parse_module_sources("example/dep", sources);
    elaborate_module(parsed, &ImportEnv::new())
        .unwrap_or_else(|diagnostics| unreachable!("the source set elaborates: {diagnostics:?}"))
}

/// What a digest-parity claim compares: the ABI, the interface surface,
/// and every represented body digest.
fn digests(loaded: &pith_loader::LoadedModule) -> (String, Vec<u8>, Vec<(String, String)>) {
    let bodies = loaded
        .pure_rules()
        .iter()
        .filter_map(|rule| {
            loaded
                .represented_pure_rule(rule.coordinate().name.as_ref())
                .map(|represented| {
                    (
                        rule.coordinate().name.to_string(),
                        represented.digest().digest().to_string(),
                    )
                })
        })
        .collect();
    (
        loaded.abi_digest().digest().to_string(),
        loaded.interface_surface().encode(),
        bodies,
    )
}

/// The parity gate of [0079]: a project whose source content is its file
/// plus its includes gives the same ABI and body digests as the same
/// declarations under the layout it replaces.
#[test]
fn a_projects_file_plus_includes_give_the_source_sets_digests() -> TestResult {
    let through_project = elaborate_module_of(&dep_project_files());
    let through_source_set = elaborate_source_set(&dep_source_set());
    assert_eq!(
        digests(&through_project),
        digests(&through_source_set),
        "the project file's header moved a semantic digest"
    );

    // The same declarations in the project file itself, none in includes:
    // the split is a spelling, and no digest follows it.
    let inline = ProjectFiles::new(DEP_INLINE, [ModuleFile::new("rules.pi", RULES)])
        .unwrap_or_else(|error| unreachable!("distinct include paths: {error}"));
    let through_inline = elaborate_module_of(&inline);
    assert_eq!(
        through_project.abi_digest(),
        through_inline.abi_digest(),
        "moving declarations between the project file and an include moved the ABI"
    );
    Ok(())
}

/// A `.pi` file in the project directory that no include names is not part
/// of the project, and no digest of the project reads it.
#[test]
fn a_file_no_include_names_affects_no_digest() -> TestResult {
    let root = tempfile::tempdir()?;
    let directory = root.path();
    file(&directory.join(PROJECT_NAME), DEP_PROJECT)?;
    file(&directory.join("types.pi"), TYPES)?;
    file(&directory.join("rules.pi"), RULES)?;
    file(
        &directory.join("stray.pi"),
        "nominal Stray = Text\npure rule stray(Stray) -> Text = host\n",
    )?;
    let with_stray = Workspace::load(&directory.join(PROJECT_NAME))
        .unwrap_or_else(|diagnostics| unreachable!("the project resolves: {diagnostics:?}"));

    let other = tempfile::tempdir()?;
    file(&other.path().join(PROJECT_NAME), DEP_PROJECT)?;
    file(&other.path().join("types.pi"), TYPES)?;
    file(&other.path().join("rules.pi"), RULES)?;
    let without = Workspace::load(&other.path().join(PROJECT_NAME))
        .unwrap_or_else(|diagnostics| unreachable!("the project resolves: {diagnostics:?}"));

    let semantics = |workspace: &Workspace| {
        workspace
            .elaborate()
            .unwrap_or_else(|error| unreachable!("the project elaborates: {error}"))
            .root()
            .abi_digest()
    };
    assert_eq!(
        semantics(&with_stray),
        semantics(&without),
        "a file no include named reached a digest"
    );
    Ok(())
}

/// An included file holds declarations only; a header clause in one is
/// refused, naming the clause.
#[test]
fn a_header_clause_in_an_include_is_refused_naming_the_clause() -> TestResult {
    let (surface, diagnostics) =
        pith_syntax::parse(&std::sync::Arc::new(pith_diag::SourceFile::new(
            pith_diag::SourceId::from_raw(0),
            "types.pi",
            "module example/greeting 0.1.0\nnominal Message = Text\n",
        )));
    assert!(surface.imports.is_empty());
    let wrong = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == FrontendCode::WrongDocument.stable())
        .unwrap_or_else(|| unreachable!("the header clause is refused: {diagnostics:?}"));
    assert!(
        wrong.message.0.contains("`module`"),
        "the refusal names the clause: {}",
        wrong.message.0
    );

    // End to end: the project names the include, and elaboration carries
    // the refusal with the offending file's identity.
    let root = tempfile::tempdir()?;
    let directory = root.path();
    file(
        &directory.join(PROJECT_NAME),
        "module example/greeting 0.1.0\n\ninputs {\n}\n\ninclude \"types.pi\"\n",
    )?;
    file(
        &directory.join("types.pi"),
        "inputs {\n}\n\nnominal Message = Text\n",
    )?;
    let workspace = Workspace::load(&directory.join(PROJECT_NAME))
        .unwrap_or_else(|diagnostics| unreachable!("the project resolves: {diagnostics:?}"));
    let diagnostics = match workspace.elaborate() {
        Ok(_) => unreachable!("an include carrying a header clause elaborates"),
        Err(pith_loader::ElaborateError::Diagnostics(diagnostics)) => diagnostics,
        Err(error) => unreachable!("the refusal is diagnostic: {error}"),
    };
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.code
            == FrontendCode::WrongDocument.stable()
            && diagnostic
                .source
                .as_ref()
                .is_some_and(|source| source.label.as_ref() == "types.pi")),
        "the refusal names the include: {diagnostics:?}"
    );
    Ok(())
}

/// The first command that resolves inputs writes the lock beside the
/// project file, recording the content identity of every resolved input;
/// content that moves rewrites it, because a path input pins nothing.
#[test]
fn resolving_inputs_writes_and_rewrites_the_lock_beside_the_project() -> TestResult {
    let root = tempfile::tempdir()?;
    let directory = root.path();
    file(
        &directory.join(PROJECT_NAME),
        &format!("module example/root 0.1.0\n\ninputs {{\n  dep = path \"dep\"\n}}\n\n{ENTRY}"),
    )?;
    file(&directory.join("dep").join(PROJECT_NAME), DEP_PROJECT)?;
    file(&directory.join("dep/types.pi"), TYPES)?;
    file(&directory.join("dep/rules.pi"), RULES)?;

    let workspace = Workspace::load(&directory.join(PROJECT_NAME))
        .unwrap_or_else(|diagnostics| unreachable!("the project resolves: {diagnostics:?}"));
    write_lock(&workspace, &directory.join(PROJECT_NAME))
        .unwrap_or_else(|error| unreachable!("the lock writes: {error}"));
    let lock = fs::read_to_string(directory.join(LOCK_NAME))?;
    assert!(
        lock.starts_with("pith lock 1\n"),
        "the first line names the format: {lock}"
    );
    assert!(
        lock.contains("example/dep 0.1 "),
        "the entry records the subject and its canonical version, `0.1.0` written to `0.1`:          {lock}"
    );
    assert!(
        lock.lines().count() == 2,
        "one line per input plus the format line: {lock}"
    );

    file(
        &directory.join("dep/rules.pi"),
        "pure rule speak(who: Message) -> Text = { \"second\" }\n",
    )?;
    let moved = Workspace::load(&directory.join(PROJECT_NAME))
        .unwrap_or_else(|diagnostics| unreachable!("the project resolves: {diagnostics:?}"));
    write_lock(&moved, &directory.join(PROJECT_NAME))
        .unwrap_or_else(|error| unreachable!("the lock rewrites: {error}"));
    let rewritten = fs::read_to_string(directory.join(LOCK_NAME))?;
    assert_ne!(
        lock, rewritten,
        "content that moved left the lock recording what no longer holds"
    );
    Ok(())
}
