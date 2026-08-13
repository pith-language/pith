//! One real toolchain, driven end to end through the engine (milestone M-3).
//!
//! Every other test that runs an action runs a fixture executor or a shell
//! script written for the occasion. This one hands a C compiler to
//! [`pith_engine::Engine::run`] and asks for an object file back, which is the
//! first time the kernel has driven a tool it did not author.
//!
//! It skips when the host has no compiler. The subject is the engine, not the
//! availability of a toolchain, and a missing `cc` is a fact about the machine.

#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::Command;

use pith_core::{
    Action, ActionInput, ActionOutput, ActionSpec, Content, EnvironmentVariable, Interface,
    NetworkPolicy, OutputKind, PlatformRequirement, Pure, Request, Rule, RuleIdentity,
    RuleRevision, Type, Value,
};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_engine::{
    ActionExecution, ActionRule, AllowAllActions, ComputationKind, Engine, ProducedOutput,
    PureRule, PureRuleFrame, PureStep, Resumption, TokioRuntime,
};
use pith_executor_local::LocalExecutor;
use pith_ids::ContentId;
use pith_store::{ContentStore, FilesystemContentStore};

mod support;

const SOURCE: &[u8] = b"int answer(void) { return 42; }\n";
const SOURCE_PATH: &str = "answer.c";
const OBJECT_PATH: &str = "answer.o";

/// The first four bytes of an ELF file. The compiler's output is checked for
/// being an object rather than for its exact bytes, which are the compiler's
/// business and not the engine's.
const ELF_MAGIC: &[u8] = b"\x7fELF";

/// The host C compiler: the driver's path, and the two search paths the driver
/// needs to find the rest of itself.
///
/// A compiler is not one executable. The driver `cc` execs `cc1` to compile and
/// `as` to assemble, and it finds them through paths baked in at *its* build
/// time, relative to where the driver itself lives. Both paths are therefore
/// asked of the driver rather than assumed: a distribution compiler and a nix
/// one keep them in different places, and neither is guessable. The driver
/// itself is referenced by its host path (decision 0030); its bytes are not
/// staged, because the driver opens the rest of its closure at baked-in
/// absolute paths only it knows.
struct HostCompiler {
    /// Absolute host path of the `cc` driver the action execves.
    driver: Box<str>,
    /// Every host path the driver reads.
    closure: Box<[Box<str>]>,
    /// Where the driver looks for `cc1` before it looks at `PATH`.
    program_path: Box<str>,
    /// The directory the driver came from, which is where `as` and `ld` live on
    /// a distribution compiler.
    tool_directory: Box<str>,
}

impl HostCompiler {
    /// `None` when there is no `cc`, or when `cc` is outside the nix store,
    /// where discovery cannot see past the loader to `cc1` and the fixed
    /// includes. A short closure would fail on an undeclared read and say
    /// nothing about the engine.
    fn discover() -> Option<Self> {
        let driver_path = find_in_path("cc")?;
        let driver = driver_path.to_str()?;
        if !support::closure_is_complete_for(driver) {
            return None;
        }
        let cc1 = print_program_path(&driver_path, "cc1")?;
        Some(Self {
            driver: driver.into(),
            closure: support::closure_for(&[driver]),
            program_path: directory_of(&cc1)?,
            tool_directory: directory_of(&driver_path)?,
        })
    }
}

fn find_in_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

/// Ask the driver where it keeps one of its own programs. Returns `None` when
/// the driver answers with a bare name, which is how it says "I expect to find
/// this on `PATH`".
fn print_program_path(driver: &Path, program: &str) -> Option<PathBuf> {
    let output = Command::new(driver)
        .arg(format!("-print-prog-name={program}"))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let answer = String::from_utf8(output.stdout).ok()?;
    let answer = PathBuf::from(answer.trim());
    answer.is_absolute().then_some(answer)
}

fn directory_of(path: &Path) -> Option<Box<str>> {
    Some(path.parent()?.to_str()?.into())
}

/// Compiles one source file to one object file. The whole contract is declared:
/// the compiler is the executable, the source is the only input, the object is
/// the only output, and the environment is two search paths and nothing else.
struct CompileAction {
    compiler: Box<str>,
    closure: Box<[Box<str>]>,
    source: ContentId,
    program_path: Box<str>,
    tool_directory: Box<str>,
}

impl ActionRule for CompileAction {
    fn plan(&self, _inputs: &[Value]) -> PithResult<ActionSpec> {
        Ok(ActionSpec {
            executable: self.compiler.clone(),
            toolchain: self.closure.clone(),
            arguments: [
                "-c".into(),
                SOURCE_PATH.into(),
                "-o".into(),
                OBJECT_PATH.into(),
            ]
            .into(),
            inputs: [ActionInput {
                path: SOURCE_PATH.into(),
                content: Content::Blob(self.source),
            }]
            .into(),
            outputs: [ActionOutput {
                path: OBJECT_PATH.into(),
                kind: OutputKind::Blob,
            }]
            .into(),
            environment: [
                EnvironmentVariable {
                    name: "COMPILER_PATH".into(),
                    value: self.program_path.clone(),
                },
                EnvironmentVariable {
                    name: "PATH".into(),
                    value: self.tool_directory.clone(),
                },
            ]
            .into(),
            platform: PlatformRequirement::Exact {
                operating_system: std::env::consts::OS.into(),
                architecture: std::env::consts::ARCH.into(),
            },
            capabilities: Box::new([]),
            network: NetworkPolicy::Deny,
        })
    }

    fn complete(&self, _inputs: &[Value], execution: &ActionExecution) -> PithResult<Value> {
        match execution.report.outputs.first() {
            Some(ProducedOutput {
                content: Content::Blob(object),
                ..
            }) => Ok(Value::Blob(*object)),
            _ => Err(test_error("the compile action captured no object file")),
        }
    }
}

/// Requests the compile and hands back whatever it produced.
struct ObjectRule {
    compile: Request<Action>,
}

impl PureRule for ObjectRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(ObjectFrame {
            compile: Some(self.compile.clone()),
        })
    }
}

struct ObjectFrame {
    compile: Option<Request<Action>>,
}

impl PureRuleFrame for ObjectFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if let Some(compile) = self.compile.take() {
            return Ok(PureStep::NeedAction(compile));
        }
        match input.and_then(Resumption::one) {
            Some(value) => Ok(PureStep::Complete(value)),
            None => Err(test_error("the compile action completed without a value")),
        }
    }
}

fn test_error(message: &str) -> DiagnosticSink {
    let mut diagnostics = DiagnosticSink::new();
    diagnostics.push(Diag::new(
        Severity::Error,
        StableCode(9001),
        Span::none(),
        message,
    ));
    diagnostics
}

fn blob_interface() -> Interface {
    Interface {
        inputs: Box::new([]),
        output: Type::Blob,
    }
}

fn rule_revision(label: &str) -> RuleRevision {
    let identity = RuleIdentity::of_module_declaration("real-toolchain-tests", label);
    RuleRevision::of_manifest(identity, b"real-toolchain-tests-v1")
}

/// An engine over a filesystem content store at `root`, with the compile action
/// and the pure rule that requests it registered, plus the request for it.
fn compile_engine(root: &Path, compiler: &HostCompiler) -> (Engine, Request<Pure>) {
    let store = match FilesystemContentStore::open(root) {
        Ok(store) => store,
        Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
    };
    let mut engine = Engine::with_content_store(store);

    // The compiler driver is referenced by host path (decision 0030), so only
    // the source blob is stored. The closure the driver reads is declared in
    // the spec's `toolchain` field, and landlock confines the child to it.
    let source_blob = match engine.put_blob(SOURCE) {
        Ok(identity) => identity,
        Err(error) => unreachable!("the store failed to hold the source: {error:?}"),
    };

    engine.register_action_rule(
        Rule::<Action>::new(
            rule_revision("compile"),
            "compile",
            blob_interface(),
            Span::none(),
        ),
        CompileAction {
            compiler: compiler.driver.clone(),
            closure: compiler.closure.clone(),
            source: source_blob,
            program_path: compiler.program_path.clone(),
            tool_directory: compiler.tool_directory.clone(),
        },
    );
    engine.register_rule(
        Rule::<Pure>::new(
            rule_revision("object"),
            "object",
            blob_interface(),
            Span::none(),
        ),
        ObjectRule {
            compile: Request::<Action>::new("compile", blob_interface(), [], Span::none()),
        },
    );

    (
        engine,
        Request::<Pure>::new("object", blob_interface(), [], Span::none()),
    )
}

fn runtime() -> TokioRuntime {
    match TokioRuntime::new() {
        Ok(runtime) => runtime,
        Err(error) => unreachable!("could not build a tokio runtime: {error:?}"),
    }
}

fn compile(engine: &mut Engine, request: &Request<Pure>) -> ContentId {
    let run = engine.run(request, &runtime(), &AllowAllActions, &LocalExecutor::new());
    let evaluation = match run {
        Ok(Ok(evaluation)) => evaluation,
        Ok(Err(diagnostics)) => unreachable!("the compile failed: {diagnostics:?}"),
        Err(error) => unreachable!("the runtime could not drive the run: {error:?}"),
    };
    match evaluation.value {
        Value::Blob(object) => object,
        other => unreachable!("the compile produced {other:?} rather than an object"),
    }
}

fn action_computations(engine: &Engine) -> usize {
    engine
        .query()
        .computations()
        .filter(|(_, node)| matches!(node.kind, ComputationKind::Action(_)))
        .count()
}

#[test]
fn a_c_compiler_produces_an_object_file_through_the_engine() {
    let Some(compiler) = HostCompiler::discover() else {
        return;
    };
    let root = match tempfile::tempdir() {
        Ok(root) => root,
        Err(error) => unreachable!("could not create a content-store root: {error:?}"),
    };
    let (mut engine, request) = compile_engine(root.path(), &compiler);

    let object = compile(&mut engine, &request);

    // The bytes the compiler wrote are in the engine's store under the identity
    // the engine gave them, which is what "the engine owns the content identity
    // of an action's output" means when the producer is a real tool. Reading
    // them from a second store instance is the cross-instance claim of 0024.
    let store = match FilesystemContentStore::open(root.path()) {
        Ok(store) => store,
        Err(error) => unreachable!("the filesystem store failed to reopen: {error:?}"),
    };
    let stored = store
        .get_blob(object)
        .expect("the store reads back")
        .expect("the object file was imported");
    assert!(
        stored.as_bytes().starts_with(ELF_MAGIC),
        "the captured output is not an object file"
    );
}

/// Determinism is a separate claim from reuse (decision 0014), and reuse hides
/// it: a served result is identical for being the same bytes, and says nothing
/// about what the tool would produce a second time. Switching caching off makes
/// the compiler run twice, which is what the claim needs.
#[test]
fn the_same_compile_run_twice_produces_identical_objects() {
    let Some(compiler) = HostCompiler::discover() else {
        return;
    };
    let root = match tempfile::tempdir() {
        Ok(root) => root,
        Err(error) => unreachable!("could not create a content-store root: {error:?}"),
    };
    let (mut engine, request) = compile_engine(root.path(), &compiler);
    engine.set_action_caching(false);

    let first = compile(&mut engine, &request);
    let second = compile(&mut engine, &request);

    assert_eq!(
        action_computations(&engine),
        2,
        "caching is off, so both compiles should have run"
    );
    assert_eq!(
        first, second,
        "two runs of the same tool over the same input produced different objects"
    );
}

#[test]
fn the_same_compile_is_served_from_the_reusable_index() {
    let Some(compiler) = HostCompiler::discover() else {
        return;
    };
    let root = match tempfile::tempdir() {
        Ok(root) => root,
        Err(error) => unreachable!("could not create a content-store root: {error:?}"),
    };
    let (mut engine, request) = compile_engine(root.path(), &compiler);

    let first = compile(&mut engine, &request);
    let second = compile(&mut engine, &request);

    // Nothing about the request, the source, or the compiler changed, so the
    // second run plans the same contract and computes the same action key.
    assert_eq!(
        action_computations(&engine),
        1,
        "the second run compiled again instead of reusing the first attempt"
    );
    assert_eq!(
        first, second,
        "the reused result is not the object the first compile produced"
    );
}
