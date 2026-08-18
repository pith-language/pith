//! What a system composition through the public engine gets: an artifact
//! tree carrying files, rendered texts, and symlinks; reuse and hydration on
//! the engine's machinery alone; merges that fail closed before any action
//! runs; a policy that is a declared input; a replacement that is the one
//! way a value wins; and a contract whose derived script is inspectable.
//!
//! Host-agnostic, on the portable fixture executor: the same derived script
//! the confined executor runs, with confinement itself claimed nowhere
//! (`Unverified`, honestly). The linux suite drives the real executor.

#[path = "support/assembler.rs"]
mod assembler;

#[path = "support/system.rs"]
mod system;

use std::path::Path;

use assembler::AssemblerExecutor;
use pith_core::{Pure, Request, Value};
use pith_diag::{DiagnosticSink, StableCode};
use pith_engine::{
    AllowAllActions, ComputationKind, Engine, Evaluation, EvaluationSource, TokioRuntime,
};
use pith_state_sqlite::SqliteEngineStateStore;
use pith_store::FilesystemContentStore;
use stele::{SteleEngine, types};
use system::{
    ArtifactEntry, BOOT_TEXT, HOSTS, MACHINE, PASSWD_TEXT, UNIT_NAME, UNIT_TEXT, WELCOME,
    artifact_entries, artifact_id, fixture, open_store, tools_value,
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

/// An engine over a durable content store and engine-state database at `root`,
/// with this domain registered. Two engines over one root are successive runs
/// of the same work.
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
    engine.register_stele();
    engine
}

fn compose(
    engine: &mut Engine,
    request: &Request<Pure>,
    executor: &AssemblerExecutor,
) -> Evaluation {
    match engine.run(request, &runtime(), &AllowAllActions, executor) {
        Ok(Ok(evaluation)) => evaluation,
        Ok(Err(diagnostics)) => unreachable!("the compose failed: {diagnostics:?}"),
        Err(error) => unreachable!("the runtime could not drive the run: {error:?}"),
    }
}

fn compose_expecting_failure(
    engine: &mut Engine,
    request: &Request<Pure>,
    executor: &AssemblerExecutor,
) -> DiagnosticSink {
    match engine.run(request, &runtime(), &AllowAllActions, executor) {
        Ok(Err(diagnostics)) => diagnostics,
        Ok(Ok(evaluation)) => unreachable!("the compose succeeded: {evaluation:?}"),
        Err(error) => unreachable!("the runtime could not drive the run: {error:?}"),
    }
}

fn action_computations(engine: &Engine) -> usize {
    engine
        .query()
        .computations()
        .filter(|(_, node)| matches!(node.kind, ComputationKind::Action(_)))
        .count()
}

fn pure_computations(engine: &Engine) -> usize {
    engine
        .query()
        .computations()
        .filter(|(_, node)| !matches!(node.kind, ComputationKind::Action(_)))
        .count()
}

fn assert_file(
    entries: &std::collections::BTreeMap<String, ArtifactEntry>,
    path: &str,
    bytes: &[u8],
    executable: bool,
) {
    match entries.get(path) {
        Some(ArtifactEntry::File {
            bytes: found,
            executable: found_executable,
        }) => {
            assert_eq!(found.as_slice(), bytes, "the artifact carries `{path}`");
            assert_eq!(
                *found_executable, executable,
                "`{path}` carries its declared mode"
            );
        }
        other => unreachable!("`{path}` should be a file in the artifact, found {other:?}"),
    }
}

fn assert_symlink(
    entries: &std::collections::BTreeMap<String, ArtifactEntry>,
    path: &str,
    target: &[u8],
) {
    match entries.get(path) {
        Some(ArtifactEntry::Symlink { target: found }) => {
            assert_eq!(
                found.as_slice(),
                target,
                "`{path}` carries its target verbatim"
            );
        }
        other => unreachable!("`{path}` should be a symlink in the artifact, found {other:?}"),
    }
}

#[test]
fn a_composed_artifact_carries_files_texts_and_symlinks() {
    let Some(tools) = tools_value() else {
        return;
    };
    let root = temp_root();
    let mut engine = durable_engine(root.path());
    let executor = AssemblerExecutor::new();
    let system = fixture(&mut engine, tools);
    let evaluation = compose(&mut engine, &system.request(), &executor);

    assert_eq!(evaluation.source, EvaluationSource::Computed);
    let entries = artifact_entries(&open_store(root.path()), artifact_id(&evaluation));
    assert_file(&entries, "etc/hosts", HOSTS, false);
    assert_file(&entries, "etc/profile.d/welcome.sh", WELCOME, true);
    assert_file(&entries, "etc/passwd", PASSWD_TEXT.as_bytes(), false);
    assert_file(
        &entries,
        &format!("etc/systemd/system/{UNIT_NAME}"),
        UNIT_TEXT.as_bytes(),
        false,
    );
    assert_file(
        &entries,
        &format!("boot/loader/entries/{MACHINE}.conf"),
        BOOT_TEXT.as_bytes(),
        false,
    );
    assert_symlink(&entries, "etc/localtime", b"../pool/zoneinfo/UTC");
    assert_symlink(&entries, "etc/hosts-link", b"hosts");
}

#[test]
fn a_second_compose_is_reused_and_a_fresh_engine_hydrates() {
    let Some(tools) = tools_value() else {
        return;
    };
    let root = temp_root();
    let mut engine = durable_engine(root.path());
    let executor = AssemblerExecutor::new();
    let system = fixture(&mut engine, tools);
    let first = compose(&mut engine, &system.request(), &executor);
    assert_eq!(executor.executions(), 1);
    assert_eq!(first.source, EvaluationSource::Computed);

    let second = compose(&mut engine, &system.request(), &executor);
    assert_eq!(second.source, EvaluationSource::Reused);
    assert_eq!(executor.executions(), 1, "a reused compose plans no action");
    assert_eq!(artifact_id(&first), artifact_id(&second));

    drop(engine);
    let mut fresh = durable_engine(root.path());
    let hydrated = compose(&mut fresh, &system.request(), &executor);
    assert_eq!(hydrated.source, EvaluationSource::Hydrated);
    assert_eq!(
        executor.executions(),
        1,
        "a hydrated compose plans no action"
    );
    assert_eq!(artifact_id(&first), artifact_id(&hydrated));
}

#[test]
fn contribution_orders_are_one_request() {
    let Some(tools) = tools_value() else {
        return;
    };
    let root = temp_root();
    let mut engine = durable_engine(root.path());
    let executor = AssemblerExecutor::new();
    let system = fixture(&mut engine, tools);
    let _ = compose(&mut engine, &system.request(), &executor);

    let hosts = match engine.put_blob(HOSTS) {
        Ok(id) => id,
        Err(error) => unreachable!("the store failed to hold the hosts file: {error:?}"),
    };
    let welcome = match engine.put_blob(WELCOME) {
        Ok(id) => id,
        Err(error) => unreachable!("the store failed to hold the welcome script: {error:?}"),
    };
    let base_files = types::file_set_value([
        (
            "etc/hosts",
            types::FileBody::File {
                content: hosts,
                executable: false,
            },
        ),
        (
            "etc/profile.d/welcome.sh",
            types::FileBody::File {
                content: welcome,
                executable: true,
            },
        ),
    ]);
    let site_files = types::file_set_value([
        (
            "etc/localtime",
            types::FileBody::Symlink {
                target: "../pool/zoneinfo/UTC".into(),
            },
        ),
        (
            "etc/hosts-link",
            types::FileBody::Symlink {
                target: "hosts".into(),
            },
        ),
    ]);
    let reversed = types::etc_contributions(&[("site", site_files), ("base", base_files)]);
    let request = types::compose_system_request(
        system.tools.clone(),
        system.boot.clone(),
        reversed,
        system.users.clone(),
        system.policy.clone(),
        system.units.clone(),
        system.replacements.clone(),
    );

    let second = compose(&mut engine, &request, &executor);
    assert_eq!(
        second.source,
        EvaluationSource::Reused,
        "two orders of the same contributions are one request"
    );
    assert_eq!(executor.executions(), 1);
}

#[test]
fn a_unit_conflict_fails_before_any_action() {
    let Some(tools) = tools_value() else {
        return;
    };
    let root = temp_root();
    let mut engine = durable_engine(root.path());
    let executor = AssemblerExecutor::new();
    let mut system = fixture(&mut engine, tools);
    system.units = types::unit_contributions(&[
        (
            "base",
            types::unit_value(UNIT_NAME, "an example", "/bin/serve", &[], &[]),
        ),
        (
            "site",
            types::unit_value(UNIT_NAME, "an example", "/bin/other", &[], &[]),
        ),
    ]);

    let diagnostics = compose_expecting_failure(&mut engine, &system.request(), &executor);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == StableCode(9006)),
        "the refusal carries this domain's code"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.0.contains("base")
                && diagnostic.message.0.contains("site")
                && diagnostic.message.0.contains("exec")),
        "the refusal names the field and both owners"
    );
    assert_eq!(
        action_computations(&engine),
        0,
        "a refused merge plans no action"
    );
}

#[test]
fn the_policy_decides_whether_a_field_accumulates_or_must_agree() {
    let Some(tools) = tools_value() else {
        return;
    };
    let root = temp_root();
    let mut engine = durable_engine(root.path());
    let executor = AssemblerExecutor::new();
    let mut agreeing = fixture(&mut engine, tools.clone());
    agreeing.policy = types::unit_policy_value(&[]);

    let diagnostics = compose_expecting_failure(&mut engine, &agreeing.request(), &executor);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.0.contains("after")),
        "a field the policy leaves unlisted must agree"
    );
    assert_eq!(action_computations(&engine), 0);

    let accumulating = fixture(&mut engine, tools);
    let evaluation = compose(&mut engine, &accumulating.request(), &executor);
    let entries = artifact_entries(&open_store(root.path()), artifact_id(&evaluation));
    assert_file(
        &entries,
        &format!("etc/systemd/system/{UNIT_NAME}"),
        UNIT_TEXT.as_bytes(),
        false,
    );
}

#[test]
fn a_replacement_wins_and_a_stale_one_fails() {
    let Some(tools) = tools_value() else {
        return;
    };
    let root = temp_root();
    let mut engine = durable_engine(root.path());
    let executor = AssemblerExecutor::new();

    let mut replaced = fixture(&mut engine, tools.clone());
    replaced.replacements = types::unit_replacements(&[("exec", "base", "/bin/serve --quiet")]);
    let evaluation = compose(&mut engine, &replaced.request(), &executor);
    let entries = artifact_entries(&open_store(root.path()), artifact_id(&evaluation));
    assert_file(
        &entries,
        &format!("etc/systemd/system/{UNIT_NAME}"),
        UNIT_TEXT
            .replace("/bin/serve --foreground", "/bin/serve --quiet")
            .as_bytes(),
        false,
    );

    let mut stale = fixture(&mut engine, tools);
    stale.replacements = types::unit_replacements(&[("exec", "elsewhere", "/bin/serve --quiet")]);
    let diagnostics = compose_expecting_failure(&mut engine, &stale.request(), &executor);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.0.contains("base")
                && diagnostic.message.0.contains("site")),
        "a stale replacement names who declares the field now"
    );
    assert_eq!(
        action_computations(&engine),
        1,
        "only the successful replacement reaches an action"
    );
}

#[test]
fn the_contract_carries_the_derived_script_and_tool_closure() {
    let Some(tools) = tools_value() else {
        return;
    };
    let root = temp_root();
    let mut engine = durable_engine(root.path());
    let system = fixture(&mut engine, tools.clone());
    let merged = match engine.evaluate_pure(&types::compose_etc_request(system.etc.clone())) {
        Ok(evaluation) => evaluation.value,
        Err(diagnostics) => unreachable!("the etc merge is pure: {diagnostics:?}"),
    };
    let request = types::assemble_request(
        tools,
        MACHINE,
        UNIT_NAME,
        merged,
        types::unit_text().value(Value::Text("unit\n".into())),
        types::passwd_text().value(Value::Text("passwd\n".into())),
        types::boot_text().value(Value::Text("boot\n".into())),
    );

    let plan = match engine.query().plan_action(&request) {
        Ok(plan) => plan,
        Err(diagnostics) => unreachable!("the assemble action plans: {diagnostics:?}"),
    };
    assert!(
        matches!(&plan.spec.executable, pith_core::ActionProgram::HostPath(_)),
        "the assembly runs the declared shell"
    );
    assert_eq!(
        plan.spec
            .toolchain
            .iter()
            .map(|path| path.rsplit('/').next().unwrap_or(path.as_ref()))
            .collect::<Vec<_>>(),
        ["cat", "chmod", "ln", "mkdir"],
        "the closure names the four tools, canonically sorted"
    );
    let Some(script) = plan.spec.arguments.get(1) else {
        unreachable!("the script is the second argument");
    };
    assert!(
        script.contains(" -s 'hosts' 'system/etc/hosts-link'")
            && script.contains(" -s '../pool/zoneinfo/UTC' 'system/etc/localtime'"),
        "the script creates the symlinks: {script}"
    );
    assert!(
        script.contains("'system/etc/hosts'") && script.contains("'pool/etc/hosts'"),
        "the script stages under pool/ and builds under system/"
    );
    assert!(script.contains("set -eu"));
    assert!(
        script.contains(" +x 'system/etc/profile.d/welcome.sh'"),
        "a declared mode reaches the script"
    );

    let environment: Vec<&str> = plan
        .spec
        .environment
        .iter()
        .map(|variable| variable.name.as_ref())
        .collect();
    assert_eq!(environment, ["STELE_BOOT", "STELE_PASSWD", "STELE_UNIT"]);

    assert_eq!(plan.spec.outputs.len(), 1);
    assert_eq!(
        plan.spec.outputs.first().map(|output| output.path.as_ref()),
        Some("system")
    );
    assert_eq!(
        plan.spec.outputs.first().map(|output| output.kind),
        Some(pith_core::OutputKind::Tree)
    );
    assert!(
        plan.spec
            .inputs
            .iter()
            .all(|input| input.path.starts_with("pool/")),
        "staged inputs live under the disjoint pool tree"
    );
}

#[test]
fn editing_one_fragment_recomputes_its_merge_and_the_assembly_only() {
    let Some(tools) = tools_value() else {
        return;
    };
    let root = temp_root();
    let mut engine = durable_engine(root.path());
    let executor = AssemblerExecutor::new();
    let system = fixture(&mut engine, tools);
    let first = compose(&mut engine, &system.request(), &executor);
    let after_first = pure_computations(&engine);
    let actions_after_first = action_computations(&engine);
    assert_eq!(actions_after_first, 1);

    let mut edited = system;
    edited.units = types::unit_contributions(&[
        (
            "base",
            types::unit_value(
                UNIT_NAME,
                "an edited service",
                "/bin/serve --foreground",
                &["network.target"],
                &[],
            ),
        ),
        (
            "site",
            types::unit_value(
                UNIT_NAME,
                "an edited service",
                "/bin/serve --foreground",
                &["time.target"],
                &["network.target"],
            ),
        ),
    ]);
    let second = compose(&mut engine, &edited.request(), &executor);

    assert_eq!(
        second.source,
        EvaluationSource::Computed,
        "an edited fragment is a new request"
    );
    assert_ne!(
        artifact_id(&first),
        artifact_id(&second),
        "the artifact moved"
    );
    assert_eq!(
        action_computations(&engine),
        actions_after_first + 1,
        "the assembly re-ran"
    );
    let new_pure = pure_computations(&engine) - after_first;
    assert_eq!(
        new_pure, 3,
        "a unit edit recomputes the unit merge, its render, and the entry: {new_pure}"
    );
}

#[test]
fn the_merges_and_renders_are_answerable_without_an_executor() {
    let Some(tools) = tools_value() else {
        return;
    };
    let mut engine = Engine::new();
    engine.register_stele();
    let system = fixture(&mut engine, tools);

    let merges = [
        types::compose_etc_request(system.etc.clone()),
        types::compose_users_request(system.users.clone()),
        types::compose_unit_request(
            system.policy.clone(),
            system.units.clone(),
            system.replacements.clone(),
        ),
        types::render_boot_request(system.boot.clone()),
    ];
    for request in &merges {
        match engine.evaluate_pure(request) {
            Ok(_) => {}
            Err(diagnostics) => unreachable!("a merge suspends nothing: {diagnostics:?}"),
        }
    }
}
