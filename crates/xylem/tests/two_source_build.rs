//! Milestone M-3: a two-source C build through xylem, with fine-grained
//! rebuilds.
//!
//! Two C sources share a header and link to one executable. The tests cover the
//! properties that matter: touching one source recompiles only its object and
//! leaves the other in the reusable action index (U-5), two cold compiles of
//! the same source produce byte-identical objects (0014 determinism), and the
//! linked executable runs and exits with the value its sources compute.
//!
//! It skips when the host has no nix-store C compiler. The subject is xylem's
//! integration with the kernel, not the availability of a toolchain.

#![cfg(target_os = "linux")]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use pith_core::{Pure, Request, Rule, RuleIdentity, RuleRevision, Value};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_engine::{
    AllowAllActions, ComputationKind, Engine, Evaluation, EvaluationSource, PureRule,
    PureRuleFrame, PureStep, Resumption, TokioRuntime,
};
use pith_executor_local::LocalExecutor;
use pith_ids::ContentId;
use pith_state_sqlite::SqliteEngineStateStore;
use pith_store::{ContentStore, FilesystemContentStore};
use xylem::{BuildEngine, Toolchain, types};

const SOURCE_A: &[u8] = b"#include \"answer.h\"\nint a(void) { return ANSWER; }\n";
const SOURCE_B: &[u8] =
    b"#include \"answer.h\"\nint b(void) { return ANSWER + 1; }\nint main(void) { return a() + b(); }\n";
const SOURCE_A_TOUCHED: &[u8] = b"#include \"answer.h\"\nint a(void) { return ANSWER + 2; }\n";
const HEADER: &[u8] = b"#define ANSWER 40\nint a(void);\nint b(void);\n";
const HEADER_PATH: &str = "answer.h";

const ELF_MAGIC: &[u8] = b"\x7fELF";
/// `main` returns `a() + b()` = `ANSWER + (ANSWER + 1)` = `81`, which the shell
/// reports as the process exit code.
const EXPECTED_EXIT_CODE: i32 = 81;

fn diag(message: &str) -> DiagnosticSink {
    let mut sink = DiagnosticSink::new();
    sink.push(Diag::new(
        Severity::Error,
        StableCode(9003),
        Span::none(),
        message,
    ));
    sink
}

/// A build: compile two sources, link the objects. The sources arrive as the
/// two request inputs (as `xylem.CSource` values); the build yields the linked
/// `xylem.Executable`.
struct TwoSourceBuild {
    toolchain_value: Value,
}

impl PureRule for TwoSourceBuild {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        let first = inputs.first().unwrap_or_else(|| {
            unreachable!("a two-source build request always supplies two source values")
        });
        let second = inputs.get(1).unwrap_or_else(|| {
            unreachable!("a two-source build request always supplies two source values")
        });
        Box::new(TwoSourceBuildFrame {
            toolchain_value: self.toolchain_value.clone(),
            sources: [first.clone(), second.clone()],
            phase: BuildPhase::Compiling,
        })
    }
}

struct TwoSourceBuildFrame {
    toolchain_value: Value,
    sources: [Value; 2],
    phase: BuildPhase,
}

enum BuildPhase {
    Compiling,
    Linking,
    Done,
}

impl PureRuleFrame for TwoSourceBuildFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        match self.phase {
            BuildPhase::Compiling => {
                let [src_a, src_b] = &self.sources;
                let compile_a = Request::new(
                    "compile",
                    types::compile_interface(),
                    [self.toolchain_value.clone(), src_a.clone()],
                    Span::none(),
                );
                let compile_b = Request::new(
                    "compile",
                    types::compile_interface(),
                    [self.toolchain_value.clone(), src_b.clone()],
                    Span::none(),
                );
                self.phase = BuildPhase::Linking;
                Ok(PureStep::NeedAll(
                    [compile_a, compile_b].to_vec().into_boxed_slice(),
                ))
            }
            BuildPhase::Linking => {
                let [obj_a, obj_b] = match input {
                    Some(Resumption::Many(values)) if values.len() == 2 => {
                        let [first, second] = values.as_ref() else {
                            unreachable!("len == 2 was checked")
                        };
                        [first.clone(), second.clone()]
                    }
                    _ => return Err(diag("the compiles did not return two objects")),
                };
                let link = Request::new(
                    "link",
                    types::link_interface(),
                    [self.toolchain_value.clone(), obj_a, obj_b],
                    Span::none(),
                );
                self.phase = BuildPhase::Done;
                Ok(PureStep::NeedAction(link))
            }
            BuildPhase::Done => match input.and_then(Resumption::one) {
                Some(executable) => Ok(PureStep::Complete(executable)),
                None => Err(diag("the link completed without an executable")),
            },
        }
    }
}

/// `(CSource, CSource) -> Executable`: the interface of the two-source build
/// rule. Which sources a build links, and how many, is the build description,
/// not build-library policy, so the rule lives in the test rather than in xylem.
fn build_interface() -> pith_core::Interface {
    pith_core::Interface {
        inputs: Box::new([
            pith_core::Type::Nominal {
                name: types::C_SOURCE.into(),
            },
            pith_core::Type::Nominal {
                name: types::C_SOURCE.into(),
            },
        ]),
        output: pith_core::Type::Nominal {
            name: types::EXECUTABLE.into(),
        },
    }
}

fn build_rule() -> Rule<Pure> {
    let identity = RuleIdentity::of_module_declaration("xylem-tests", "two-source-build");
    let revision = RuleRevision::of_manifest(identity, b"xylem-tests-v1");
    Rule::<Pure>::new(
        revision,
        "two-source-build",
        build_interface(),
        Span::none(),
    )
}

fn runtime() -> TokioRuntime {
    match TokioRuntime::new() {
        Ok(runtime) => runtime,
        Err(error) => unreachable!("could not build a tokio runtime: {error:?}"),
    }
}

fn action_computations(engine: &Engine) -> usize {
    engine
        .query()
        .computations()
        .filter(|(_, node)| matches!(node.kind, ComputationKind::Action(_)))
        .count()
}

fn run_build(engine: &mut Engine, request: &Request<Pure>) -> Evaluation {
    let run = engine.run(request, &runtime(), &AllowAllActions, &LocalExecutor::new());
    match run {
        Ok(Ok(evaluation)) => evaluation,
        Ok(Err(diagnostics)) => unreachable!("the build failed: {diagnostics:?}"),
        Err(error) => unreachable!("the runtime could not drive the run: {error:?}"),
    }
}

/// A fresh engine over the durable substrate at `root` — a filesystem content
/// store and a sqlite engine state database — with xylem's rules and the
/// two-source build rule registered for `toolchain`, and the shared header
/// staged. Two engines built over one root are successive runs of the same
/// build.
fn build_engine(root: &Path, toolchain: &Toolchain) -> (Engine, Value) {
    let store = match FilesystemContentStore::open(root) {
        Ok(store) => store,
        Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
    };
    let state = match SqliteEngineStateStore::open(root.join("state.db")) {
        Ok(state) => state,
        Err(error) => unreachable!("the engine-state database failed to open: {error:?}"),
    };
    let mut engine = Engine::with_state_store(store, state);
    let header = match engine.put_blob(HEADER) {
        Ok(identity) => identity,
        Err(error) => unreachable!("the store failed to hold the header: {error:?}"),
    };
    let toolchain_value = toolchain.value();
    engine.register_xylem(toolchain.clone(), Some((HEADER_PATH.into(), header)));
    engine.register_rule(
        build_rule(),
        TwoSourceBuild {
            toolchain_value: toolchain_value.clone(),
        },
    );
    (engine, toolchain_value)
}

fn build_request(sources: [ContentId; 2]) -> Request<Pure> {
    let [first, second] = sources;
    Request::<Pure>::new(
        "two-source-build",
        build_interface(),
        [types::c_source(first), types::c_source(second)],
        Span::none(),
    )
}

/// Extract the blob identity a nominal content value carries.
fn blob_of(value: &Value) -> ContentId {
    match value {
        Value::Nominal { representation, .. } => match representation.as_ref() {
            Value::Blob(id) => *id,
            _ => unreachable!("a nominal content value carried no blob"),
        },
        _ => unreachable!("the value was not a nominal content value"),
    }
}

fn temp_root() -> tempfile::TempDir {
    match tempfile::tempdir() {
        Ok(root) => root,
        Err(error) => unreachable!("could not create a content-store root: {error:?}"),
    }
}

/// Write `bytes` to `program` under `root` and mark it executable. The store
/// holds an executable as a plain blob with no executability bit, so the bit is
/// set here when materializing it for execution.
fn materialize_executable(root: &Path, bytes: &[u8]) -> PathBuf {
    let program = root.join("program");
    match std::fs::write(&program, bytes) {
        Ok(()) => {}
        Err(error) => unreachable!("could not write the program: {error:?}"),
    }
    match std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)) {
        Ok(()) => {}
        Err(error) => unreachable!("could not make the program executable: {error:?}"),
    }
    program
}

#[test]
fn a_two_source_build_produces_an_executable() {
    let Ok(toolchain) = Toolchain::discover("cc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, _toolchain_value) = build_engine(root.path(), &toolchain);
    let source_a = match engine.put_blob(SOURCE_A) {
        Ok(id) => id,
        Err(error) => unreachable!("the store failed to hold a.c: {error:?}"),
    };
    let source_b = match engine.put_blob(SOURCE_B) {
        Ok(id) => id,
        Err(error) => unreachable!("the store failed to hold b.c: {error:?}"),
    };

    let evaluation = run_build(&mut engine, &build_request([source_a, source_b]));
    let executable = blob_of(&evaluation.value);

    let store = match FilesystemContentStore::open(root.path()) {
        Ok(store) => store,
        Err(error) => unreachable!("the store failed to reopen: {error:?}"),
    };
    let bytes = match store.get_blob(executable) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => unreachable!("the executable was not in the store"),
        Err(error) => unreachable!("the store failed to read: {error:?}"),
    };
    assert!(
        bytes.as_bytes().starts_with(ELF_MAGIC),
        "the build output is not an ELF executable"
    );
}

/// The linked executable runs and exits with the value its sources compute.
/// `main` returns `a() + b()` = `ANSWER + (ANSWER + 1)` = `81`, which becomes
/// the process exit code. This is the truest end-to-end check: the toolchain
/// produced a program that works, not just bytes with the right magic.
#[test]
fn the_built_executable_runs_and_exits_with_the_expected_code() {
    let Ok(toolchain) = Toolchain::discover("cc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, _toolchain_value) = build_engine(root.path(), &toolchain);
    let source_a = match engine.put_blob(SOURCE_A) {
        Ok(id) => id,
        Err(error) => unreachable!("the store failed to hold a.c: {error:?}"),
    };
    let source_b = match engine.put_blob(SOURCE_B) {
        Ok(id) => id,
        Err(error) => unreachable!("the store failed to hold b.c: {error:?}"),
    };

    let evaluation = run_build(&mut engine, &build_request([source_a, source_b]));
    let executable = blob_of(&evaluation.value);

    let store = match FilesystemContentStore::open(root.path()) {
        Ok(store) => store,
        Err(error) => unreachable!("the store failed to reopen: {error:?}"),
    };
    let bytes = match store.get_blob(executable) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => unreachable!("the executable was not in the store"),
        Err(error) => unreachable!("the store failed to read: {error:?}"),
    };
    let program = materialize_executable(root.path(), bytes.as_bytes());

    let status = match Command::new(&program).status() {
        Ok(status) => status,
        Err(error) => unreachable!("could not run the program: {error:?}"),
    };
    let code = status
        .code()
        .unwrap_or_else(|| unreachable!("the program was terminated by a signal: {status}"));
    assert_eq!(
        code, EXPECTED_EXIT_CODE,
        "the program exited with {code}; the sources compute {EXPECTED_EXIT_CODE}"
    );
}

/// Fine-grained invalidation (U-5): touching `a.c` recompiles `a.o` and does
/// not recompile `b.o`. The `b.o` compile is served from the reusable action
/// index, so the second build adds one fewer action computation than a cold
/// rebuild would.
#[test]
fn touching_one_source_recompiles_only_its_object() {
    let Ok(toolchain) = Toolchain::discover("cc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, _toolchain_value) = build_engine(root.path(), &toolchain);
    let source_a = match engine.put_blob(SOURCE_A) {
        Ok(id) => id,
        Err(error) => unreachable!("the store failed to hold a.c: {error:?}"),
    };
    let source_b = match engine.put_blob(SOURCE_B) {
        Ok(id) => id,
        Err(error) => unreachable!("the store failed to hold b.c: {error:?}"),
    };

    let _first = run_build(&mut engine, &build_request([source_a, source_b]));
    let after_first = action_computations(&engine);

    let source_a_touched = match engine.put_blob(SOURCE_A_TOUCHED) {
        Ok(id) => id,
        Err(error) => unreachable!("the store failed to hold the touched a.c: {error:?}"),
    };
    let _second = run_build(&mut engine, &build_request([source_a_touched, source_b]));
    let after_second = action_computations(&engine);

    let new_actions = after_second
        .checked_sub(after_first)
        .expect("the action count went down");
    assert_eq!(
        new_actions, 2,
        "touching a.c should recompile a.o and re-link (2 actions), \
         not recompile b.o (which would be 3); got {new_actions}"
    );
}

/// A second build of unchanged sources reuses its root (decision 0033). Every
/// target of a build sits above an action, so before the consumer of an action
/// could be reused this was the whole pure half of the graph re-running on every
/// build. Nothing new is computed and no action is planned.
#[test]
fn a_second_build_of_unchanged_sources_reuses_its_root() {
    let Ok(toolchain) = Toolchain::discover("cc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, _toolchain_value) = build_engine(root.path(), &toolchain);
    let source_a = match engine.put_blob(SOURCE_A) {
        Ok(id) => id,
        Err(error) => unreachable!("the store failed to hold a.c: {error:?}"),
    };
    let source_b = match engine.put_blob(SOURCE_B) {
        Ok(id) => id,
        Err(error) => unreachable!("the store failed to hold b.c: {error:?}"),
    };
    let request = build_request([source_a, source_b]);

    let first = run_build(&mut engine, &request);
    let after_first = action_computations(&engine);
    let second = run_build(&mut engine, &request);

    assert_eq!(first.source, EvaluationSource::Computed);
    assert_eq!(second.source, EvaluationSource::Reused);
    assert_eq!(first.computation, second.computation);
    assert_eq!(first.value, second.value);
    assert_eq!(
        action_computations(&engine),
        after_first,
        "a reused root plans no action"
    );
}

/// The same build in a fresh engine over the same sqlite state and filesystem
/// content store: the root hydrates rather than recomputing. The first engine is
/// dropped before the second opens, so the result crosses a process boundary in
/// everything but name.
#[test]
fn a_fresh_engine_over_the_same_state_hydrates_the_build() {
    let Ok(toolchain) = Toolchain::discover("cc") else {
        return;
    };
    let root = temp_root();
    let (source_a, source_b, computed) = {
        let (mut engine, _toolchain_value) = build_engine(root.path(), &toolchain);
        let source_a = match engine.put_blob(SOURCE_A) {
            Ok(id) => id,
            Err(error) => unreachable!("the store failed to hold a.c: {error:?}"),
        };
        let source_b = match engine.put_blob(SOURCE_B) {
            Ok(id) => id,
            Err(error) => unreachable!("the store failed to hold b.c: {error:?}"),
        };
        let computed = run_build(&mut engine, &build_request([source_a, source_b]));
        assert_eq!(computed.source, EvaluationSource::Computed);
        (source_a, source_b, computed.value)
    };

    let (mut engine, _toolchain_value) = build_engine(root.path(), &toolchain);
    let hydrated = run_build(&mut engine, &build_request([source_a, source_b]));

    assert_eq!(hydrated.source, EvaluationSource::Hydrated);
    assert_eq!(hydrated.value, computed);
    assert_eq!(
        action_computations(&engine),
        0,
        "a hydrated root allocates no computation beneath it"
    );
}

/// Determinism (0014): two cold compiles of the same source over the same
/// header produce byte-identical objects. Caching is switched off so both
/// compiles actually run.
#[test]
fn two_cold_compiles_of_the_same_source_are_identical() {
    let Ok(toolchain) = Toolchain::discover("cc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, toolchain_value) = build_engine(root.path(), &toolchain);
    engine.set_action_caching(false);
    let source = match engine.put_blob(SOURCE_A) {
        Ok(id) => id,
        Err(error) => unreachable!("the store failed to hold a.c: {error:?}"),
    };

    let compile = types::compile_request(toolchain_value, source);
    let first = blob_of(&run_build(&mut engine, &compile).value);
    let second = blob_of(&run_build(&mut engine, &compile).value);
    assert_eq!(
        first, second,
        "two cold compiles of the same source produced different objects"
    );
}
