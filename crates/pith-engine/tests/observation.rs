#![allow(
    clippy::expect_used,
    reason = "integration-test assertions unwrap the run outcome they are specifically exercising"
)]

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use pith_core::{
    EffectCategory, Interface, Observation, Pure, Request, Rule, RuleIdentity, RuleRevision, Type,
    Value,
};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_engine::{
    ActionInvocation, AllowAllActions, CapturedActionExecution, Engine, EvaluationSource,
    ExecutionPlatform, Executor, ExecutorIdentity, MemoryEngineStateStore, ObservationRule,
    Observed, Observer, ObserverIdentity, PureRule, PureRuleFrame, PureStep, Resumption,
    TokioRuntime,
};
use pith_store::MemoryContentStore;

#[derive(Clone)]
struct FileMtimeObserver {
    identity: Box<str>,
    attestations: Arc<AtomicUsize>,
    observations: Arc<AtomicUsize>,
}

impl FileMtimeObserver {
    fn new(identity: &str) -> Self {
        Self {
            identity: identity.into(),
            attestations: Arc::new(AtomicUsize::new(0)),
            observations: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn revision(subject: &Value) -> PithResult<Value> {
        let Value::Text(path) = subject else {
            return Err(test_error("file-mtime subject is not text"));
        };
        let modified = fs::metadata(Path::new(path.as_ref()))
            .and_then(|metadata| metadata.modified())
            .map_err(|error| test_error(&format!("file metadata is unavailable: {error}")))?;
        let duration = modified.duration_since(UNIX_EPOCH).map_err(|error| {
            test_error(&format!("file modification predates the epoch: {error}"))
        })?;
        Ok(Value::Text(
            format!("{}:{}", duration.as_secs(), duration.subsec_nanos()).into(),
        ))
    }
}

#[async_trait::async_trait]
impl Observer for FileMtimeObserver {
    fn identity(&self) -> ObserverIdentity {
        ObserverIdentity {
            observer: self.identity.clone(),
        }
    }

    async fn attest(&self, subject: &Value, _bound: &pith_engine::RunBound) -> PithResult<Value> {
        self.attestations.fetch_add(1, Ordering::SeqCst);
        Self::revision(subject)
    }

    async fn observe(
        &self,
        subject: &Value,
        _bound: &pith_engine::RunBound,
    ) -> PithResult<Observed> {
        self.observations.fetch_add(1, Ordering::SeqCst);
        let revision = Self::revision(subject)?;
        Ok(Observed {
            value: revision.clone(),
            revision,
        })
    }
}

struct PathSubject;

impl ObservationRule for PathSubject {
    fn subject(&self, inputs: &[Value]) -> PithResult<Value> {
        inputs
            .first()
            .cloned()
            .ok_or_else(|| test_error("file-mtime request has no path input"))
    }
}

struct ObserveRule {
    request: Request<Observation>,
}

impl PureRule for ObserveRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(ObserveFrame {
            request: self.request.clone(),
            requested: false,
        })
    }
}

struct ObserveFrame {
    request: Request<Observation>,
    requested: bool,
}

impl PureRuleFrame for ObserveFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::NeedObservation(self.request.clone()));
        }
        let value = input
            .and_then(Resumption::one)
            .ok_or_else(|| test_error("observation frame resumed without a value"))?;
        Ok(PureStep::Complete(value))
    }
}

struct NeverExecutor;

#[async_trait::async_trait]
impl Executor for NeverExecutor {
    fn identity(&self) -> ExecutorIdentity {
        ExecutorIdentity {
            executor: "observation-test".into(),
            platform: ExecutionPlatform {
                operating_system: "test".into(),
                architecture: "test".into(),
            },
        }
    }

    async fn execute(&self, _invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        Err(test_error(
            "observation test unexpectedly executed an action",
        ))
    }
}

struct Fixture {
    root: Request<Pure>,
    path: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "pith-m9-observation-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|error| unreachable!("current time predates epoch: {error}"))
                .as_nanos()
        ));
        fs::write(&path, b"first")
            .unwrap_or_else(|error| unreachable!("could not create observation fixture: {error}"));
        let root_interface = interface([], Type::Text);
        Self {
            root: Request::<Pure>::new("root", root_interface, [], Span::none()),
            path,
        }
    }

    fn configure(&self, state: MemoryEngineStateStore, observer: FileMtimeObserver) -> Engine {
        let mut engine = Engine::with_state_store(MemoryContentStore::default(), state);
        let observation_interface = interface([Type::Text], Type::Text);
        engine.register_observation_rule(
            rule::<Observation>("file-mtime", observation_interface),
            PathSubject,
        );
        engine.register_rule(
            rule::<Pure>("root", interface([], Type::Text)),
            ObserveRule {
                request: Request::<Observation>::new(
                    "file mtime",
                    interface([Type::Text], Type::Text),
                    [Value::Text(self.path.to_string_lossy().into_owned().into())],
                    Span::none(),
                ),
            },
        );
        engine.set_observer(observer);
        engine
    }

    fn change_mtime(&self) {
        let previous = fs::metadata(&self.path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or_else(|error| unreachable!("fixture mtime is unavailable: {error}"));
        for length in 6..100 {
            fs::write(&self.path, vec![b'x'; length]).unwrap_or_else(|error| {
                unreachable!("could not update observation fixture: {error}")
            });
            let current = fs::metadata(&self.path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or_else(|error| unreachable!("updated mtime is unavailable: {error}"));
            if current != previous {
                return;
            }
            std::thread::yield_now();
        }
        unreachable!("filesystem did not advance the fixture mtime");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn interface(inputs: impl Into<Box<[Type]>>, output: Type) -> Interface {
    Interface {
        inputs: inputs.into(),
        output,
    }
}

fn rule<E: EffectCategory>(label: &str, interface: Interface) -> Rule<E> {
    let identity = RuleIdentity::of_module_declaration("observation-tests", label);
    let revision = RuleRevision::of_manifest(identity, b"observation-tests-v1");
    Rule::new(revision, label, interface, Span::none())
}

fn runtime() -> TokioRuntime {
    TokioRuntime::new().unwrap_or_else(|error| unreachable!("could not create runtime: {error:?}"))
}

fn test_error(message: &str) -> DiagnosticSink {
    let mut diagnostics = DiagnosticSink::new();
    diagnostics.push(Diag::new(
        Severity::Error,
        StableCode(1999),
        Span::none(),
        message,
    ));
    diagnostics
}

#[test]
fn unchanged_file_attests_and_reuses_the_live_root() {
    let fixture = Fixture::new();
    let observer = FileMtimeObserver::new("file-mtime-v1");
    let mut engine = fixture.configure(MemoryEngineStateStore::default(), observer.clone());

    let first = engine
        .run(&fixture.root, &runtime(), &AllowAllActions, &NeverExecutor)
        .expect("runtime")
        .expect("first run");
    let second = engine
        .run(&fixture.root, &runtime(), &AllowAllActions, &NeverExecutor)
        .expect("runtime")
        .expect("second run");

    assert_eq!(first.source, EvaluationSource::Computed);
    assert_eq!(second.source, EvaluationSource::Reused);
    assert_eq!(observer.observations.load(Ordering::SeqCst), 1);
    assert_eq!(observer.attestations.load(Ordering::SeqCst), 1);
}

#[test]
fn unchanged_file_hydrates_across_engines_after_attestation() {
    let fixture = Fixture::new();
    let state = MemoryEngineStateStore::default();
    let first_observer = FileMtimeObserver::new("file-mtime-v1");
    let mut first = fixture.configure(state.clone(), first_observer.clone());
    first
        .run(&fixture.root, &runtime(), &AllowAllActions, &NeverExecutor)
        .expect("runtime")
        .expect("first run");

    let second_observer = FileMtimeObserver::new("file-mtime-v1");
    let mut second = fixture.configure(state, second_observer.clone());
    let evaluation = second
        .run(&fixture.root, &runtime(), &AllowAllActions, &NeverExecutor)
        .expect("runtime")
        .expect("second run");

    assert_eq!(evaluation.source, EvaluationSource::Hydrated);
    assert_eq!(second_observer.attestations.load(Ordering::SeqCst), 1);
    assert_eq!(second_observer.observations.load(Ordering::SeqCst), 0);
}

#[test]
fn changed_revision_recomputes_and_observes() {
    let fixture = Fixture::new();
    let observer = FileMtimeObserver::new("file-mtime-v1");
    let mut engine = fixture.configure(MemoryEngineStateStore::default(), observer.clone());
    engine
        .run(&fixture.root, &runtime(), &AllowAllActions, &NeverExecutor)
        .expect("runtime")
        .expect("first run");
    fixture.change_mtime();

    let evaluation = engine
        .run(&fixture.root, &runtime(), &AllowAllActions, &NeverExecutor)
        .expect("runtime")
        .expect("second run");

    assert_eq!(evaluation.source, EvaluationSource::Computed);
    assert_eq!(observer.observations.load(Ordering::SeqCst), 2);
}

#[test]
fn observer_identity_mismatch_recomputes() {
    let fixture = Fixture::new();
    let state = MemoryEngineStateStore::default();
    let mut first = fixture.configure(state.clone(), FileMtimeObserver::new("file-mtime-v1"));
    first
        .run(&fixture.root, &runtime(), &AllowAllActions, &NeverExecutor)
        .expect("runtime")
        .expect("first run");

    let observer = FileMtimeObserver::new("different-file-mtime");
    let mut second = fixture.configure(state, observer.clone());
    let evaluation = second
        .run(&fixture.root, &runtime(), &AllowAllActions, &NeverExecutor)
        .expect("runtime")
        .expect("second run");

    assert_eq!(evaluation.source, EvaluationSource::Computed);
    assert_eq!(observer.attestations.load(Ordering::SeqCst), 0);
    assert_eq!(observer.observations.load(Ordering::SeqCst), 1);
}

#[test]
fn observation_requires_a_run_and_an_observer() {
    let fixture = Fixture::new();
    let state = MemoryEngineStateStore::default();
    let observer = FileMtimeObserver::new("file-mtime-v1");
    let mut pure_engine = fixture.configure(state, observer);
    let pure_diagnostics = pure_engine
        .evaluate_pure(&fixture.root)
        .expect_err("pure evaluation must reject observation");
    assert_eq!(
        pure_diagnostics.iter().next().map(|diag| diag.code.0),
        Some(1206)
    );

    let mut run_engine = Engine::new();
    run_engine.register_observation_rule(
        rule::<Observation>("file-mtime", interface([Type::Text], Type::Text)),
        PathSubject,
    );
    run_engine.register_rule(
        rule::<Pure>("root", interface([], Type::Text)),
        ObserveRule {
            request: Request::<Observation>::new(
                "file mtime",
                interface([Type::Text], Type::Text),
                [Value::Text(
                    fixture.path.to_string_lossy().into_owned().into(),
                )],
                Span::none(),
            ),
        },
    );
    let diagnostics = run_engine
        .run(&fixture.root, &runtime(), &AllowAllActions, &NeverExecutor)
        .expect("runtime")
        .expect_err("run without observer must fail");
    assert_eq!(
        diagnostics.iter().next().map(|diag| diag.code.0),
        Some(1217)
    );
}
