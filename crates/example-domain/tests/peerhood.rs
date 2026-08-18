//! What a domain outside the first-party set gets from the kernel.
//!
//! Requirement U-10 asks for a test that an external library can extend the
//! first-party set "without hidden hooks". These are that test. A template
//! renderer is here for the proof and for nothing else. What is asserted is
//! that registering two rules through the public engine is enough to get the
//! properties the first-party domains are measured on: an action
//! planned from a contract, a result served from the reusable index on the
//! second request, the same result hydrated in a later process, and a failure
//! that stops before anything runs.

#[path = "support/renderer.rs"]
mod renderer;

use std::path::Path;

use example_domain::{ExampleEngine, types};
use pith_core::{Action, Pure, Request, Value};
use pith_diag::{DiagnosticSink, Span, StableCode};
use pith_engine::{
    AllowAllActions, ComputationKind, Engine, Evaluation, EvaluationSource, TokioRuntime,
};
use pith_ids::ContentId;
use pith_state_sqlite::SqliteEngineStateStore;
use pith_store::{ContentStore, FilesystemContentStore};
use renderer::{OTHER_RENDERER, RENDERER, RendererExecutor};
use tempfile::TempDir;

const TEMPLATE: &[u8] = b"hello {{name}}, welcome to {{place}}\n";
const RENDERED: &[u8] = b"hello world, welcome to pith\n";
const UNBOUND_TEMPLATE: &[u8] = b"hello {{name}}, welcome to {{elsewhere}}\n";

fn temp_root() -> TempDir {
    match TempDir::new() {
        Ok(root) => root,
        Err(error) => unreachable!("could not create a temporary directory: {error:?}"),
    }
}

fn runtime() -> TokioRuntime {
    match TokioRuntime::new() {
        Ok(runtime) => runtime,
        Err(error) => unreachable!("could not build a tokio runtime: {error:?}"),
    }
}

/// An engine over a durable content store and engine-state database at `root`,
/// with this domain registered. Two engines built over one root are successive
/// runs of the same work, which is what the hydration test needs.
fn durable_engine(root: &Path) -> Engine {
    let store = match FilesystemContentStore::open(root) {
        Ok(store) => store,
        Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
    };
    let state = match SqliteEngineStateStore::open(root.join("state.db")) {
        Ok(state) => state,
        Err(error) => unreachable!("the engine-state database failed to open: {error:?}"),
    };
    let mut engine = Engine::with_state_store(store, state);
    engine.register_example_domain();
    engine
}

fn store_blob(engine: &mut Engine, bytes: &[u8], what: &str) -> ContentId {
    match engine.put_blob(bytes) {
        Ok(id) => id,
        Err(error) => unreachable!("the store failed to hold {what}: {error:?}"),
    }
}

fn render(engine: &mut Engine, request: &Request<Pure>, executor: &RendererExecutor) -> Evaluation {
    match engine.run(request, &runtime(), &AllowAllActions, executor) {
        Ok(Ok(evaluation)) => evaluation,
        Ok(Err(diagnostics)) => unreachable!("the render failed: {diagnostics:?}"),
        Err(error) => unreachable!("the runtime could not drive the run: {error:?}"),
    }
}

fn render_expecting_failure(
    engine: &mut Engine,
    request: &Request<Pure>,
    executor: &RendererExecutor,
) -> DiagnosticSink {
    match engine.run(request, &runtime(), &AllowAllActions, executor) {
        Ok(Err(diagnostics)) => diagnostics,
        Ok(Ok(evaluation)) => unreachable!("the render succeeded: {evaluation:?}"),
        Err(error) => unreachable!("the runtime could not drive the run: {error:?}"),
    }
}

/// The document bytes a completed render names, read through a second handle on
/// the store, since the engine owns the one it was built with.
fn document_bytes(root: &Path, evaluation: &Evaluation) -> Vec<u8> {
    let Value::Nominal {
        name,
        representation,
    } = &evaluation.value
    else {
        unreachable!(
            "a render completes with a nominal value, not {:?}",
            evaluation.value
        );
    };
    assert_eq!(name.as_ref(), types::document().name());
    let Value::Blob(id) = representation.as_ref() else {
        unreachable!("a document carries content, not {representation:?}");
    };
    let store = match FilesystemContentStore::open(root) {
        Ok(store) => store,
        Err(error) => unreachable!("the store failed to reopen: {error:?}"),
    };
    match store.get_blob(*id) {
        Ok(Some(blob)) => blob.as_bytes().to_vec(),
        Ok(None) => unreachable!("the document was not in the store"),
        Err(error) => unreachable!("the store failed to read the document: {error:?}"),
    }
}

fn action_computations(engine: &Engine) -> usize {
    engine
        .query()
        .computations()
        .filter(|(_, node)| matches!(node.kind, ComputationKind::Action(_)))
        .count()
}

fn bound() -> Value {
    types::bindings_value([("name", "world"), ("place", "pith")])
}

#[test]
fn a_domain_the_kernel_does_not_know_about_renders_through_the_public_engine() {
    let root = temp_root();
    let mut engine = durable_engine(root.path());
    let executor = RendererExecutor::default();
    let program = store_blob(&mut engine, RENDERER, "the renderer");
    let template = store_blob(&mut engine, TEMPLATE, "the template");

    let evaluation = render(
        &mut engine,
        &types::render_request(program, template, bound()),
        &executor,
    );

    assert_eq!(evaluation.source, EvaluationSource::Computed);
    assert_eq!(document_bytes(root.path(), &evaluation), RENDERED);
    assert_eq!(executor.executions(), 1);
}

#[test]
fn the_second_request_for_one_document_is_served_from_the_reusable_index() {
    let root = temp_root();
    let mut engine = durable_engine(root.path());
    let executor = RendererExecutor::default();
    let program = store_blob(&mut engine, RENDERER, "the renderer");
    let template = store_blob(&mut engine, TEMPLATE, "the template");
    let request = types::render_request(program, template, bound());

    let first = render(&mut engine, &request, &executor);
    let second = render(&mut engine, &request, &executor);

    assert_eq!(first.source, EvaluationSource::Computed);
    assert_eq!(
        second.source,
        EvaluationSource::Reused,
        "a peer's completed computation is reusable on the same terms as a first-party one"
    );
    assert_eq!(
        executor.executions(),
        1,
        "the renderer ran once for two requests"
    );
}

#[test]
fn bindings_listed_in_two_orders_are_one_computation() {
    let root = temp_root();
    let mut engine = durable_engine(root.path());
    let executor = RendererExecutor::default();
    let program = store_blob(&mut engine, RENDERER, "the renderer");
    let template = store_blob(&mut engine, TEMPLATE, "the template");

    let first = render(
        &mut engine,
        &types::render_request(
            program,
            template,
            types::bindings_value([("name", "world"), ("place", "pith")]),
        ),
        &executor,
    );
    let second = render(
        &mut engine,
        &types::render_request(
            program,
            template,
            types::bindings_value([("place", "pith"), ("name", "world")]),
        ),
        &executor,
    );

    assert_eq!(first.source, EvaluationSource::Computed);
    assert_eq!(
        second.source,
        EvaluationSource::Reused,
        "the constructor's canonical order is what makes the two requests one key"
    );
    assert_eq!(executor.executions(), 1);
}

#[test]
fn a_rebuilt_renderer_is_a_different_render() {
    let root = temp_root();
    let mut engine = durable_engine(root.path());
    let executor = RendererExecutor::default();
    let program = store_blob(&mut engine, RENDERER, "the renderer");
    let other = store_blob(&mut engine, OTHER_RENDERER, "the other renderer");
    let template = store_blob(&mut engine, TEMPLATE, "the template");

    render(
        &mut engine,
        &types::render_request(program, template, bound()),
        &executor,
    );
    let second = render(
        &mut engine,
        &types::render_request(other, template, bound()),
        &executor,
    );

    assert_eq!(
        second.source,
        EvaluationSource::Computed,
        "the program's content identity reaches the request, so a rebuilt \
         renderer does not serve the old document"
    );
    assert_eq!(executor.executions(), 2);
}

#[test]
fn a_fresh_engine_over_the_same_state_hydrates_the_render() {
    let root = temp_root();
    let (program, template) = {
        let mut engine = durable_engine(root.path());
        let executor = RendererExecutor::default();
        let program = store_blob(&mut engine, RENDERER, "the renderer");
        let template = store_blob(&mut engine, TEMPLATE, "the template");
        render(
            &mut engine,
            &types::render_request(program, template, bound()),
            &executor,
        );
        (program, template)
    };

    let mut engine = durable_engine(root.path());
    let executor = RendererExecutor::default();
    let evaluation = render(
        &mut engine,
        &types::render_request(program, template, bound()),
        &executor,
    );

    assert_eq!(
        evaluation.source,
        EvaluationSource::Hydrated,
        "a peer's recorded attempt revalidates and loads in a later process"
    );
    assert_eq!(executor.executions(), 0);
    assert_eq!(document_bytes(root.path(), &evaluation), RENDERED);
}

#[test]
fn an_unbound_placeholder_fails_before_the_renderer_runs() {
    let root = temp_root();
    let mut engine = durable_engine(root.path());
    let executor = RendererExecutor::default();
    let program = store_blob(&mut engine, RENDERER, "the renderer");
    let template = store_blob(&mut engine, UNBOUND_TEMPLATE, "the template");

    let diagnostics = render_expecting_failure(
        &mut engine,
        &types::render_request(program, template, bound()),
        &executor,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == StableCode(9005) && diag.message.0.contains("elsewhere")),
        "the diagnostic names the placeholder nothing binds: {diagnostics:?}"
    );
    assert_eq!(
        action_computations(&engine),
        0,
        "a check the pure rule can make is made before an action is planned"
    );
    assert_eq!(executor.executions(), 0);
}

#[test]
fn a_name_bound_twice_is_refused() {
    let root = temp_root();
    let mut engine = durable_engine(root.path());
    let executor = RendererExecutor::default();
    let program = store_blob(&mut engine, RENDERER, "the renderer");
    let template = store_blob(&mut engine, TEMPLATE, "the template");

    let diagnostics = render_expecting_failure(
        &mut engine,
        &types::render_request(
            program,
            template,
            types::bindings_value([("name", "world"), ("place", "pith"), ("name", "elsewhere")]),
        ),
        &executor,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.0.contains("bound twice")),
        "a template with two answers is refused: {diagnostics:?}"
    );
    assert_eq!(executor.executions(), 0);
}

#[test]
fn the_planned_contract_is_inspectable_through_the_public_query_surface() {
    let root = temp_root();
    let mut engine = durable_engine(root.path());
    let program = store_blob(&mut engine, RENDERER, "the renderer");
    let template = store_blob(&mut engine, TEMPLATE, "the template");
    let request = Request::<Action>::new(
        types::RENDER,
        types::render_interface(),
        [
            types::renderer().content(program),
            types::template().content(template),
            bound(),
        ],
        Span::none(),
    );

    let plan = match engine.query().plan_action(&request) {
        Ok(plan) => plan,
        Err(diagnostics) => unreachable!("the contract did not plan: {diagnostics:?}"),
    };

    assert_eq!(
        plan.spec.executable,
        pith_core::ActionProgram::Content(program),
        "the renderer enters the contract as content"
    );
    assert_eq!(
        plan.spec.arguments.as_ref(),
        ["name=world".into(), "place=pith".into()],
        "the bindings reach the program in the canonical order the value carries"
    );
    assert_eq!(plan.spec.inputs.len(), 1);
    assert_eq!(plan.spec.outputs.len(), 1);
}
