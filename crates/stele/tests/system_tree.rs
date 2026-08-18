//! The composed artifact under the first-party confined executor, on the
//! host that has one. This is where the milestone's two executor findings are
//! measured for the domain: an action whose child creates symlinks inside a
//! declared tree output, and those entries surviving capture as symlinks
//! rather than dereferenced copies.

#![cfg(target_os = "linux")]

#[path = "support/system.rs"]
mod system;

use pith_engine::{AllowAllActions, Engine, EvaluationSource, TokioRuntime};
use pith_executor_local::LocalExecutor;
use pith_store::materialize_tree;
use stele::SteleEngine;
use stele::discover::tools_closure;
use system::{
    ArtifactEntry, HOSTS, UNIT_NAME, artifact_entries, artifact_id, confined_tools_value,
    find_tool, fixture, open_store, tool_paths,
};
use tempfile::TempDir;

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

fn durable_engine(root: &std::path::Path) -> Engine {
    let store = match pith_store::FilesystemContentStore::open(root) {
        Ok(store) => store,
        Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
    };
    let state = match pith_state_sqlite::SqliteEngineStateStore::open(root.join("state.db")) {
        Ok(state) => state,
        Err(error) => unreachable!("the engine-state database failed to open: {error:?}"),
    };
    let mut engine = Engine::with_state_store(store, state);
    engine.register_stele();
    engine
}

fn compose(
    engine: &mut Engine,
    request: &pith_core::Request<pith_core::Pure>,
) -> pith_engine::Evaluation {
    let executor = LocalExecutor::new();
    match engine.run(request, &runtime(), &AllowAllActions, &executor) {
        Ok(Ok(evaluation)) => evaluation,
        Ok(Err(diagnostics)) => unreachable!("the confined compose failed: {diagnostics:?}"),
        Err(error) => unreachable!("the runtime could not drive the run: {error:?}"),
    }
}

/// A host without the five tools cannot run the confined assembly; one that
/// has them runs it for real.
fn host_has_the_tools() -> bool {
    ["sh", "mkdir", "cat", "chmod", "ln"]
        .iter()
        .all(|tool| find_tool(tool).is_some())
}

/// The five tools plus the closure a confined child opens to run them. The
/// interpreter is in the closure, so the child can start at all.
fn confined_tools() -> Option<pith_core::Value> {
    let paths = tool_paths()?;
    let closure = tools_closure(&paths.iter().map(String::as_str).collect::<Vec<_>>());
    confined_tools_value(&closure.iter().map(String::as_str).collect::<Vec<_>>())
}

#[test]
fn a_confined_assembly_produces_symlinks_that_survive_capture() {
    if !host_has_the_tools() {
        return;
    }
    let Some(tools) = confined_tools() else {
        return;
    };
    let root = temp_root();
    let mut engine = durable_engine(root.path());
    let system = fixture(&mut engine, tools);
    let evaluation = compose(&mut engine, &system.request());

    let store = open_store(root.path());
    let entries = artifact_entries(&store, artifact_id(&evaluation));
    match entries.get("etc/hosts") {
        Some(ArtifactEntry::File { bytes, executable }) => {
            assert_eq!(bytes.as_slice(), HOSTS);
            assert!(!*executable);
        }
        other => unreachable!("etc/hosts should be a file, found {other:?}"),
    }
    match entries.get("etc/localtime") {
        Some(ArtifactEntry::Symlink { target }) => {
            assert_eq!(target.as_slice(), b"../pool/zoneinfo/UTC");
        }
        other => unreachable!("etc/localtime should be a symlink, found {other:?}"),
    }
    match entries.get("etc/hosts-link") {
        Some(ArtifactEntry::Symlink { target }) => {
            assert_eq!(target.as_slice(), b"hosts");
        }
        other => unreachable!("etc/hosts-link should be a symlink, found {other:?}"),
    }
    match entries.get("etc/profile.d/welcome.sh") {
        Some(ArtifactEntry::File { executable, .. }) => {
            assert!(*executable, "a declared mode survived a confined assembly");
        }
        other => unreachable!("welcome.sh should be a file, found {other:?}"),
    }
    assert!(
        entries.contains_key(&format!("etc/systemd/system/{UNIT_NAME}")),
        "the rendered unit file is in the artifact"
    );

    // What the store asserts by identity, the filesystem asserts by reading:
    // the artifact materializes with its links as links.
    let destination = root.path().join("materialized");
    if let Err(error) = materialize_tree(&store, artifact_id(&evaluation), &destination) {
        unreachable!("the artifact should materialize: {error:?}");
    }
    let localtime = destination.join("etc/localtime");
    match std::fs::read_link(&localtime) {
        Ok(target) => assert_eq!(target, std::path::Path::new("../pool/zoneinfo/UTC")),
        Err(error) => unreachable!("etc/localtime should materialize as a link: {error:?}"),
    }
    match std::fs::read(destination.join("etc/hosts-link/does-not-matter")) {
        Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::NotADirectory),
        Ok(_) => unreachable!("a dangling link should not resolve"),
    }
    match std::fs::read(destination.join("etc/hosts-link")) {
        Ok(bytes) => assert_eq!(bytes, HOSTS),
        Err(error) => unreachable!("a resolving link should read through: {error:?}"),
    }
}

#[test]
fn the_confined_assembly_is_served_on_a_second_compose_and_hydrates_after() {
    if !host_has_the_tools() {
        return;
    }
    let Some(tools) = confined_tools() else {
        return;
    };
    let root = temp_root();
    let mut engine = durable_engine(root.path());
    let system = fixture(&mut engine, tools);
    let first = compose(&mut engine, &system.request());
    assert_eq!(first.source, EvaluationSource::Computed);

    let second = compose(&mut engine, &system.request());
    assert_eq!(second.source, EvaluationSource::Reused);
    assert_eq!(artifact_id(&first), artifact_id(&second));

    drop(engine);
    let mut fresh = durable_engine(root.path());
    let hydrated = compose(&mut fresh, &system.request());
    assert_eq!(hydrated.source, EvaluationSource::Hydrated);
    assert_eq!(artifact_id(&first), artifact_id(&hydrated));
}

#[test]
fn two_cold_engines_compose_the_same_artifact_identity() {
    if !host_has_the_tools() {
        return;
    }
    let Some(tools) = confined_tools() else {
        return;
    };
    let first_root = temp_root();
    let mut first_engine = durable_engine(first_root.path());
    let first_system = fixture(&mut first_engine, tools.clone());
    let first = compose(&mut first_engine, &first_system.request());

    let second_root = temp_root();
    let mut second_engine = durable_engine(second_root.path());
    let second_system = fixture(&mut second_engine, tools);
    let second = compose(&mut second_engine, &second_system.request());

    assert_eq!(
        artifact_id(&first),
        artifact_id(&second),
        "the artifact is a function of its declared inputs, not of the engine that built it"
    );
}
