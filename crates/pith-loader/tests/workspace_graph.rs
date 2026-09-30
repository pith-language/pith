//! The frontend graph tier, reached through a resolved workspace rather
//! than hand-built inputs. Where `edits.rs` checks equal outputs, these
//! tests read the engine's evaluation source: the claim is a reusable
//! lookup.

use std::fs;
use std::path::Path;

use pith_engine::{Engine, EvaluationSource, MemoryEngineStateStore};
use pith_loader::{FrontendInputs, RegisterFrontend, ResolvedModule, Workspace};
use pith_store::MemoryContentStore;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const ROOT_PROJECT: &str = "module example/root 0.1.0

inputs {
  dep = path \"dep\"
}

import dep

entry \
     hello : Text = ask (Message(\"hello\"))
";
const DEP_PROJECT: &str = "module example/dep 0.1.0

inputs {
}

include \"types.pi\"
include \"rules.pi\"
";
const DEP_TYPES: &str = "nominal Message = Text\n";
const DEP_RULES: &str = "pure rule speak(who: Message) -> Text = { \"first\" }\n";

/// A body edit: the rule's text moves, its signature does not.
const DEP_RULES_BODY_EDIT: &str = "pure rule speak(who: Message) -> Text = { \"second\" }\n";

/// A public representation edit: the imported nominal's representation
/// moves, so the dependency's ABI moves with it.
const DEP_TYPES_REPRESENTATION_EDIT: &str = "nominal Message = Bytes\n";

fn file(path: &Path, text: &str) -> TestResult {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

fn project(directory: &Path) -> TestResult {
    file(&directory.join("pith.pi"), ROOT_PROJECT)?;
    file(&directory.join("dep").join("pith.pi"), DEP_PROJECT)?;
    file(&directory.join("dep/types.pi"), DEP_TYPES)?;
    file(&directory.join("dep/rules.pi"), DEP_RULES)?;
    Ok(())
}

fn engine() -> Engine {
    let mut engine = Engine::with_state_store(
        MemoryContentStore::default(),
        MemoryEngineStateStore::default(),
    );
    engine.register_frontend();
    engine
}

fn load(directory: &Path) -> TestResult<Workspace> {
    Workspace::load(&directory.join("pith.pi")).map_err(
        |diagnostics| -> Box<dyn std::error::Error> {
            format!("the fixture resolves: {diagnostics:?}").into()
        },
    )
}

/// What one projection records: the inputs each module's frontend
/// computations are asked under, and the interface the dependency published.
struct Projected {
    dependency: FrontendInputs,
    root: FrontendInputs,
    dependency_abi: pith_ids::ModuleAbiDigest,
    dependency_surface: pith_ids::ContentId,
}

fn projected(workspace: &Workspace, engine: &mut Engine) -> TestResult<Projected> {
    let projection = workspace
        .project_onto_frontend(engine)
        .map_err(|error| -> Box<dyn std::error::Error> { error.to_string().into() })?;
    let dependency = projection
        .dependencies()
        .first()
        .ok_or_else(|| -> Box<dyn std::error::Error> { "the fixture has one dependency".into() })?;
    Ok(Projected {
        dependency: dependency.1.inputs().clone(),
        root: projection.root().1.inputs().clone(),
        dependency_abi: dependency.1.interface().abi(),
        dependency_surface: dependency.1.interface().surface(),
    })
}

fn bodies_source(inputs: &FrontendInputs, engine: &mut Engine) -> TestResult<EvaluationSource> {
    engine
        .evaluate_with_content(&inputs.bodies_of())
        .map(|evaluation| evaluation.source)
        .map_err(|diagnostics| -> Box<dyn std::error::Error> {
            format!("bodies-of failed: {diagnostics:?}").into()
        })
}

fn subjects(workspace: &Workspace) -> Vec<&ResolvedModule> {
    workspace.modules().collect()
}

/// The reuse cutoff through the public workspace path: editing a
/// dependency's rule body moves that module's `bodies-of` and leaves its
/// consumer's reusable, because the consumer's inputs name the dependency's
/// published surface and the body edit did not move it.
#[test]
fn a_dependency_body_edit_leaves_the_consumer_bodies_reusable() -> TestResult {
    let directory = tempfile::tempdir()?;
    project(directory.path())?;
    let mut engine = engine();

    let workspace = load(directory.path())?;
    let before = projected(&workspace, &mut engine)?;
    assert_eq!(
        bodies_source(&before.dependency, &mut engine)?,
        EvaluationSource::Computed
    );
    assert_eq!(
        bodies_source(&before.root, &mut engine)?,
        EvaluationSource::Computed
    );

    file(&directory.path().join("dep/rules.pi"), DEP_RULES_BODY_EDIT)?;
    let workspace = load(directory.path())?;
    let after = projected(&workspace, &mut engine)?;

    assert_ne!(
        before.dependency.source(),
        after.dependency.source(),
        "the body edit left the dependency's source inputs unmoved"
    );
    assert_eq!(
        before.dependency_abi, after.dependency_abi,
        "a body edit moved the dependency's ABI"
    );
    assert_eq!(
        before.dependency_surface, after.dependency_surface,
        "a body edit moved the dependency's published surface"
    );
    assert_eq!(
        before.root, after.root,
        "a dependency body edit moved the consumer's frontend inputs"
    );

    assert_eq!(
        bodies_source(&after.dependency, &mut engine)?,
        EvaluationSource::Computed,
        "the edited dependency's bodies-of was reused"
    );
    assert_eq!(
        bodies_source(&after.root, &mut engine)?,
        EvaluationSource::Reused,
        "the consumer's bodies-of recomputed across a dependency body edit"
    );
    Ok(())
}

/// The control: a public representation edit moves the dependency's ABI and
/// surface, so the consumer's inputs move with them and its `bodies-of`
/// recomputes.
#[test]
fn a_public_representation_edit_moves_the_consumer_inputs() -> TestResult {
    let directory = tempfile::tempdir()?;
    project(directory.path())?;
    let mut engine = engine();

    let workspace = load(directory.path())?;
    let before = projected(&workspace, &mut engine)?;
    assert_eq!(
        bodies_source(&before.root, &mut engine)?,
        EvaluationSource::Computed
    );

    file(
        &directory.path().join("dep/types.pi"),
        DEP_TYPES_REPRESENTATION_EDIT,
    )?;
    let workspace = load(directory.path())?;
    let after = projected(&workspace, &mut engine)?;

    assert_ne!(
        before.dependency_abi, after.dependency_abi,
        "a representation edit left the dependency's ABI"
    );
    assert_ne!(
        before.dependency_surface, after.dependency_surface,
        "a representation edit left the dependency's published surface"
    );
    assert_ne!(
        before.root.imports(),
        after.root.imports(),
        "the consumer's import environment did not name the moved surface"
    );
    assert_eq!(
        before.root.source(),
        after.root.source(),
        "the consumer's own source moved, and only its dependency was edited"
    );
    Ok(())
}

/// The projection is a function of the resolved workspace: the same bytes
/// in the same module-relative places project to the same inputs, wherever
/// the checkout lives. [`Workspace`]'s relocation property carried through
/// to the graph tier's keys, which is where a dependency acquired from
/// anywhere reuses a local one's computations.
#[test]
fn relocation_leaves_the_projected_inputs_identical() -> TestResult {
    let first_directory = tempfile::tempdir()?;
    let second_directory = tempfile::tempdir()?;
    project(first_directory.path())?;
    project(second_directory.path())?;

    let mut engine = engine();
    let first_workspace = load(first_directory.path())?;
    let second_workspace = load(second_directory.path())?;
    assert_eq!(
        subjects(&first_workspace)
            .iter()
            .map(|module| module.subject().spelling())
            .collect::<Vec<_>>(),
        subjects(&second_workspace)
            .iter()
            .map(|module| module.subject().spelling())
            .collect::<Vec<_>>()
    );

    let first = projected(&first_workspace, &mut engine)?;
    assert_eq!(
        bodies_source(&first.root, &mut engine)?,
        EvaluationSource::Computed
    );
    let second = projected(&second_workspace, &mut engine)?;

    assert_eq!(first.dependency, second.dependency);
    assert_eq!(first.root, second.root);
    assert_eq!(first.dependency_abi, second.dependency_abi);
    assert_eq!(first.dependency_surface, second.dependency_surface);

    assert_eq!(
        bodies_source(&second.root, &mut engine)?,
        EvaluationSource::Reused,
        "the relocated checkout recomputed what its twin already computed"
    );
    Ok(())
}
