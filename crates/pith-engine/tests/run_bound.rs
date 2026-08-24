//! A run's declared ceiling: the wall clock at the scheduling boundaries and
//! in the child, the step budget inside the step machine (decision 0059).
//!
//! The four shapes here are the four ways a bound stops a run: a body that
//! yields fresh requests forever, a deadline already spent, a deadline with
//! real work still in front of it, and an action whose executor refused at its
//! own deadline. The control is an ordinary run under a generous bound, which
//! the bound must not touch.

use std::time::{Duration, Instant};

use pith_core::{
    Action, ActionSpec, Interface, Pure, Request, Rule, RuleIdentity, RuleRevision, Type, Value,
};
use pith_diag::{Diag, EngineCode, PithResult, Severity, Span, StableCode};
use pith_engine::{
    AllowAllActions, AttemptState, CapturedActionExecution, Engine, ExecutionPlatform, Executor,
    ExecutorIdentity, PureRule, PureRuleFrame, PureStep, Resumption, RunBound,
};

#[path = "support/constant_rule.rs"]
mod constant_rule_support;

use constant_rule_support::ConstantRule;

#[path = "support/runtime.rs"]
mod runtime_support;

use runtime_support::runtime;

fn bound_code() -> StableCode {
    StableCode::from(EngineCode::RunBoundExceeded)
}

fn rule<K: pith_core::EffectCategory>(label: &str, output: Type) -> Rule<K> {
    let identity = RuleIdentity::of_module_declaration("run-bound-tests", label);
    let revision = RuleRevision::of_manifest(identity, b"run-bound-tests-v1");
    Rule::new(
        "run-bound-tests",
        revision,
        label,
        Interface {
            inputs: Box::new([]),
            output,
        },
        Span::none(),
    )
}

fn request(label: &str, output: Type) -> Request<Pure> {
    Request::new(
        label,
        Interface {
            inputs: Box::new([]),
            output,
        },
        [],
        Span::none(),
    )
}

fn attempt_states(engine: &Engine) -> Vec<AttemptState> {
    engine
        .query()
        .computations()
        .map(|(_, node)| node.state.clone())
        .collect()
}

fn assert_stopped_not_broken(engine: &Engine) {
    let states = attempt_states(engine);
    assert!(!states.is_empty(), "the run should have allocated work");
    assert!(
        states
            .iter()
            .all(|state| !matches!(state, AttemptState::Pending)),
        "a stopped run left work pending: {states:?}"
    );
    assert!(
        states
            .iter()
            .all(|state| !matches!(state, AttemptState::Failed { .. })),
        "a bound stop recorded a failure, and nothing was wrong with the work: {states:?}"
    );
    assert!(
        states
            .iter()
            .any(|state| matches!(state, AttemptState::Cancelled { .. })),
        "a bound stop recorded nothing as cancelled: {states:?}"
    );
}

fn bound_code_of(diagnostics: &pith_diag::DiagnosticSink) -> Vec<StableCode> {
    diagnostics.iter().map(|diag| diag.code).collect()
}

/// A body that yields a fresh request forever. Every request is distinct, so
/// the cycle predicate cannot refuse the chain (decision 0050) — this is the
/// shape the step budget exists to bound.
struct SpinRule {
    target: Interface,
}

impl PureRule for SpinRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(SpinFrame {
            target: self.target.clone(),
            spins: 0,
        })
    }
}

struct SpinFrame {
    target: Interface,
    spins: u64,
}

impl PureRuleFrame for SpinFrame {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        self.spins = self.spins.saturating_add(1);
        let label = format!("spin-{}", self.spins);
        Ok(PureStep::Need(Request::<Pure>::new(
            label,
            self.target.clone(),
            [],
            Span::none(),
        )))
    }
}

#[test]
fn a_body_that_yields_forever_is_stopped_by_the_step_budget() {
    let mut engine = Engine::new();
    engine.register_rule(rule("const", Type::Int), ConstantRule(Value::int(7)));
    engine.register_rule(
        rule("spin", Type::Text),
        SpinRule {
            target: Interface {
                inputs: Box::new([]),
                output: Type::Int,
            },
        },
    );

    let diagnostics = engine
        .run_bounded(
            &request("spin", Type::Text),
            &runtime(),
            &AllowAllActions,
            &RefusingExecutor,
            &RunBound::none().with_step_budget(50),
        )
        .expect("the runtime drives the run")
        .expect_err("a spinning body does not produce a result");

    assert_eq!(bound_code_of(&diagnostics), vec![bound_code()]);
    let message = diagnostics
        .iter()
        .next()
        .expect("one diagnostic")
        .message
        .0
        .as_ref();
    assert!(
        message.contains("spin") && message.contains("50"),
        "the diagnostic names the runaway and the budget, got: {message}"
    );
    assert_stopped_not_broken(&engine);
}

#[test]
fn an_expired_deadline_stops_the_run_before_any_work() {
    let mut engine = Engine::new();
    engine.register_rule(rule("const", Type::Int), ConstantRule(Value::int(7)));
    let spent = Instant::now() - Duration::from_millis(1);

    let diagnostics = engine
        .run_bounded(
            &request("const", Type::Int),
            &runtime(),
            &AllowAllActions,
            &RefusingExecutor,
            &RunBound::none().with_deadline(spent),
        )
        .expect("the runtime drives the run")
        .expect_err("a run past its deadline does not produce a result");

    assert_eq!(bound_code_of(&diagnostics), vec![bound_code()]);
    assert_stopped_not_broken(&engine);
}

#[test]
fn a_generous_bound_leaves_an_ordinary_run_untouched() {
    let mut engine = Engine::new();
    engine.register_rule(rule("const", Type::Int), ConstantRule(Value::int(7)));

    let evaluation = engine
        .run_bounded(
            &request("const", Type::Int),
            &runtime(),
            &AllowAllActions,
            &RefusingExecutor,
            &RunBound::none()
                .with_deadline(Instant::now() + Duration::from_secs(60))
                .with_step_budget(100_000),
        )
        .expect("the runtime drives the run")
        .expect("a run inside its bound produces a result");

    assert_eq!(evaluation.value, Value::int(7));
}

// ---------------------------------------------------------------------------
// An action refused at its own deadline

/// Plans an empty contract and derives `1` from whatever the executor reports.
struct UnitActionRule;

impl pith_engine::ActionRule for UnitActionRule {
    fn plan(&self, _inputs: &[Value]) -> PithResult<ActionSpec> {
        Ok(ActionSpec {
            executable: pith_core::ActionProgram::HostPath("/bin/true".into()),
            toolchain: Box::new([]),
            arguments: Box::new([]),
            inputs: Box::new([]),
            outputs: Box::new([]),
            environment: Box::new([]),
            platform: pith_core::PlatformRequirement::Any,
            capabilities: Box::new([]),
            network: pith_core::NetworkPolicy::Deny,
            exit_status: pith_core::ExitStatusContract::SuccessRequired,
        })
    }

    fn complete(
        &self,
        _inputs: &[Value],
        _execution: &pith_engine::ActionExecution,
    ) -> PithResult<Value> {
        Ok(Value::int(1))
    }
}

/// Refuses with the bound's code, standing in for a first-party executor whose
/// child was killed at the deadline (decision 0059): the run's own clock is
/// far away, so what is measured here is the engine's classification of the
/// refusal, not the clock.
struct TimedOutExecutor;

#[async_trait::async_trait]
impl Executor for TimedOutExecutor {
    fn identity(&self) -> ExecutorIdentity {
        ExecutorIdentity {
            executor: "timed-out-fixture".into(),
            platform: ExecutionPlatform {
                operating_system: "test".into(),
                architecture: "test".into(),
            },
        }
    }

    async fn execute(
        &self,
        _invocation: &pith_engine::ActionInvocation,
    ) -> PithResult<CapturedActionExecution> {
        let diag = Diag::new(
            Severity::Error,
            bound_code(),
            Span::none(),
            "the action exceeded the wall-clock bound its run declared",
        );
        let mut sink = pith_diag::DiagnosticSink::new();
        sink.push(diag);
        Err(sink)
    }
}

/// Requests one action, then completes with the value it resumed with.
struct ActionCallerRule {
    action: Interface,
}

impl PureRule for ActionCallerRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(ActionCallerFrame {
            action: self.action.clone(),
            requested: false,
        })
    }
}

struct ActionCallerFrame {
    action: Interface,
    requested: bool,
}

impl PureRuleFrame for ActionCallerFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::NeedAction(Request::<Action>::new(
                "call",
                self.action.clone(),
                [],
                Span::none(),
            )));
        }
        let Some(Resumption::One(value)) = input else {
            return Err(fixture_error("an action must resume with One"));
        };
        Ok(PureStep::Complete(value))
    }
}

fn fixture_error(message: &str) -> pith_diag::DiagnosticSink {
    let diag = Diag::new(Severity::Error, StableCode(1899), Span::none(), message);
    let mut sink = pith_diag::DiagnosticSink::new();
    sink.push(diag);
    sink
}

/// An executor with nothing to run, for runs that never reach an action.
struct RefusingExecutor;

#[async_trait::async_trait]
impl Executor for RefusingExecutor {
    fn identity(&self) -> ExecutorIdentity {
        ExecutorIdentity {
            executor: "refusing-fixture".into(),
            platform: ExecutionPlatform {
                operating_system: "test".into(),
                architecture: "test".into(),
            },
        }
    }

    async fn execute(
        &self,
        _invocation: &pith_engine::ActionInvocation,
    ) -> PithResult<CapturedActionExecution> {
        Err(fixture_error("this run never starts an action"))
    }
}

#[test]
fn a_timed_out_action_fails_itself_and_cancels_what_was_waiting_on_it() {
    let mut engine = Engine::new();
    let action_interface = Interface {
        inputs: Box::new([]),
        output: Type::Int,
    };
    engine.register_action_rule(rule("unit-action", Type::Int), UnitActionRule);
    engine.register_rule(
        rule("caller", Type::Int),
        ActionCallerRule {
            action: action_interface,
        },
    );

    let diagnostics = engine
        .run_bounded(
            &request("caller", Type::Int),
            &runtime(),
            &AllowAllActions,
            &TimedOutExecutor,
            &RunBound::none().with_deadline(Instant::now() + Duration::from_secs(60)),
        )
        .expect("the runtime drives the run")
        .expect_err("a timed-out action does not produce a result");

    assert_eq!(bound_code_of(&diagnostics), vec![bound_code()]);

    let states = attempt_states(&engine);
    assert!(
        states
            .iter()
            .all(|state| !matches!(state, AttemptState::Pending)),
        "a stopped run left work pending: {states:?}"
    );
    // The action ran and produced nothing within the authority it was given,
    // so its attempt is a failure carrying the bound's code — re-running it
    // needs more authority, and the record says so. The chain that was merely
    // waiting on it is cancelled: nothing about that work is known to be
    // wrong (decision 0059).
    assert!(
        states
            .iter()
            .any(|state| matches!(state, AttemptState::Failed { .. })),
        "the timed-out action was not recorded as failed: {states:?}"
    );
    assert!(
        states
            .iter()
            .any(|state| matches!(state, AttemptState::Cancelled { .. })),
        "the chain waiting on the action was not recorded as cancelled: {states:?}"
    );
    for state in &states {
        if let AttemptState::Failed { diagnostics } = state {
            assert_eq!(
                diagnostics.first().map(|diag| diag.code),
                Some(bound_code()),
                "the failed attempt should carry the bound's code"
            );
        }
    }
}
