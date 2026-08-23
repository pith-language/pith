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

#[path = "support/toolchain.rs"]
mod toolchain_support;

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
use toolchain_support::{assert_c_toolchain_available, toolchain_or_skip};
use xylem::{BuildEngine, HeaderUniverse, Toolchain, Toolchains, types};

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
                            [
                                self.toolchain_value.clone(),
                                source.clone(),
                                types::provided_headers([] as [(Box<str>, ContentId); 0]),
                            ],
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
        inputs: Box::new([pith_core::Type::List(Box::new(types::c_source_type()))]),
        output: types::executable_type(),
    }
}

fn build_rule() -> Rule<Pure> {
    let identity = RuleIdentity::of_module_declaration("xylem-tests", "sources-build");
    let revision = RuleRevision::of_manifest(identity, b"xylem-tests-v1");
    Rule::<Pure>::new(
        "xylem-tests",
        revision,
        "sources-build",
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

/// An empty provided-header set: the fixture's sources include nothing that a
/// request declares beside them.
fn no_headers() -> Value {
    types::provided_headers([] as [(Box<str>, ContentId); 0])
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

#[path = "two_source_build/basic_builds.rs"]
mod basic_builds;
#[path = "two_source_build/equivalence.rs"]
mod equivalence;
#[path = "two_source_build/generation.rs"]
mod generation;
#[path = "two_source_build/invalidation.rs"]
mod invalidation;
#[path = "two_source_build/reuse.rs"]
mod reuse;
#[path = "two_source_build/toolchains.rs"]
mod toolchains;
#[path = "two_source_build/verdicts.rs"]
mod verdicts;
