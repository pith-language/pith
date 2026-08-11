//! Tests that independent work actually overlaps (decision 0022).
//!
//! Concurrency needs something to be concurrent about, and the step machine
//! only ever yields one request at a time. Two constructs create independent
//! work: `PureStep::NeedAll`, which declares that a batch of requests do not
//! depend on one another, and `Engine::run_many`, which drives several roots.
//! These tests cover both, and the overlap assertions measure it rather than
//! inferring it from wall time: the executor holds each action at a barrier
//! that only releases once every participant has arrived, so an engine that ran
//! its actions one at a time could not get past it.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use pith_core::{
    Action, ActionSpec, Interface, Pure, Request, Rule, RuleIdentity, RuleRevision, Type, Value,
};
use pith_diag::{Diag, DiagnosticSink, EngineCode, PithResult, Severity, Span, StableCode};
use pith_engine::state::DurableAttemptState;
use pith_engine::{
    AccessVerification, ActionExecution, ActionInvocation, ActionRule, AllowAllActions,
    AttemptState, CapturedActionExecution, CapturedExecutionReport, ComputationKind, Engine,
    Evaluation, ExecutionPlatform, Executor, PureRule, PureRuleFrame, PureStep, Resumption,
    TokioRuntime,
};
use pith_store::MemoryContentStore;
use tokio::sync::Barrier;

/// A runtime for one test. Built per call: constructing a thread pool is
/// cheap next to what these tests do, and it keeps each test independent.
fn runtime() -> TokioRuntime {
    match TokioRuntime::new() {
        Ok(runtime) => runtime,
        Err(error) => unreachable!("could not build a tokio runtime: {error:?}"),
    }
}

/// How long an action waits at the barrier before giving up. Only a regression
/// reaches it: with real overlap every participant arrives immediately. The
/// timeout exists so a serialized engine fails the test instead of hanging it.
const BARRIER_TIMEOUT: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// Pure rules

/// Completes immediately with a constant. Used where a test needs distinct
/// values it can check the order of.
struct ConstantRule(Value);

impl PureRule for ConstantRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(ConstantFrame(self.0.clone()))
    }
}

struct ConstantFrame(Value);

impl PureRuleFrame for ConstantFrame {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        Ok(PureStep::Complete(self.0.clone()))
    }
}

/// Requests a batch of pure results at once, then sums them. A sum says
/// nothing about the order they arrived in, which is the point here — these
/// tests are about whether the batch ran concurrently and completed. Order is
/// checked separately, by [`OrderedAllRule`].
struct SumAllRule {
    dependencies: Box<[Request<Pure>]>,
}

impl PureRule for SumAllRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(SumAllFrame {
            dependencies: self.dependencies.clone(),
            requested: false,
        })
    }
}

struct SumAllFrame {
    dependencies: Box<[Request<Pure>]>,
    requested: bool,
}

impl PureRuleFrame for SumAllFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::NeedAll(self.dependencies.clone()));
        }
        let Some(Resumption::Many(values)) = input else {
            return Err(fixture_error("a NeedAll batch must resume with Many"));
        };
        let mut total = 0_i64;
        for value in values {
            match value {
                Value::Int(number) => total = total.saturating_add(number),
                other => return Err(fixture_error(&format!("expected an int, got {other:?}"))),
            }
        }
        Ok(PureStep::Complete(Value::Int(total)))
    }
}

/// Requests a batch and reports the results in the order they arrived, so a
/// test can tell request order from completion order.
struct OrderedAllRule {
    dependencies: Box<[Request<Pure>]>,
}

impl PureRule for OrderedAllRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(OrderedAllFrame {
            dependencies: self.dependencies.clone(),
            requested: false,
        })
    }
}

struct OrderedAllFrame {
    dependencies: Box<[Request<Pure>]>,
    requested: bool,
}

impl PureRuleFrame for OrderedAllFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::NeedAll(self.dependencies.clone()));
        }
        let Some(Resumption::Many(values)) = input else {
            return Err(fixture_error("a NeedAll batch must resume with Many"));
        };
        let rendered: Vec<String> = values
            .iter()
            .map(|value| match value {
                Value::Int(number) => number.to_string(),
                other => format!("{other:?}"),
            })
            .collect();
        Ok(PureStep::Complete(Value::Text(
            rendered.join(",").into_boxed_str(),
        )))
    }
}

/// Runs one action and returns its result. The unit of independent work in the
/// overlap tests.
struct ActionDepRule {
    dependency: Request<Action>,
}

impl PureRule for ActionDepRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(ActionDepFrame {
            dependency: self.dependency.clone(),
            requested: false,
        })
    }
}

struct ActionDepFrame {
    dependency: Request<Action>,
    requested: bool,
}

impl PureRuleFrame for ActionDepFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::NeedAction(self.dependency.clone()));
        }
        match input.and_then(Resumption::one) {
            Some(value) => Ok(PureStep::Complete(value)),
            None => Err(fixture_error("the action completed without a value")),
        }
    }
}

// ---------------------------------------------------------------------------
// Action rule and executors

/// Plans a contract that declares nothing but its executable and returns twice
/// its input count. Rules are selected by interface, so arity is what tells the
/// fixtures apart; making the result a function of arity keeps each leaf's
/// expected value readable. The action is deliberately trivial: these tests are
/// about scheduling, not about what an action computes.
struct ArityAction;

impl ActionRule for ArityAction {
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec> {
        let mut spec = ActionSpec::isolated(fixture_executable());
        // The only thing distinguishing one fixture action from another, which
        // is what `FailOneExecutor` selects on.
        spec.arguments = [arity(inputs).to_string().into_boxed_str()].into();
        Ok(spec)
    }

    fn complete(&self, inputs: &[Value], _execution: &ActionExecution) -> PithResult<Value> {
        Ok(Value::Int(arity(inputs).saturating_mul(2)))
    }
}

fn arity(inputs: &[Value]) -> i64 {
    i64::try_from(inputs.len()).unwrap_or(0)
}

/// Holds every action at a shared barrier, so the run can only finish if the
/// engine had them all in flight at once. `peak` records how many were running
/// together, which is the direct measurement of overlap.
struct BarrierExecutor {
    barrier: Arc<Barrier>,
    live: AtomicUsize,
    peak: AtomicUsize,
}

impl BarrierExecutor {
    fn expecting(actions: usize) -> Self {
        Self {
            barrier: Arc::new(Barrier::new(actions)),
            live: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        }
    }

    fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }
}

/// Let `engine` run `actions` at once, and return a barrier of the same width.
///
/// The two numbers have to agree. The engine's default bound is the host's core
/// count, so without this a barrier wider than the host's cores would never
/// release and the test would measure the machine rather than the engine.
fn barrier_across(engine: &mut Engine, actions: usize) -> BarrierExecutor {
    allow_actions(engine, actions);
    BarrierExecutor::expecting(actions)
}

fn allow_actions(engine: &mut Engine, actions: usize) {
    engine.set_action_concurrency(NonZeroUsize::new(actions).unwrap_or(NonZeroUsize::MIN));
}

#[async_trait::async_trait]
impl Executor for BarrierExecutor {
    async fn execute(&self, _invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        let live = self.live.fetch_add(1, Ordering::SeqCst).saturating_add(1);
        self.peak.fetch_max(live, Ordering::SeqCst);
        let waited = tokio::time::timeout(BARRIER_TIMEOUT, self.barrier.wait()).await;
        self.live.fetch_sub(1, Ordering::SeqCst);
        if waited.is_err() {
            return Err(fixture_error(
                "timed out waiting for a sibling action; the engine ran them one at a time",
            ));
        }
        Ok(empty_execution())
    }
}

/// Fails the action whose request carries `failing`, and holds every other
/// action open until the failure has had a chance to abort the run.
struct FailOneExecutor {
    failing: i64,
}

#[async_trait::async_trait]
impl Executor for FailOneExecutor {
    async fn execute(&self, invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        // The spec declares nothing else distinguishing, so the argument list
        // is what tells the two actions apart.
        let failing = invocation
            .spec
            .arguments
            .first()
            .is_some_and(|argument| argument.as_ref() == self.failing.to_string());
        if failing {
            return Err(fixture_error("this action fails"));
        }
        tokio::task::yield_now().await;
        Ok(empty_execution())
    }
}

fn empty_execution() -> CapturedActionExecution {
    CapturedActionExecution {
        report: CapturedExecutionReport {
            executor: "fixture".into(),
            platform: ExecutionPlatform {
                operating_system: "fixture".into(),
                architecture: "fixture".into(),
            },
            access: AccessVerification::Prevented,
            outputs: Box::new([]),
            capabilities_used: Box::new([]),
        },
    }
}

// ---------------------------------------------------------------------------
// Fixtures

fn fixture_executable() -> &'static str {
    "/bin/fixture-action"
}

fn fixture_error(message: &str) -> DiagnosticSink {
    let mut diagnostics = DiagnosticSink::new();
    diagnostics.push(Diag::new(
        Severity::Error,
        StableCode(1211),
        Span::none(),
        message,
    ));
    diagnostics
}

fn fixture_engine() -> Engine {
    // The executable is a host path (decision 0030) and the concurrency fixture
    // action declares no inputs, so the store starts empty.
    Engine::with_content_store(MemoryContentStore::default())
}

fn interface(inputs: &[Type], output: Type) -> Interface {
    Interface {
        inputs: inputs.to_vec().into_boxed_slice(),
        output,
    }
}

fn pure_rule(label: &str, interface: Interface) -> Rule<Pure> {
    let identity = RuleIdentity::of_module_declaration("concurrency-tests", label);
    let revision = RuleRevision::of_manifest(identity, b"concurrency-tests-v1");
    Rule::<Pure>::new(revision, label, interface, Span::none())
}

fn pure_request(
    label: &str,
    interface: Interface,
    inputs: impl Into<Box<[Value]>>,
) -> Request<Pure> {
    Request::<Pure>::new(label, interface, inputs, Span::none())
}

fn action_rule(label: &str, interface: Interface) -> Rule<Action> {
    let identity = RuleIdentity::of_module_declaration("concurrency-tests", label);
    let revision = RuleRevision::of_manifest(identity, b"concurrency-tests-v1");
    Rule::<Action>::new(revision, label, interface, Span::none())
}

fn action_request(
    label: &str,
    interface: Interface,
    inputs: impl Into<Box<[Value]>>,
) -> Request<Action> {
    Request::<Action>::new(label, interface, inputs, Span::none())
}

/// An interface taking `arity` ints and returning one. Rules are selected by
/// interface alone, so every fixture rule that coexists needs a distinct arity.
fn int_interface(arity: usize) -> Interface {
    interface(&vec![Type::Int; arity], Type::Int)
}

fn int_inputs(arity: usize) -> Box<[Value]> {
    vec![Value::Int(0); arity].into_boxed_slice()
}

/// Register `count` action-backed pure rules, each running one action, and
/// return their requests. Leaf `i` has arity `i` and evaluates to `2 * i`.
fn register_action_leaves(engine: &mut Engine, count: usize) -> Vec<Request<Pure>> {
    let mut requests = Vec::new();
    for index in 0..count {
        let leaf_interface = int_interface(index);
        engine.register_action_rule(
            action_rule(&format!("action-{index}"), leaf_interface.clone()),
            ArityAction,
        );
        engine.register_rule(
            pure_rule(&format!("leaf-{index}"), leaf_interface.clone()),
            ActionDepRule {
                dependency: action_request(
                    &format!("action-{index}"),
                    leaf_interface.clone(),
                    int_inputs(index),
                ),
            },
        );
        requests.push(pure_request(
            &format!("leaf-{index}"),
            leaf_interface,
            int_inputs(index),
        ));
    }
    requests
}

fn values(evaluations: &[Evaluation]) -> Vec<Value> {
    evaluations
        .iter()
        .map(|evaluation| evaluation.value.clone())
        .collect()
}

fn assert_no_pending_attempts(engine: &Engine) {
    assert!(
        engine
            .query()
            .computations()
            .all(|(_, node)| !matches!(node.state, AttemptState::Pending)),
        "the run left a computation Pending"
    );
}

// ---------------------------------------------------------------------------
// Tests

#[test]
fn independent_roots_run_their_actions_concurrently() {
    let mut engine = fixture_engine();
    let roots = register_action_leaves(&mut engine, 2);
    let executor = barrier_across(&mut engine, 2);

    let evaluations = engine
        .run_many(&roots, &runtime(), &AllowAllActions, &executor)
        .expect("the runtime drives the run")
        .expect("both roots evaluate");

    assert_eq!(values(&evaluations), [Value::Int(0), Value::Int(2)]);
    assert_eq!(
        executor.peak(),
        2,
        "the two roots' actions should have been in flight together"
    );
    assert_no_pending_attempts(&engine);
}

#[test]
fn a_fan_out_runs_its_actions_concurrently() {
    let mut engine = fixture_engine();
    let leaves = register_action_leaves(&mut engine, 3);
    // Arity 3 keeps the root distinct from the arity-0/1/2 leaves.
    let root_interface = int_interface(3);
    engine.register_rule(
        pure_rule("root", root_interface.clone()),
        SumAllRule {
            dependencies: leaves.into_boxed_slice(),
        },
    );
    let executor = barrier_across(&mut engine, 3);

    let evaluation = engine
        .run(
            &pure_request("root", root_interface, int_inputs(3)),
            &runtime(),
            &AllowAllActions,
            &executor,
        )
        .expect("the runtime drives the run")
        .expect("the root evaluates");

    // 2*0 + 2*1 + 2*2
    assert_eq!(evaluation.value, Value::Int(6));
    assert_eq!(
        executor.peak(),
        3,
        "all three fan-out actions should have been in flight together"
    );
    assert_no_pending_attempts(&engine);
}

/// The width of the fan-out the bounded tests build, and the bound they hold it
/// to. Six over two is three batches: enough that a driver which started a batch
/// and then stopped refilling would not finish.
const BOUNDED_FAN_OUT: usize = 6;
const ACTION_BOUND: usize = 2;

/// Build a root that fans out over `BOUNDED_FAN_OUT` action-backed leaves.
fn wide_fan_out_root(engine: &mut Engine) -> Request<Pure> {
    let leaves = register_action_leaves(engine, BOUNDED_FAN_OUT);
    let root_interface = int_interface(BOUNDED_FAN_OUT);
    engine.register_rule(
        pure_rule("root", root_interface.clone()),
        SumAllRule {
            dependencies: leaves.into_boxed_slice(),
        },
    );
    pure_request("root", root_interface, int_inputs(BOUNDED_FAN_OUT))
}

fn action_computations(engine: &Engine) -> usize {
    engine
        .query()
        .computations()
        .filter(|(_, node)| matches!(node.kind, ComputationKind::Action(_)))
        .count()
}

#[test]
fn a_fan_out_wider_than_the_bound_runs_its_actions_in_batches() {
    let mut engine = fixture_engine();
    let root = wide_fan_out_root(&mut engine);
    // The barrier is as wide as the bound, so it releases once the driver has
    // filled the pipe and not before. A driver that started the whole fan-out
    // would show a peak of six; one that started a batch and never refilled
    // would never reach the third generation and would time out.
    let executor = barrier_across(&mut engine, ACTION_BOUND);

    let evaluation = engine
        .run(&root, &runtime(), &AllowAllActions, &executor)
        .expect("the runtime drives the run")
        .expect("the root evaluates");

    // 2 * (0 + 1 + 2 + 3 + 4 + 5)
    assert_eq!(evaluation.value, Value::Int(30));
    assert_eq!(
        executor.peak(),
        ACTION_BOUND,
        "the bound should have held the fan-out to {ACTION_BOUND} actions at a time"
    );
    assert_no_pending_attempts(&engine);
}

#[test]
fn actions_waiting_for_a_slot_are_not_planned() {
    let mut engine = fixture_engine();
    let root = wide_fan_out_root(&mut engine);
    allow_actions(&mut engine, ACTION_BOUND);
    // Leaf 0's action plans with the argument "0", so the failure lands in the
    // first batch and the run aborts while the rest are still queued.
    let executor = FailOneExecutor { failing: 0 };

    let result = engine
        .run(&root, &runtime(), &AllowAllActions, &executor)
        .expect("the runtime drives the run");

    assert!(result.is_err(), "the failing action should abort the run");
    // The cost the bound exists to control is the materialized invocation, not
    // the chain. A queued action has no computation node: it was never planned,
    // so its inputs were never read out of the content store.
    assert_eq!(
        action_computations(&engine),
        ACTION_BOUND,
        "only the actions that had a slot should have been planned"
    );
    assert_no_pending_attempts(&engine);
}

#[test]
fn a_fan_out_resumes_in_request_order() {
    let mut engine = fixture_engine();
    // Register the leaves in the reverse of the order they are requested, so a
    // scheduler that resumed in registration or completion order would show it.
    for (arity, value) in [(1_usize, 20_i64), (0, 10)] {
        engine.register_rule(
            pure_rule(&format!("leaf-{arity}"), int_interface(arity)),
            ConstantRule(Value::Int(value)),
        );
    }
    let root_interface = interface(&[], Type::Text);
    engine.register_rule(
        pure_rule("root", root_interface.clone()),
        OrderedAllRule {
            dependencies: [
                pure_request("leaf-0", int_interface(0), int_inputs(0)),
                pure_request("leaf-1", int_interface(1), int_inputs(1)),
            ]
            .into(),
        },
    );

    let evaluation = engine
        .evaluate_pure(&pure_request("root", root_interface, []))
        .expect("the root evaluates");

    assert_eq!(evaluation.value, Value::Text("10,20".into()));
}

#[test]
fn an_empty_fan_out_resumes_with_no_values() {
    let mut engine = fixture_engine();
    let root_interface = interface(&[], Type::Int);
    engine.register_rule(
        pure_rule("root", root_interface.clone()),
        SumAllRule {
            dependencies: Box::new([]),
        },
    );

    let evaluation = engine
        .evaluate_pure(&pure_request("root", root_interface, []))
        .expect("the root evaluates");

    assert_eq!(evaluation.value, Value::Int(0));
}

#[test]
fn a_fan_out_that_requests_its_requester_is_a_cycle() {
    let mut engine = fixture_engine();
    let root_interface = interface(&[], Type::Int);
    engine.register_rule(
        pure_rule("root", root_interface.clone()),
        SumAllRule {
            dependencies: [pure_request("root", root_interface.clone(), [])].into(),
        },
    );

    let diagnostics = engine
        .evaluate_pure(&pure_request("root", root_interface, []))
        .expect_err("a self-request is a cycle");

    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        codes.contains(&pith_diag::StableCode::from(EngineCode::DependencyCycle)),
        "expected a dependency-cycle diagnostic, got {codes:?}"
    );
    assert_no_pending_attempts(&engine);
}

#[test]
fn a_cycle_is_detected_through_the_frame_that_opened_the_fan_out() {
    let mut engine = fixture_engine();
    // root fans out to `middle`, which requests `root` again. The repeat is in
    // an ancestor chain, not in `middle`'s own stack, so only a cycle check that
    // walks past the fan-out boundary catches it.
    engine.register_rule(
        pure_rule("root", int_interface(0)),
        SumAllRule {
            dependencies: [pure_request("middle", int_interface(1), int_inputs(1))].into(),
        },
    );
    engine.register_rule(
        pure_rule("middle", int_interface(1)),
        SumAllRule {
            dependencies: [pure_request("root", int_interface(0), int_inputs(0))].into(),
        },
    );

    let diagnostics = engine
        .evaluate_pure(&pure_request("root", int_interface(0), int_inputs(0)))
        .expect_err("the cycle crosses the fan-out boundary");

    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        codes.contains(&pith_diag::StableCode::from(EngineCode::DependencyCycle)),
        "expected a dependency-cycle diagnostic, got {codes:?}"
    );
    assert_no_pending_attempts(&engine);
}

#[test]
fn a_failing_action_leaves_no_concurrent_sibling_pending() {
    let mut engine = fixture_engine();
    let roots = register_action_leaves(&mut engine, 2);
    allow_actions(&mut engine, 2);
    // Leaf 1's action plans with the argument "1"; the executor fails that one
    // and yields on the other, so the sibling is in flight when the run aborts.
    let executor = FailOneExecutor { failing: 1 };

    let result = engine
        .run_many(&roots, &runtime(), &AllowAllActions, &executor)
        .expect("the runtime drives the run");

    assert!(result.is_err(), "the failing action should abort the run");
    // The sibling was in flight when the run aborted. Its arena node must not
    // be left waiting on an executor result nobody will read.
    assert_no_pending_attempts(&engine);

    let states = attempt_states(&engine);
    // The action that failed is a failure; the one that was still running was
    // stopped. Recording both as failures would blame work that never got a
    // verdict.
    assert!(
        states
            .iter()
            .any(|state| matches!(state, AttemptState::Failed { .. })),
        "the failing action should be recorded as failed: {states:?}"
    );
    assert!(
        states
            .iter()
            .any(|state| matches!(state, AttemptState::Cancelled { .. })),
        "the in-flight sibling should be recorded as cancelled: {states:?}"
    );
}

// ---------------------------------------------------------------------------
// Cancellation

/// Cancels the run from inside the executor: the first action to arrive sets
/// the flag, so cancellation lands while its siblings are still in flight.
/// Every action then waits at the barrier, which is what guarantees the
/// siblings really are in flight rather than merely unstarted.
struct CancellingExecutor {
    cancel: Arc<AtomicBool>,
    barrier: Arc<Barrier>,
}

#[async_trait::async_trait]
impl Executor for CancellingExecutor {
    async fn execute(&self, _invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        self.cancel.store(true, Ordering::SeqCst);
        if tokio::time::timeout(BARRIER_TIMEOUT, self.barrier.wait())
            .await
            .is_err()
        {
            return Err(fixture_error("timed out waiting for a sibling action"));
        }
        Ok(empty_execution())
    }
}

fn attempt_states(engine: &Engine) -> Vec<AttemptState> {
    engine
        .query()
        .computations()
        .map(|(_, node)| node.state.clone())
        .collect()
}

#[test]
fn a_cancelled_run_reports_the_cancellation() {
    let mut engine = fixture_engine();
    let roots = register_action_leaves(&mut engine, 2);
    allow_actions(&mut engine, 2);
    let cancel = Arc::new(AtomicBool::new(false));
    let executor = CancellingExecutor {
        cancel: Arc::clone(&cancel),
        barrier: Arc::new(Barrier::new(2)),
    };

    let diagnostics = engine
        .run_many_cancellable(
            &roots,
            &runtime(),
            &AllowAllActions,
            &executor,
            cancel.as_ref(),
        )
        .expect("the runtime drives the run")
        .expect_err("a cancelled run does not produce results");

    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        codes.contains(&pith_diag::StableCode::from(EngineCode::RunCancelled)),
        "expected a run-cancelled diagnostic, got {codes:?}"
    );
}

#[test]
fn cancelled_work_is_recorded_as_cancelled_rather_than_failed() {
    let mut engine = fixture_engine();
    let roots = register_action_leaves(&mut engine, 2);
    allow_actions(&mut engine, 2);
    let cancel = Arc::new(AtomicBool::new(false));
    let executor = CancellingExecutor {
        cancel: Arc::clone(&cancel),
        barrier: Arc::new(Barrier::new(2)),
    };

    let _ = engine
        .run_many_cancellable(
            &roots,
            &runtime(),
            &AllowAllActions,
            &executor,
            cancel.as_ref(),
        )
        .expect("the runtime drives the run");

    let states = attempt_states(&engine);
    assert!(
        !states.is_empty(),
        "the run should have allocated computations"
    );
    assert_no_pending_attempts(&engine);
    // Nothing was wrong with any of this work; it was stopped. Recording it as
    // failed would tell a later reader not to bother re-running it.
    assert!(
        states
            .iter()
            .all(|state| !matches!(state, AttemptState::Failed { .. })),
        "a cancelled run recorded a failure: {states:?}"
    );
    assert!(
        states
            .iter()
            .any(|state| matches!(state, AttemptState::Cancelled { .. })),
        "a cancelled run recorded nothing as cancelled: {states:?}"
    );
}

#[test]
fn cancelled_attempts_are_published_as_cancelled() {
    let mut engine = fixture_engine();
    let roots = register_action_leaves(&mut engine, 2);
    allow_actions(&mut engine, 2);
    let cancel = Arc::new(AtomicBool::new(false));
    let executor = CancellingExecutor {
        cancel: Arc::clone(&cancel),
        barrier: Arc::new(Barrier::new(2)),
    };

    let _ = engine
        .run_many_cancellable(
            &roots,
            &runtime(),
            &AllowAllActions,
            &executor,
            cancel.as_ref(),
        )
        .expect("the runtime drives the run");

    // The arena state and the durable record must agree: a reader that only has
    // the database has to see the same thing the graph does.
    let mut cancelled = 0_usize;
    for (computation, node) in engine.query().computations() {
        let Some(attempt) = engine.durable_attempt_for(computation) else {
            continue;
        };
        let record = engine
            .state_store()
            .attempt(attempt)
            .expect("the memory adapter reads back")
            .expect("a published attempt exists");
        match node.state {
            AttemptState::Cancelled { .. } => {
                cancelled += 1;
                assert!(
                    matches!(record.state, DurableAttemptState::Cancelled(_)),
                    "arena says cancelled, durable record says {:?}",
                    record.state
                );
            }
            _ => assert!(
                !matches!(record.state, DurableAttemptState::Cancelled(_)),
                "durable record says cancelled, arena says {:?}",
                node.state
            ),
        }
    }
    assert!(cancelled > 0, "nothing was cancelled");
}

#[test]
fn an_uncancelled_signal_lets_the_run_finish() {
    let mut engine = fixture_engine();
    let roots = register_action_leaves(&mut engine, 2);
    let cancel = AtomicBool::new(false);
    let executor = barrier_across(&mut engine, 2);

    let evaluations = engine
        .run_many_cancellable(&roots, &runtime(), &AllowAllActions, &executor, &cancel)
        .expect("the runtime drives the run")
        .expect("an uncancelled run produces results");

    assert_eq!(values(&evaluations), [Value::Int(0), Value::Int(2)]);
}

// ---------------------------------------------------------------------------
// Edges a caller can reach

#[test]
fn running_no_requests_produces_no_evaluations() {
    let mut engine = fixture_engine();

    let evaluations = engine
        .run_many(
            &[],
            &runtime(),
            &AllowAllActions,
            &BarrierExecutor::expecting(0),
        )
        .expect("the runtime drives the run")
        .expect("an empty run succeeds");

    assert!(evaluations.is_empty());
}

#[test]
fn roots_that_share_a_dependency_still_evaluate() {
    // `run_many` documents its roots as independent, but nothing stops a caller
    // passing two that reach the same computation. Neither sees the other's
    // result — the shared computation has not completed when the second root is
    // prepared — so it is evaluated twice. Wasteful, and it must still be
    // correct: two chains publishing the same computation key concurrently is
    // exactly the case that would corrupt a reusable index that assumed one.
    let mut engine = fixture_engine();
    engine.register_rule(
        pure_rule("shared", int_interface(0)),
        ConstantRule(Value::Int(7)),
    );
    for arity in [1_usize, 2] {
        engine.register_rule(
            pure_rule(&format!("consumer-{arity}"), int_interface(arity)),
            SumAllRule {
                dependencies: [pure_request("shared", int_interface(0), int_inputs(0))].into(),
            },
        );
    }
    let roots = [
        pure_request("consumer-1", int_interface(1), int_inputs(1)),
        pure_request("consumer-2", int_interface(2), int_inputs(2)),
    ];

    let evaluations = engine
        .run_many(
            &roots,
            &runtime(),
            &AllowAllActions,
            &BarrierExecutor::expecting(0),
        )
        .expect("the runtime drives the run")
        .expect("both roots evaluate");

    assert_eq!(values(&evaluations), [Value::Int(7), Value::Int(7)]);
    assert_no_pending_attempts(&engine);
}

#[test]
fn the_same_root_requested_twice_evaluates_twice() {
    let mut engine = fixture_engine();
    engine.register_rule(
        pure_rule("leaf", int_interface(0)),
        ConstantRule(Value::Int(5)),
    );
    let request = pure_request("leaf", int_interface(0), int_inputs(0));

    let evaluations = engine
        .run_many(
            &[request.clone(), request],
            &runtime(),
            &AllowAllActions,
            &BarrierExecutor::expecting(0),
        )
        .expect("the runtime drives the run")
        .expect("both roots evaluate");

    assert_eq!(values(&evaluations), [Value::Int(5), Value::Int(5)]);
    assert_no_pending_attempts(&engine);
}
