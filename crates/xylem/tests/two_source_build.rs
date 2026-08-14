//! Milestone M-3: a two-source C build through xylem, with fine-grained
//! rebuilds and discovered header dependencies.
//!
//! Two C sources share a header and link to one executable. The tests cover
//! the properties that matter: touching one source recompiles only its object
//! and leaves the other's discovery and compile in the reusable action index
//! (U-5), editing the shared header rebuilds both objects because each
//! compile's planned contract stages the header's new content (decision 0034),
//! an undeclared header fails the compile inside the sandbox rather than
//! reading the host filesystem (decision 0030), two cold compiles of the same
//! source produce byte-identical objects (0014 determinism), and the linked
//! executable runs and exits with the value its sources compute.
//!
//! It skips when the host has no nix-store C compiler, and only then: a
//! driver that is present but fails discovery fails the test rather than
//! skipping it green. The subject is xylem's integration with the kernel, not
//! the availability of a toolchain.

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
use xylem::{BuildEngine, DiscoveryError, HeaderUniverse, Toolchain, Toolchains, types};

const SOURCE_A: &[u8] = b"#include \"answer.h\"\nint a(void) { return ANSWER; }\n";
const SOURCE_B: &[u8] =
    b"#include \"answer.h\"\nint b(void) { return ANSWER + 1; }\nint main(void) { return a() + b(); }\n";
/// `b` without the `main`, for builds whose main lives in its own source.
const SOURCE_B_NO_MAIN: &[u8] = b"#include \"answer.h\"\nint b(void) { return ANSWER + 1; }\n";
const SOURCE_A_TOUCHED: &[u8] = b"#include \"answer.h\"\nint a(void) { return ANSWER + 2; }\n";
/// The third source and the main that sums all three, for the variadic-link
/// test: `a() + b() + c()` = `40 + 41 + 42` = `123`.
const SOURCE_C: &[u8] = b"#include \"answer.h\"\nint c(void) { return ANSWER + 2; }\n";
const SOURCE_MAIN_THREE: &[u8] =
    b"#include \"answer.h\"\nint main(void) { return a() + b() + c(); }\n";
/// Includes a header the universe in the undeclared-header test does not
/// offer, so the discovery pass inside the sandbox cannot find it.
const SOURCE_UNDECLARED: &[u8] = b"#include \"answer.h\"\n#include \"unused.h\"\n";
const HEADER: &[u8] = b"#define ANSWER 40\nint a(void);\nint b(void);\n";
const HEADER_TOUCHED: &[u8] = b"#define ANSWER 42\nint a(void);\nint b(void);\n";
const HEADER_PATH: &str = "answer.h";
const UNUSED_PATH: &str = "unused.h";
const UNUSED: &[u8] = b"#define UNUSED 0\n";

/// A generator: writes a C source to the path it is given. Takes the output path
/// as an argument, the way a codegen tool does, so nothing about where the result
/// goes is baked into the program.
const SOURCE_GENERATOR: &[u8] = b"#include <stdio.h>\nint main(int argc, char **argv) {\n  if (argc < 2) return 1;\n  FILE *out = fopen(argv[1], \"w\");\n  if (!out) return 1;\n  fputs(\"int generated(void) { return 7; }\\n\", out);\n  return fclose(out) == 0 ? 0 : 1;\n}\n";
/// A second generator emitting a different constant, for the invalidation test.
const SOURCE_GENERATOR_TOUCHED: &[u8] = b"#include <stdio.h>\nint main(int argc, char **argv) {\n  if (argc < 2) return 1;\n  FILE *out = fopen(argv[1], \"w\");\n  if (!out) return 1;\n  fputs(\"int generated(void) { return 9; }\\n\", out);\n  return fclose(out) == 0 ? 0 : 1;\n}\n";
/// A main that exits with what the generated source returns, so the built
/// program's exit status reports which generator produced it.
const SOURCE_USES_GENERATED: &[u8] =
    b"int generated(void);\nint main(void) { return generated(); }\n";

/// A test program reporting success the way a test does: exit zero.
const SOURCE_TEST_PASSING: &[u8] = b"int main(void) { return 0; }\n";
/// A test program reporting a failure: a nonzero exit, which under
/// `ExitStatusContract::Reported` is a verdict rather than a broken action.
const SOURCE_TEST_FAILING: &[u8] = b"int main(void) { return 3; }\n";

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

/// A build: compile the sources, link the objects. The sources arrive as one
/// `List<CSource>` request input in link order; the build yields the linked
/// `xylem.Executable`.
struct SourcesBuild {
    toolchain_value: Value,
}

impl PureRule for SourcesBuild {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        let Value::List(sources) = inputs
            .first()
            .unwrap_or_else(|| unreachable!("a build request always supplies a source list"))
        else {
            unreachable!("a build request always supplies a source list")
        };
        Box::new(SourcesBuildFrame {
            toolchain_value: self.toolchain_value.clone(),
            sources: sources.clone(),
            phase: BuildPhase::Compiling,
        })
    }
}

struct SourcesBuildFrame {
    toolchain_value: Value,
    sources: Box<[Value]>,
    phase: BuildPhase,
}

enum BuildPhase {
    Compiling,
    Linking,
    Done,
}

impl PureRuleFrame for SourcesBuildFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        match self.phase {
            BuildPhase::Compiling => {
                let compiles = self
                    .sources
                    .iter()
                    .map(|source| {
                        Request::new(
                            "compile",
                            types::compile_interface(),
                            [self.toolchain_value.clone(), source.clone()],
                            Span::none(),
                        )
                    })
                    .collect::<Vec<_>>();
                self.phase = BuildPhase::Linking;
                Ok(PureStep::NeedAll(compiles.into_boxed_slice()))
            }
            BuildPhase::Linking => {
                let objects = match input {
                    Some(Resumption::Many(values)) if values.len() == self.sources.len() => {
                        values.clone()
                    }
                    _ => return Err(diag("the compiles did not return one object per source")),
                };
                let link = Request::<pith_core::Action>::new(
                    "link",
                    types::link_interface(),
                    [self.toolchain_value.clone(), Value::List(objects.clone())],
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

/// `List<CSource> -> Executable`: the interface of the build rule. Which
/// sources a build links, and how many, is the build description, not
/// build-library policy, so the rule lives in the test rather than in xylem.
fn build_interface() -> pith_core::Interface {
    pith_core::Interface {
        inputs: Box::new([pith_core::Type::List(Box::new(pith_core::Type::Nominal {
            name: types::C_SOURCE.into(),
        }))]),
        output: pith_core::Type::Nominal {
            name: types::EXECUTABLE.into(),
        },
    }
}

fn build_rule() -> Rule<Pure> {
    let identity = RuleIdentity::of_module_declaration("xylem-tests", "sources-build");
    let revision = RuleRevision::of_manifest(identity, b"xylem-tests-v1");
    Rule::<Pure>::new(revision, "sources-build", build_interface(), Span::none())
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

/// Drive a build that is expected to fail, and return the diagnostics it
/// failed with. The undeclared-header test lives here: the failure is the
/// claim being asserted, not a surprise.
fn run_build_expecting_failure(engine: &mut Engine, request: &Request<Pure>) -> DiagnosticSink {
    let run = engine.run(request, &runtime(), &AllowAllActions, &LocalExecutor::new());
    match run {
        Ok(Err(diagnostics)) => diagnostics,
        Ok(Ok(evaluation)) => unreachable!("the build succeeded: {evaluation:?}"),
        Err(error) => unreachable!("the runtime could not drive the run: {error:?}"),
    }
}

/// A fresh engine over the durable substrate at `root` — a filesystem content
/// store and a sqlite engine state database — with xylem's rules and the
/// two-source build rule registered for `toolchain`, and `universe` offered to
/// `#include`. Two engines built over one root are successive runs of the same
/// build; a changed `universe` between them is an edit to a shared header.
fn build_engine(root: &Path, toolchain: &Toolchain, universe: HeaderUniverse) -> (Engine, Value) {
    let store = match FilesystemContentStore::open(root) {
        Ok(store) => store,
        Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
    };
    let state = match SqliteEngineStateStore::open(root.join("state.db")) {
        Ok(state) => state,
        Err(error) => unreachable!("the engine-state database failed to open: {error:?}"),
    };
    let mut engine = Engine::with_state_store(store, state);
    let toolchain_value = toolchain.value();
    engine.register_xylem(Toolchains::one(toolchain.clone()), universe);
    engine.register_rule(
        build_rule(),
        SourcesBuild {
            toolchain_value: toolchain_value.clone(),
        },
    );
    (engine, toolchain_value)
}

/// Stage the shared header (and, for the full-universe tests, a second header
/// no source includes) into `engine`'s store and return the universe over
/// them.
fn header_universe(engine: &mut Engine, include_unused: bool) -> HeaderUniverse {
    let header = match engine.put_blob(HEADER) {
        Ok(identity) => identity,
        Err(error) => unreachable!("the store failed to hold the header: {error:?}"),
    };
    let mut entries = vec![(HEADER_PATH.into(), header)];
    if include_unused {
        let unused = match engine.put_blob(UNUSED) {
            Ok(identity) => identity,
            Err(error) => unreachable!("the store failed to hold unused.h: {error:?}"),
        };
        entries.push((UNUSED_PATH.into(), unused));
    }
    HeaderUniverse::new(entries.into_boxed_slice())
}

fn build_request(sources: &[ContentId]) -> Request<Pure> {
    Request::<Pure>::new(
        "sources-build",
        build_interface(),
        [Value::List(
            sources.iter().map(|id| types::c_source(*id)).collect(),
        )],
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

/// Discover a toolchain, skipping only on genuine absence. A driver that is
/// present but undiscoverable fails the test rather than skipping it green:
/// a skip and a pass must not be the same color, because nearly every claim
/// M-3 makes rests on these tests actually running. A skip prints, which is
/// visible under an uncaptured run (`cargo nextest run --no-capture`) — nextest
/// captures the output of passing tests, so `a_c_toolchain_is_available` is
/// what makes a compiler-less host fail rather than look green.
fn toolchain_or_skip(driver: &str) -> Option<Toolchain> {
    match Toolchain::discover(driver) {
        Ok(toolchain) => Some(toolchain),
        Err(DiscoveryError::NotFound) => {
            eprintln!("skipping: no {driver} driver on this host");
            None
        }
        // Reachable by design: the driver is on the host but could not be
        // resolved into a closure. `unreachable!` would be a lie about the
        // state space; a plain panic with the reason is the honest form.
        Err(error) => panic!("{driver} is present but discovery failed: {error}"),
    }
}

/// A host with no C compiler at all cannot run any test in this file, and a
/// green run over seventeen skips would read as a verified M-3. Fail instead:
/// installing a compiler is the fix, not reading the run as evidence.
#[test]
fn a_c_toolchain_is_available() {
    for driver in ["cc", "gcc", "clang"] {
        match Toolchain::discover(driver) {
            Ok(_toolchain) => return,
            Err(DiscoveryError::NotFound) => {}
            Err(error) => panic!("{driver} is present but discovery failed: {error}"),
        }
    }
    panic!(
        "no C compiler (cc, gcc, or clang) on this host: the M-3 fixture cannot run \
         and its other tests all skipped"
    );
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

fn store_blob(engine: &mut Engine, bytes: &[u8], what: &str) -> ContentId {
    match engine.put_blob(bytes) {
        Ok(id) => id,
        Err(error) => unreachable!("the store failed to hold {what}: {error:?}"),
    }
}

#[test]
fn a_two_source_build_produces_an_executable() {
    let Some(toolchain) = toolchain_or_skip("cc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, _) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        build_engine(root.path(), &toolchain, universe)
    };
    let source_a = store_blob(&mut engine, SOURCE_A, "a.c");
    let source_b = store_blob(&mut engine, SOURCE_B, "b.c");

    let evaluation = run_build(&mut engine, &build_request(&[source_a, source_b]));
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
    let Some(toolchain) = toolchain_or_skip("cc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, _) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        build_engine(root.path(), &toolchain, universe)
    };
    let source_a = store_blob(&mut engine, SOURCE_A, "a.c");
    let source_b = store_blob(&mut engine, SOURCE_B, "b.c");

    let evaluation = run_build(&mut engine, &build_request(&[source_a, source_b]));
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

/// Variadic linking (decision 0035): a build of four sources links all four
/// objects in one driver invocation over a `List<Object>`, and the executable
/// that comes out computes over all of them — `a() + b() + c()` = `123`. The
/// cold build runs nine actions: four discoveries, four compiles, one link.
#[test]
fn a_three_source_build_links_a_list_of_objects() {
    let Some(toolchain) = toolchain_or_skip("cc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, _) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        // A universe whose header declares all three functions, since this
        // build's main calls `c`.
        let header = match engine
            .put_blob(b"#define ANSWER 40\nint a(void);\nint b(void);\nint c(void);\n")
        {
            Ok(identity) => identity,
            Err(error) => unreachable!("the store failed to hold the header: {error:?}"),
        };
        let universe = HeaderUniverse::new(vec![(HEADER_PATH.into(), header)].into_boxed_slice());
        build_engine(root.path(), &toolchain, universe)
    };
    let source_a = store_blob(&mut engine, SOURCE_A, "a.c");
    let source_b = store_blob(&mut engine, SOURCE_B_NO_MAIN, "b.c");
    let source_c = store_blob(&mut engine, SOURCE_C, "c.c");
    let source_main = store_blob(&mut engine, SOURCE_MAIN_THREE, "main3.c");

    let evaluation = run_build(
        &mut engine,
        &build_request(&[source_a, source_b, source_c, source_main]),
    );
    assert_eq!(
        action_computations(&engine),
        9,
        "the cold four-source build runs nine actions: \
         four discoveries, four compiles, one link"
    );

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
        code, 123,
        "main computes 40 + 41 + 42 over the four linked objects; got {code}"
    );
}

/// Fine-grained invalidation (U-5): touching `a.c` recompiles `a.o` and does
/// not re-run `b.o`'s discovery or compile. Both of those are served from the
/// reusable action index, so the second build adds three action computations —
/// `a`'s discovery, `a`'s compile, and the link — rather than five.
#[test]
fn touching_one_source_recompiles_only_its_object() {
    let Some(toolchain) = toolchain_or_skip("cc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, _) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, true);
        build_engine(root.path(), &toolchain, universe)
    };
    let source_a = store_blob(&mut engine, SOURCE_A, "a.c");
    let source_b = store_blob(&mut engine, SOURCE_B, "b.c");

    let _first = run_build(&mut engine, &build_request(&[source_a, source_b]));
    let after_first = action_computations(&engine);

    let source_a_touched = store_blob(&mut engine, SOURCE_A_TOUCHED, "the touched a.c");
    let _second = run_build(&mut engine, &build_request(&[source_a_touched, source_b]));
    let after_second = action_computations(&engine);

    let new_actions = after_second
        .checked_sub(after_first)
        .expect("the action count went down");
    assert_eq!(
        new_actions, 3,
        "touching a.c should re-run its discovery, recompile a.o, and re-link (3 actions), \
         not re-run b's discovery or compile (which would be 5); got {new_actions}"
    );
}

/// A rebuild under an edited header recompiles both objects (decision 0034).
/// `a.c` is touched so the root re-runs, and the header's new content is
/// offered through a changed universe. `b`'s compile entry has an unchanged
/// pure key, so the measurement is on the walk 0033 built: serving it from the
/// index re-plans its recorded action requests against the universe this run
/// registered, the planned contracts stage the header's new content identity,
/// and both compiles — not just `a`'s — execute again. The rebuild is two
/// discoveries, two compiles, and a link, and the executable that comes out
/// answers the touched header.
#[test]
fn a_rebuild_under_an_edited_header_recompiles_both_objects() {
    let Some(toolchain) = toolchain_or_skip("cc") else {
        return;
    };
    let root = temp_root();
    let source_a;
    let source_b;
    {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, true);
        let (mut engine, _) = build_engine(root.path(), &toolchain, universe);
        source_a = store_blob(&mut engine, SOURCE_A, "a.c");
        source_b = store_blob(&mut engine, SOURCE_B, "b.c");
        let _first = run_build(&mut engine, &build_request(&[source_a, source_b]));
        assert_eq!(
            action_computations(&engine),
            5,
            "the cold build ran five actions"
        );
    }

    // The header edit: same universe shape, new content identity, delivered by
    // a fresh engine over the same durable state. The first engine is dropped
    // before the second opens, so the rebuild crosses the process boundary the
    // reusable index lives across.
    let (mut engine, _) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the store failed to reopen: {error:?}"),
        };
        let header = match engine.put_blob(HEADER_TOUCHED) {
            Ok(identity) => identity,
            Err(error) => unreachable!("the store failed to hold the touched header: {error:?}"),
        };
        let unused = match engine.put_blob(UNUSED) {
            Ok(identity) => identity,
            Err(error) => unreachable!("the store failed to hold unused.h: {error:?}"),
        };
        let universe = HeaderUniverse::new(
            vec![(HEADER_PATH.into(), header), (UNUSED_PATH.into(), unused)].into_boxed_slice(),
        );
        build_engine(root.path(), &toolchain, universe)
    };

    // `a.c` is touched alongside the header, so the root re-runs. `b`'s entry
    // is served from the index only after its action edges re-plan.
    let source_a_touched = store_blob(&mut engine, SOURCE_A_TOUCHED, "the touched a.c");
    let evaluation = run_build(&mut engine, &build_request(&[source_a_touched, source_b]));
    assert_eq!(
        action_computations(&engine),
        5,
        "an edited header must re-run both discoveries, both compiles, and the link; \
         a smaller count means a stale object was served"
    );

    // The rebuilt executable answers the touched header, which is the proof
    // the recompiles actually read the new content: a() is ANSWER+2 = 44,
    // b() is ANSWER+1 = 43.
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
        code, 87,
        "main computes (42+2) + (42+1) against the touched header"
    );
}

/// A header the universe does not offer is a loud failure inside the sandbox,
/// not a compile against the host filesystem (decisions 0030, 0034). Landlock
/// confines the discovery pass to the staged universe, so the preprocessor's
/// `#include` resolves nowhere and the tool reports it; nothing outside the
/// declared set can be read instead.
#[test]
fn an_undeclared_header_fails_rather_than_reading_the_host() {
    let Some(toolchain) = toolchain_or_skip("cc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, _) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        build_engine(root.path(), &toolchain, universe)
    };
    let source = store_blob(&mut engine, SOURCE_UNDECLARED, "undeclared.c");

    let diagnostics = run_build_expecting_failure(
        &mut engine,
        &types::compile_request(toolchain.value(), source),
    );
    let text: Vec<String> = diagnostics
        .iter()
        .map(|d| d.message.0.as_ref().to_owned())
        .collect();
    assert!(
        text.iter().any(|message| message.contains("unused.h")),
        "the diagnostics should name the undeclared header; got: {text:?}"
    );
}

/// A second build of unchanged sources reuses its root (decision 0033). Every
/// target of a build sits above an action, so before the consumer of an action
/// could be reused this was the whole pure half of the graph re-running on every
/// build. Nothing new is computed and no action is planned.
/// Build `source` into an executable and return its content identity.
fn built_executable(engine: &mut Engine, source: &[u8], what: &str) -> ContentId {
    let id = store_blob(engine, source, what);
    blob_of(&run_build(engine, &build_request(&[id])).value)
}

/// An engine registered with both toolchains, so one registration of each rule
/// serves both and the toolchain a request names is what selects the closure.
fn two_toolchain_engine(
    root: &Path,
    gcc: &Toolchain,
    clang: &Toolchain,
    universe: HeaderUniverse,
) -> Engine {
    let store = match FilesystemContentStore::open(root) {
        Ok(store) => store,
        Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
    };
    let state = match SqliteEngineStateStore::open(root.join("state.db")) {
        Ok(state) => state,
        Err(error) => unreachable!("the engine-state database failed to open: {error:?}"),
    };
    let mut engine = Engine::with_state_store(store, state);
    engine.register_xylem(
        Toolchains::new(Box::new([gcc.clone(), clang.clone()])),
        universe,
    );
    engine.register_rule(
        build_rule(),
        SourcesBuild {
            toolchain_value: gcc.value(),
        },
    );
    engine
}

#[test]
fn two_toolchains_compile_the_same_source_without_sharing_a_cache_entry() {
    let Some(gcc) = toolchain_or_skip("gcc") else {
        return;
    };
    let Some(clang) = toolchain_or_skip("clang") else {
        return;
    };
    // Without this the test could be one compiler twice and still pass.
    assert_ne!(
        gcc.driver, clang.driver,
        "the two drivers must be different programs"
    );

    let root = temp_root();
    let mut engine = {
        let mut store_engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut store_engine, false);
        two_toolchain_engine(root.path(), &gcc, &clang, universe)
    };
    let source = store_blob(&mut engine, SOURCE_A, "a.c");

    let under_gcc = run_build(&mut engine, &types::compile_request(gcc.value(), source));
    let after_gcc = action_computations(&engine);
    let under_clang = run_build(&mut engine, &types::compile_request(clang.value(), source));
    let after_clang = action_computations(&engine);
    let gcc_again = run_build(&mut engine, &types::compile_request(gcc.value(), source));

    // The toolchain is a request input, so clang's compile is a different
    // request that plans a different contract and cannot be answered from gcc's
    // entry.
    assert_eq!(under_gcc.source, EvaluationSource::Computed);
    assert_eq!(under_clang.source, EvaluationSource::Computed);
    assert!(
        after_clang > after_gcc,
        "clang's compile must plan its own actions, not reuse gcc's"
    );
    assert_ne!(
        blob_of(&under_gcc.value),
        blob_of(&under_clang.value),
        "two compilers must not produce byte-identical objects here"
    );
    assert_eq!(
        gcc_again.source,
        EvaluationSource::Reused,
        "the first toolchain's entry survives the second toolchain's build"
    );
}

#[test]
fn a_toolchain_the_build_was_not_registered_with_is_refused() {
    let Some(gcc) = toolchain_or_skip("gcc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, _) = {
        let mut store_engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut store_engine, false);
        build_engine(root.path(), &gcc, universe)
    };
    let source = store_blob(&mut engine, SOURCE_A, "a.c");

    // A driver nothing registered has no closure to confine, and planning a
    // contract against a guessed one would be the ambient discovery 0007 forbids.
    let diagnostics = run_build_expecting_failure(
        &mut engine,
        &types::compile_request(types::toolchain("/nowhere/cc"), source),
    );
    let message = diagnostics
        .iter()
        .next()
        .map(|diagnostic| diagnostic.message.0.to_string())
        .unwrap_or_default();
    assert!(
        message.contains("was not registered with"),
        "the error should name the unregistered toolchain, got: {message}"
    );
}

#[test]
fn a_generated_source_is_compiled_and_linked_through_the_graph() {
    let Some(toolchain) = toolchain_or_skip("cc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, toolchain_value) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        build_engine(root.path(), &toolchain, universe)
    };
    let generator = built_executable(&mut engine, SOURCE_GENERATOR, "generator.c");

    // The generated source is a value the graph produced, so the compile
    // depends on the generate action.
    let generated = blob_of(
        &run_build(
            &mut engine,
            &types::generate_request(toolchain_value.clone(), generator),
        )
        .value,
    );
    let main = store_blob(&mut engine, SOURCE_USES_GENERATED, "main.c");
    let program = blob_of(&run_build(&mut engine, &build_request(&[generated, main])).value);

    // The generator wrote a function returning 7, so a program that exits
    // nonzero ran code the generate action produced: the generated source
    // compiled, linked, and behaved as it was written.
    let verdict = run_build(&mut engine, &types::test_request(toolchain_value, program));
    assert_eq!(verdict.value, types::test_report(false));
}

#[test]
fn touching_the_generator_regenerates_the_source_and_relinks() {
    let Some(toolchain) = toolchain_or_skip("cc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, toolchain_value) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        build_engine(root.path(), &toolchain, universe)
    };
    let main = store_blob(&mut engine, SOURCE_USES_GENERATED, "main.c");

    let first_generator = built_executable(&mut engine, SOURCE_GENERATOR, "generator.c");
    let first_generated = blob_of(
        &run_build(
            &mut engine,
            &types::generate_request(toolchain_value.clone(), first_generator),
        )
        .value,
    );
    let first_program =
        blob_of(&run_build(&mut engine, &build_request(&[first_generated, main])).value);

    // A changed generator is a different program, so the generate action's
    // contract names different content and derives a different key (0031, 0036).
    let second_generator =
        built_executable(&mut engine, SOURCE_GENERATOR_TOUCHED, "generator.c touched");
    let second_generated = blob_of(
        &run_build(
            &mut engine,
            &types::generate_request(toolchain_value, second_generator),
        )
        .value,
    );
    let second_program =
        blob_of(&run_build(&mut engine, &build_request(&[second_generated, main])).value);

    assert_ne!(
        first_generator, second_generator,
        "the two generators must differ for this test to mean anything"
    );
    assert_ne!(
        first_generated, second_generated,
        "a changed generator must produce a changed source"
    );
    assert_ne!(
        first_program, second_program,
        "a changed generated source must produce a changed executable"
    );
}

#[test]
fn a_passing_test_reports_a_passing_verdict() {
    let Some(toolchain) = toolchain_or_skip("cc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, toolchain_value) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        build_engine(root.path(), &toolchain, universe)
    };
    let executable = built_executable(&mut engine, SOURCE_TEST_PASSING, "passing.c");

    // The program under test is the action's program, staged from the store and
    // run confined (decisions 0036, 0028).
    let verdict = run_build(
        &mut engine,
        &types::test_request(toolchain_value, executable),
    );

    assert_eq!(verdict.value, types::test_report(true));
}

#[test]
fn a_failing_test_is_a_verdict_rather_than_a_failed_build() {
    let Some(toolchain) = toolchain_or_skip("cc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, toolchain_value) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        build_engine(root.path(), &toolchain, universe)
    };
    let executable = built_executable(&mut engine, SOURCE_TEST_FAILING, "failing.c");

    // `run_build` fails the test on a failed run, so reaching an assertion at
    // all is half the claim: the nonzero exit did not fail the action.
    let verdict = run_build(
        &mut engine,
        &types::test_request(toolchain_value, executable),
    );

    assert_eq!(verdict.value, types::test_report(false));
}

#[test]
fn an_unchanged_failing_test_is_served_rather_than_re_run() {
    let Some(toolchain) = toolchain_or_skip("cc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, toolchain_value) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        build_engine(root.path(), &toolchain, universe)
    };
    let executable = built_executable(&mut engine, SOURCE_TEST_FAILING, "failing.c");
    let request = types::test_request(toolchain_value, executable);

    let first = run_build(&mut engine, &request);
    let after_first = action_computations(&engine);
    let second = run_build(&mut engine, &request);

    // A failed computation is not in the reusable index, so this is the case a
    // failed action could not have served.
    assert_eq!(first.source, EvaluationSource::Computed);
    assert_eq!(second.source, EvaluationSource::Reused);
    assert_eq!(first.value, second.value);
    assert_eq!(second.value, types::test_report(false));
    assert_eq!(
        action_computations(&engine),
        after_first,
        "a reused verdict plans no action"
    );
}

#[test]
fn a_second_build_of_unchanged_sources_reuses_its_root() {
    let Some(toolchain) = toolchain_or_skip("cc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, _) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        build_engine(root.path(), &toolchain, universe)
    };
    let source_a = store_blob(&mut engine, SOURCE_A, "a.c");
    let source_b = store_blob(&mut engine, SOURCE_B, "b.c");
    let request = build_request(&[source_a, source_b]);

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
    let Some(toolchain) = toolchain_or_skip("cc") else {
        return;
    };
    let root = temp_root();
    let (source_a, source_b, computed) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        let (mut engine, _) = build_engine(root.path(), &toolchain, universe);
        let source_a = store_blob(&mut engine, SOURCE_A, "a.c");
        let source_b = store_blob(&mut engine, SOURCE_B, "b.c");
        let computed = run_build(&mut engine, &build_request(&[source_a, source_b]));
        assert_eq!(computed.source, EvaluationSource::Computed);
        (source_a, source_b, computed.value)
    };

    let (mut engine, _) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the store failed to reopen: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        build_engine(root.path(), &toolchain, universe)
    };
    let hydrated = run_build(&mut engine, &build_request(&[source_a, source_b]));

    assert_eq!(hydrated.source, EvaluationSource::Hydrated);
    assert_eq!(hydrated.value, computed);
    assert_eq!(
        action_computations(&engine),
        0,
        "a hydrated root allocates no computation beneath it"
    );
}

/// Determinism (0014): two cold compiles of the same source over the same
/// header universe produce byte-identical objects. Caching is switched off so
/// both compiles actually run.
#[test]
fn two_cold_compiles_of_the_same_source_are_identical() {
    let Some(toolchain) = toolchain_or_skip("cc") else {
        return;
    };
    let root = temp_root();
    let (mut engine, toolchain_value) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        build_engine(root.path(), &toolchain, universe)
    };
    engine.set_action_caching(false);
    let source = store_blob(&mut engine, SOURCE_A, "a.c");

    let compile = types::compile_request(toolchain_value, source);
    let first = blob_of(&run_build(&mut engine, &compile).value);
    let second = blob_of(&run_build(&mut engine, &compile).value);
    assert_eq!(
        first, second,
        "two cold compiles of the same source produced different objects"
    );
}
