//! Real child processes overlapping under the engine's scheduler (decisions
//! 0022, 0029).
//!
//! The fixture-executor tests in `pith-engine` prove the scheduler holds
//! several actions in flight. They cannot prove the thing that has to be true
//! for it to matter: that two actual processes run at the same time, through
//! the real staging, fork/exec, and capture path.
//!
//! Overlap is not inferred from wall time here. Each action touches its own
//! marker in a shared directory and then waits for its sibling's, so the two
//! can only both finish if they were running together. An engine that ran them
//! one at a time deadlocks the first one, which gives up after ten seconds and
//! exits nonzero — a failed run with a diagnostic, not a hung test.

#![cfg(target_os = "linux")]

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

/// A runtime for one test. Built per call: constructing a thread pool is
/// cheap next to what these tests do, and it keeps each test independent.
fn runtime() -> TokioRuntime {
    match TokioRuntime::new() {
        Ok(runtime) => runtime,
        Err(error) => unreachable!("could not build a tokio runtime: {error:?}"),
    }
}

/// Touch this participant's marker, wait for its sibling's, then write the
/// result. The wait is bounded so a serialized engine fails the run instead of
/// hanging it; ten seconds is far longer than the rendezvous needs and far
/// shorter than a test-suite timeout.
const RENDEZVOUS_SCRIPT: &str = r#"
touch "$RENDEZVOUS/$ME"
attempts=0
while [ ! -e "$RENDEZVOUS/$PEER" ]; do
    attempts=$((attempts + 1))
    if [ "$attempts" -gt 200 ]; then
        echo "timed out waiting for $PEER: the actions did not overlap" >&2
        exit 3
    fi
    sleep 0.05
done
printf 'met:%s' "$ME" > result
"#;

/// One participant in the rendezvous. Each gets a distinct spec — different
/// `ME`/`PEER` — so the engine plans, materializes, and caches them separately.
struct RendezvousAction {
    executable: ContentId,
    rendezvous: String,
    me: String,
    peer: String,
}

impl ActionRule for RendezvousAction {
    fn plan(&self, _inputs: &[Value]) -> PithResult<ActionSpec> {
        let mut spec = ActionSpec::isolated(self.executable);
        spec.arguments = ["-c".into(), RENDEZVOUS_SCRIPT.into()].into();
        spec.environment = [
            // The child runs with `env_clear`, so the tools the script uses
            // have to be declared. Inheriting the host's PATH is what makes
            // this a test of scheduling rather than of nix path layout.
            EnvironmentVariable {
                name: "PATH".into(),
                value: host_path().into_boxed_str(),
            },
            EnvironmentVariable {
                name: "RENDEZVOUS".into(),
                value: self.rendezvous.clone().into_boxed_str(),
            },
            EnvironmentVariable {
                name: "ME".into(),
                value: self.me.clone().into_boxed_str(),
            },
            EnvironmentVariable {
                name: "PEER".into(),
                value: self.peer.clone().into_boxed_str(),
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
        Ok(PureStep::Complete(Value::Int(
            i64::try_from(values.len()).unwrap_or(i64::MAX),
        )))
    }
}

fn host_path() -> String {
    std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string())
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
    vec![Value::Int(0); arity].into_boxed_slice()
}

fn rule<K: pith_core::EffectCategory>(label: &str, interface: Interface) -> Rule<K> {
    let identity = RuleIdentity::of_module_declaration("local-executor-concurrency", label);
    let revision = RuleRevision::of_manifest(identity, b"v1");
    Rule::new(revision, label, interface, Span::none())
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
fn register_participants(
    engine: &mut Engine,
    rendezvous: &std::path::Path,
) -> Option<Vec<Request<Pure>>> {
    let shell = std::fs::read("/bin/sh").ok()?;
    let executable = engine.put_blob(&shell).ok()?;
    let names = ["first", "second"];
    let mut requests = Vec::new();
    for (arity, name) in names.iter().enumerate() {
        let peer = names.iter().find(|other| *other != name)?;
        let action_interface = interface(arity, Type::Blob);
        engine.register_action_rule(
            rule::<Action>(&format!("act-{name}"), action_interface.clone()),
            RendezvousAction {
                executable,
                rendezvous: rendezvous.to_string_lossy().into_owned(),
                me: (*name).to_string(),
                peer: (*peer).to_string(),
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

fn rendezvous_directory() -> Option<tempfile::TempDir> {
    tempfile::Builder::new()
        .prefix("pith-rendezvous-")
        .tempdir()
        .ok()
}

#[test]
fn two_real_child_processes_run_at_the_same_time() {
    let Some(rendezvous) = rendezvous_directory() else {
        eprintln!("skipping: could not create a rendezvous directory");
        return;
    };
    let mut engine = Engine::new();
    let Some(requests) = register_participants(&mut engine, rendezvous.path()) else {
        eprintln!("skipping: /bin/sh is not readable");
        return;
    };

    let evaluations = engine
        .run_many(
            &requests,
            &runtime(),
            &AllowAllActions,
            &LocalExecutor::new(),
        )
        .expect("the runtime drives the run")
        .expect("both actions meet at the rendezvous");

    assert_eq!(evaluations.len(), 2);
    // Both markers exist and both children wrote their result, which they only
    // reach after seeing each other.
    for name in ["first", "second"] {
        assert!(
            rendezvous.path().join(name).exists(),
            "{name} never reached the rendezvous"
        );
    }
    // The engine content-addresses what each child wrote, so the identity of
    // the imported output is the assertion: only a child that saw its sibling
    // gets as far as writing `met:<name>`.
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
    let Some(rendezvous) = rendezvous_directory() else {
        eprintln!("skipping: could not create a rendezvous directory");
        return;
    };
    let mut engine = Engine::new();
    let Some(requests) = register_participants(&mut engine, rendezvous.path()) else {
        eprintln!("skipping: /bin/sh is not readable");
        return;
    };
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
            &LocalExecutor::new(),
        )
        .expect("the runtime drives the run")
        .expect("the fan-out's actions meet at the rendezvous");

    assert_eq!(evaluation.value, Value::Int(2));
}
