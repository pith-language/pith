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
use pith_diag::{EngineCode, PithResult, Span};
use pith_engine::state::DurableAttemptState;
use pith_engine::{
    AccessVerification, ActionExecution, ActionInvocation, ActionRule, AllowAllActions,
    AttemptState, CapturedActionExecution, CapturedExecutionReport, ComputationKind, Engine,
    Evaluation, ExecutionPlatform, Executor, ExecutorIdentity, PureRule, PureRuleFrame, PureStep,
    Resumption,
};
use pith_store::MemoryContentStore;
use tokio::sync::Barrier;

#[path = "support/action_dependency.rs"]
mod action_dependency_support;
#[path = "support/constant_rule.rs"]
mod constant_rule_support;
#[path = "support/diagnostic.rs"]
mod diagnostic;
#[path = "support/runtime.rs"]
mod runtime_support;

use action_dependency_support::ActionDepRule;
use constant_rule_support::ConstantRule;
use diagnostic::fixture_error;
use runtime_support::runtime;

/// How long an action waits at the barrier before giving up. Only a regression
/// reaches it: with real overlap every participant arrives immediately. The
/// timeout exists so a serialized engine fails the test instead of hanging it.
const BARRIER_TIMEOUT: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// Pure rules

/// Completes immediately with a constant. Used where a test needs distinct
/// values it can check the order of.
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
    fn identity(&self) -> ExecutorIdentity {
        fixture_identity()
    }

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
    fn identity(&self) -> ExecutorIdentity {
        fixture_identity()
    }

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

fn fixture_identity() -> ExecutorIdentity {
    ExecutorIdentity {
        executor: "fixture".into(),
        platform: ExecutionPlatform {
            operating_system: "fixture".into(),
            architecture: "fixture".into(),
        },
    }
}

fn empty_execution() -> CapturedActionExecution {
    let identity = fixture_identity();
    CapturedActionExecution {
        report: CapturedExecutionReport {
            executor: identity.executor,
            platform: identity.platform,
            access: AccessVerification::Prevented,
            outputs: Box::new([]),
            capabilities_used: Box::new([]),
        },
        exit: None,
    }
}

// ---------------------------------------------------------------------------
// Fixtures

fn fixture_executable() -> &'static str {
    "/bin/fixture-action"
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

struct CancellingExecutor {
    cancel: Arc<AtomicBool>,
    barrier: Arc<Barrier>,
}

#[async_trait::async_trait]
impl Executor for CancellingExecutor {
    fn identity(&self) -> ExecutorIdentity {
        fixture_identity()
    }

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

// ---------------------------------------------------------------------------
// Tests

#[path = "concurrency/cancellation.rs"]
mod cancellation;
#[path = "concurrency/failures.rs"]
mod failures;
#[path = "concurrency/scheduling.rs"]
mod scheduling;
#[path = "concurrency/shared_roots.rs"]
mod shared_roots;
