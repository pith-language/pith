//! Real child processes overlapping under the engine's scheduler (decisions
//! 0022, 0029).
//!
//! The fixture-executor tests in `pith-engine` prove the scheduler holds
//! several actions in flight. They cannot prove the thing that has to be true
//! for it to matter: that two actual processes run at the same time, through
//! the real staging, fork/exec, and capture path.
//!
//! Overlap is not inferred from wall time here. Each action marks its arrival
//! in its own scratch root and waits to be released, and the test releases both
//! only once both markers exist. An engine that ran them one at a time never
//! releases the first, which gives up after ten seconds and exits nonzero — a
//! failed run with a diagnostic, not a hung test.
//!
//! The children cannot see each other: landlock confines each to its own scratch
//! root (decision 0030). The rendezvous is therefore observed and released from
//! outside, by the test.

#![cfg(target_os = "linux")]

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use pith_core::{
    Action, ActionOutput, ActionSpec, Content, EnvironmentVariable, Interface, OutputKind, Pure,
    Request, Rule, RuleIdentity, RuleRevision, Type, Value,
};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_engine::{
    ActionExecution, ActionRule, AllowAllActions, Engine, PureRule, PureRuleFrame, PureStep,
    Resumption, TokioRuntime,
};
use pith_executor_local::LocalExecutor;
use pith_ids::ContentId;

mod support;

/// A runtime for one test. Built per call: constructing a thread pool is
/// cheap next to what these tests do, and it keeps each test independent.
fn runtime() -> TokioRuntime {
    match TokioRuntime::new() {
        Ok(runtime) => runtime,
        Err(error) => unreachable!("could not build a tokio runtime: {error:?}"),
    }
}

/// Mark arrival, wait to be released, write the result. The wait is bounded so a
/// serialized engine fails the run instead of hanging it; ten seconds is far
/// longer than the rendezvous needs and far shorter than a test-suite timeout.
const RENDEZVOUS_SCRIPT: &str = r#"
: > arrived
attempts=0
while [ ! -e released ]; do
    attempts=$((attempts + 1))
    if [ "$attempts" -gt 200 ]; then
        echo "timed out waiting for release: the actions did not overlap" >&2
        exit 3
    fi
    "$SLEEP" 0.05
done
printf 'met:%s' "$ME" > result
"#;

/// The marker a child writes on arrival, and the one the test writes to release
/// it. Both live in the child's own scratch root.
const ARRIVED: &str = "arrived";
const RELEASED: &str = "released";

const POLL_INTERVAL: Duration = Duration::from_millis(20);
const RENDEZVOUS_TIMEOUT: Duration = Duration::from_secs(10);

/// One participant in the rendezvous. Each gets a distinct spec — a different
/// `ME` — so the engine plans, materializes, and caches them separately.
struct RendezvousAction {
    executable: &'static str,
    sleep: Box<str>,
    me: String,
}

impl ActionRule for RendezvousAction {
    fn plan(&self, _inputs: &[Value]) -> PithResult<ActionSpec> {
        let mut spec = ActionSpec::isolated(self.executable);
        spec.toolchain = support::closure_for(&[self.executable, "sleep"]);
        spec.arguments = ["-c".into(), RENDEZVOUS_SCRIPT.into()].into();
        spec.environment = [
            // The script calls `sleep` by absolute path, so the child needs no
            // PATH at all.
            EnvironmentVariable {
                name: "SLEEP".into(),
                value: self.sleep.clone(),
            },
            EnvironmentVariable {
                name: "ME".into(),
                value: self.me.clone().into_boxed_str(),
            },
        ]
        .into();
        spec.outputs = [ActionOutput {
            path: "result".into(),
            kind: OutputKind::Blob,
        }]
        .into();
        Ok(spec)
    }

    fn complete(&self, _inputs: &[Value], execution: &ActionExecution) -> PithResult<Value> {
        match execution
            .report
            .outputs
            .first()
            .map(|output| &output.content)
        {
            Some(Content::Blob(identity)) => Ok(Value::Blob(*identity)),
            _ => Err(fixture_error("the rendezvous action produced no blob")),
        }
    }
}

/// Runs one action and returns its result.
struct RunAction {
    request: Request<Action>,
}

impl PureRule for RunAction {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(RunActionFrame {
            request: self.request.clone(),
            requested: false,
        })
    }
}

struct RunActionFrame {
    request: Request<Action>,
    requested: bool,
}

impl PureRuleFrame for RunActionFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::NeedAction(self.request.clone()));
        }
        input
            .and_then(Resumption::one)
            .map(PureStep::Complete)
            .ok_or_else(|| fixture_error("the action resumed without a value"))
    }
}

/// Requests both participants at once, declaring them independent.
struct BothAtOnce {
    dependencies: Box<[Request<Pure>]>,
}

impl PureRule for BothAtOnce {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(BothAtOnceFrame {
            dependencies: self.dependencies.clone(),
            requested: false,
        })
    }
}

struct BothAtOnceFrame {
    dependencies: Box<[Request<Pure>]>,
    requested: bool,
}

impl PureRuleFrame for BothAtOnceFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::NeedAll(self.dependencies.clone()));
        }
        let Some(Resumption::Many(values)) = input else {
            return Err(fixture_error("a NeedAll batch must resume with Many"));
        };
        Ok(PureStep::Complete(Value::int(
            i64::try_from(values.len()).unwrap_or(i64::MAX),
        )))
    }
}

/// Watches the executors' scratch base for every participant to arrive, then
/// releases them all at once. Nothing is released until the last arrives, so a
/// serialized engine never gets past its first child.
struct Rendezvous {
    watcher: thread::JoinHandle<usize>,
}

impl Rendezvous {
    fn watching(base: PathBuf, participants: usize) -> Self {
        let watcher = thread::spawn(move || {
            let started = Instant::now();
            loop {
                let arrived = arrived_roots(&base);
                if arrived.len() >= participants {
                    for root in &arrived {
                        drop(std::fs::File::create(root.join(RELEASED)));
                    }
                    return arrived.len();
                }
                if started.elapsed() >= RENDEZVOUS_TIMEOUT {
                    return arrived.len();
                }
                thread::sleep(POLL_INTERVAL);
            }
        });
        Self { watcher }
    }

    /// How many participants were seen together. Anything short of the expected
    /// count means they did not overlap.
    fn released(self) -> usize {
        self.watcher.join().unwrap_or(0)
    }
}

/// The directories beneath `base` holding an arrival marker. Searched rather
/// than derived, because where a child runs inside its scratch root is the
/// executor's business and not something this test should restate.
fn arrived_roots(base: &Path) -> Vec<PathBuf> {
    const MAX_DEPTH: usize = 3;
    let mut found = Vec::new();
    let mut frontier = vec![(base.to_path_buf(), 0usize)];
    while let Some((directory, depth)) = frontier.pop() {
        if directory.join(ARRIVED).is_file() {
            found.push(directory);
            continue;
        }
        if depth >= MAX_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            if entry.path().is_dir() {
                frontier.push((entry.path(), depth.saturating_add(1)));
            }
        }
    }
    found
}

fn scratch_base() -> Option<tempfile::TempDir> {
    tempfile::Builder::new()
        .prefix("pith-scratch-")
        .tempdir()
        .ok()
}

/// Rules are selected by interface, so the two participants are told apart by
/// arity rather than by label.
fn interface(arity: usize, output: Type) -> Interface {
    Interface {
        inputs: vec![Type::Int; arity].into_boxed_slice(),
        output,
    }
}

fn inputs(arity: usize) -> Box<[Value]> {
    vec![Value::int(0); arity].into_boxed_slice()
}

fn rule<K: pith_core::EffectCategory>(label: &str, interface: Interface) -> Rule<K> {
    let identity = RuleIdentity::of_module_declaration("local-executor-concurrency", label);
    let revision = RuleRevision::of_manifest(identity, b"v1");
    Rule::new(
        "local-executor-concurrency",
        revision,
        label,
        interface,
        Span::none(),
    )
}

fn request<K: pith_core::EffectCategory>(
    label: &str,
    interface: Interface,
    inputs: Box<[Value]>,
) -> Request<K> {
    Request::new(label, interface, inputs, Span::none())
}

fn fixture_error(message: &str) -> DiagnosticSink {
    let mut sink = DiagnosticSink::new();
    sink.push(Diag::new(
        Severity::Error,
        StableCode(1211),
        Span::none(),
        message,
    ));
    sink
}

/// Register two rendezvous participants and return the pure requests that run
/// them. Participant `i` has arity `i`, because rules are selected by interface.
fn register_participants(engine: &mut Engine) -> Option<Vec<Request<Pure>>> {
    if std::fs::read("/bin/sh").is_err() {
        return None;
    }
    let sleep: Box<str> = support::program_path("sleep").into_boxed_str();
    let names = ["first", "second"];
    let mut requests = Vec::new();
    for (arity, name) in names.iter().enumerate() {
        let action_interface = interface(arity, Type::Blob);
        engine.register_action_rule(
            rule::<Action>(&format!("act-{name}"), action_interface.clone()),
            RendezvousAction {
                executable: "/bin/sh",
                sleep: sleep.clone(),
                me: (*name).to_string(),
            },
        );
        engine.register_rule(
            rule::<Pure>(&format!("run-{name}"), action_interface.clone()),
            RunAction {
                request: request::<Action>(
                    &format!("act-{name}"),
                    action_interface.clone(),
                    inputs(arity),
                ),
            },
        );
        requests.push(request::<Pure>(
            &format!("run-{name}"),
            action_interface,
            inputs(arity),
        ));
    }
    Some(requests)
}

/// An engine that keeps two actions in flight, whatever the host reports.
///
/// The default width is the host's available parallelism, and a CI runner
/// whose CPU quota reports one available core would serialize the rendezvous
/// these tests exist to observe. The overlap under test is the scheduler's
/// willingness, not the host's core count, so the width is declared the way
/// [`Engine::set_action_concurrency`] exists for.
fn overlap_engine() -> Engine {
    let mut engine = Engine::new();
    engine.set_action_concurrency(NonZeroUsize::new(2).unwrap_or(NonZeroUsize::MIN));
    engine
}

#[test]
fn two_real_child_processes_run_at_the_same_time() {
    let Some(base) = scratch_base() else {
        eprintln!("skipping: could not create a scratch base");
        return;
    };
    let mut engine = overlap_engine();
    let Some(requests) = register_participants(&mut engine) else {
        eprintln!("skipping: /bin/sh is not readable on this host");
        return;
    };
    let rendezvous = Rendezvous::watching(base.path().to_path_buf(), 2);

    let evaluations = engine
        .run_many(
            &requests,
            &runtime(),
            &AllowAllActions,
            &LocalExecutor::with_scratch_base(base.path().to_path_buf()),
        )
        .expect("the runtime drives the run")
        .expect("both actions meet at the rendezvous");

    assert_eq!(evaluations.len(), 2);
    assert_eq!(
        rendezvous.released(),
        2,
        "the two actions were never in flight together"
    );
    // The engine content-addresses what each child wrote, so the identity of
    // the imported output is the assertion: only a released child gets as far
    // as writing `met:<name>`.
    let identities: Vec<Value> = evaluations
        .iter()
        .map(|evaluation| evaluation.value.clone())
        .collect();
    assert_eq!(
        identities,
        [
            Value::Blob(ContentId::of_blob(b"met:first")),
            Value::Blob(ContentId::of_blob(b"met:second")),
        ]
    );
}

#[test]
fn a_fan_out_overlaps_real_child_processes() {
    let Some(base) = scratch_base() else {
        eprintln!("skipping: could not create a scratch base");
        return;
    };
    let mut engine = overlap_engine();
    let Some(requests) = register_participants(&mut engine) else {
        eprintln!("skipping: /bin/sh is not readable on this host");
        return;
    };
    let rendezvous = Rendezvous::watching(base.path().to_path_buf(), 2);
    // Arity 2 keeps the root distinct from the arity-0 and arity-1 participants.
    let root_interface = interface(2, Type::Int);
    engine.register_rule(
        rule::<Pure>("both", root_interface.clone()),
        BothAtOnce {
            dependencies: requests.into_boxed_slice(),
        },
    );

    let evaluation = engine
        .run(
            &request::<Pure>("both", root_interface, inputs(2)),
            &runtime(),
            &AllowAllActions,
            &LocalExecutor::with_scratch_base(base.path().to_path_buf()),
        )
        .expect("the runtime drives the run")
        .expect("the fan-out's actions meet at the rendezvous");

    assert_eq!(evaluation.value, Value::int(2));
    assert_eq!(
        rendezvous.released(),
        2,
        "the fan-out's actions were never in flight together"
    );
}
