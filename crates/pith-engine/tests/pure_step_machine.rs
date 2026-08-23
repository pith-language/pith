use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use pith_core::{Int, Interface, Request, Rule, RuleIdentity, RuleRevision, Type, Value};
use pith_diag::{Diag, DiagnosticSink, EngineCode, PithResult, Severity, Span, StableCode};
use pith_engine::{
    AttemptState, DependencyEdge, Engine, EvaluationSource, PureRule, PureRuleFrame, PureStep,
    Resumption,
};

#[path = "support/constant_rule.rs"]
mod constant_rule_support;

use constant_rule_support::{ConstantFrame, ConstantRule};

struct FailingRule;

impl PureRule for FailingRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(FailingFrame)
    }
}

struct FailingFrame;

impl PureRuleFrame for FailingFrame {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        let mut diagnostics = DiagnosticSink::new();
        diagnostics.push(Diag::new(
            Severity::Error,
            StableCode(1299),
            Span::none(),
            "fixture pure failure",
        ));
        Err(diagnostics)
    }
}

struct CountingRule {
    value: Value,
    starts: Arc<AtomicUsize>,
}

impl PureRule for CountingRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        self.starts.fetch_add(1, Ordering::Relaxed);
        Box::new(ConstantFrame(self.value.clone()))
    }
}

struct FirstInputRule;

impl PureRule for FirstInputRule {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(ConstantFrame(
            inputs.first().cloned().unwrap_or(Value::Unit),
        ))
    }
}

struct IncrementRule {
    dependency: Request,
}

impl PureRule for IncrementRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(IncrementFrame {
            dependency: self.dependency.clone(),
            requested: false,
        })
    }
}

struct IncrementFrame {
    dependency: Request,
    requested: bool,
}

impl PureRuleFrame for IncrementFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::Need(self.dependency.clone()));
        }

        let value = match input.and_then(Resumption::one) {
            Some(Value::Int(value)) => Value::Int(value.added(&Int::from(1))),
            Some(value) => value,
            None => Value::Unit,
        };
        Ok(PureStep::Complete(value))
    }
}

struct ForwardRule {
    dependency: Request,
}

impl PureRule for ForwardRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(ForwardFrame {
            dependency: self.dependency.clone(),
            requested: false,
        })
    }
}

struct ForwardFrame {
    dependency: Request,
    requested: bool,
}

impl PureRuleFrame for ForwardFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::Need(self.dependency.clone()));
        }
        Ok(PureStep::Complete(
            input.and_then(Resumption::one).unwrap_or(Value::Unit),
        ))
    }
}

struct CountdownRule {
    interface: Interface,
}

impl PureRule for CountdownRule {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        let remaining = match inputs.first() {
            Some(Value::Int(value)) => value.to_i64().unwrap_or(0),
            _ => 0,
        };
        Box::new(CountdownFrame {
            interface: self.interface.clone(),
            remaining,
            requested: false,
        })
    }
}

struct CountdownFrame {
    interface: Interface,
    remaining: i64,
    requested: bool,
}

impl PureRuleFrame for CountdownFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if self.remaining > 0 && !self.requested {
            self.requested = true;
            return Ok(PureStep::Need(request(
                "countdown",
                self.interface.clone(),
                [Value::int(self.remaining.saturating_sub(1))],
            )));
        }
        Ok(PureStep::Complete(
            input
                .and_then(Resumption::one)
                .unwrap_or(Value::int(self.remaining)),
        ))
    }
}

/// Needs the same request twice in sequence, then completes with the second
/// result. The two requests are siblings, not nested: the first has completed
/// and left the stack before the second is made.
struct TwiceRule {
    dependency: Request,
}

impl PureRule for TwiceRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(TwiceFrame {
            dependency: self.dependency.clone(),
            made: 0,
        })
    }
}

struct TwiceFrame {
    dependency: Request,
    made: u8,
}

impl PureRuleFrame for TwiceFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if self.made < 2 {
            self.made = self.made.saturating_add(1);
            return Ok(PureStep::Need(self.dependency.clone()));
        }
        Ok(PureStep::Complete(
            input.and_then(Resumption::one).unwrap_or(Value::Unit),
        ))
    }
}

fn interface(inputs: &[Type], output: Type) -> Interface {
    Interface {
        inputs: inputs.to_vec().into_boxed_slice(),
        output,
    }
}

fn rule(label: &str, interface: Interface) -> Rule {
    let identity = RuleIdentity::of_module_declaration("pure-step-machine-tests", label);
    let revision = RuleRevision::of_manifest(identity, b"pure-step-machine-tests-v1");
    Rule::new(revision, label, interface, Span::none())
}

fn request(label: &str, interface: Interface, inputs: impl Into<Box<[Value]>>) -> Request {
    Request::new(label, interface, inputs, Span::none())
}

fn assert_no_pending_attempts(engine: &Engine) {
    assert!(
        engine
            .query()
            .computations()
            .all(|(_, node)| !matches!(node.state, AttemptState::Pending))
    );
}

#[test]
fn leaf_rule_returns_its_value() {
    let mut engine = Engine::new();
    let leaf = interface(&[], Type::Int);
    engine.register_rule(
        rule("leaf provider", leaf.clone()),
        ConstantRule(Value::int(41)),
    );

    let evaluation = engine
        .evaluate_pure(&request("diagnostic leaf label", leaf, []))
        .unwrap();

    assert_eq!(evaluation.value, Value::int(41));
    let node = engine.query().computation(evaluation.computation).unwrap();
    let AttemptState::Complete { result, .. } = &node.state else {
        unreachable!("the attempt completed")
    };
    assert_eq!(*result, Value::int(41));
}

#[test]
fn selection_query_does_not_evaluate_the_rule() {
    let mut engine = Engine::new();
    let signature = interface(&[], Type::Int);
    let rule = engine.register_rule(
        rule("provider", signature.clone()),
        ConstantRule(Value::int(41)),
    );
    let request = request("answer", signature.clone(), []);

    let query = engine.query();
    let selection = query.select(&request).unwrap();

    assert_eq!(selection.rule, rule);
    assert_eq!(selection.interface, signature);
    assert_eq!(
        query.rule(selection.rule).unwrap().label.as_ref(),
        "provider"
    );
    assert_eq!(query.rules().count(), 1);
    assert_eq!(query.computations().count(), 0);
}

#[test]
fn rule_body_receives_request_inputs() {
    let mut engine = Engine::new();
    let signature = interface(&[Type::Int], Type::Int);
    engine.register_rule(rule("identity", signature.clone()), FirstInputRule);

    let evaluation = engine
        .evaluate_pure(&request("seven", signature, [Value::int(7)]))
        .unwrap();

    assert_eq!(evaluation.value, Value::int(7));
}

#[test]
fn parent_resumes_with_child_value_and_records_the_edge() {
    let mut engine = Engine::new();
    let leaf = interface(&[], Type::Int);
    let parent = interface(&[Type::Int], Type::Int);
    engine.register_rule(
        rule("leaf provider", leaf.clone()),
        ConstantRule(Value::int(41)),
    );
    engine.register_rule(
        rule("increment provider", parent.clone()),
        IncrementRule {
            dependency: request("base value", leaf.clone(), []),
        },
    );

    let evaluation = engine
        .evaluate_pure(&request("requested answer", parent, [Value::int(0)]))
        .unwrap();

    assert_eq!(evaluation.value, Value::int(42));
    let query = engine.query();
    let dependencies = query.dependencies_of(evaluation.computation).unwrap();
    assert_eq!(dependencies.len(), 1);
    match dependencies.first().unwrap() {
        DependencyEdge::Request {
            request,
            computation,
        } => {
            assert_eq!(request.label.as_ref(), "base value");
            assert_eq!(request.interface, leaf);
            let child = query.computation(*computation).unwrap();
            let AttemptState::Complete { result, .. } = &child.state else {
                unreachable!("the dependency completed")
            };
            assert_eq!(*result, Value::int(41));
            let dependents: Vec<_> = query.dependents_of(*computation).collect();
            assert_eq!(dependents.len(), 1);
            assert_eq!(dependents.first().unwrap().0, evaluation.computation);
        }
        DependencyEdge::Blob { .. }
        | DependencyEdge::CapabilityUse { .. }
        | DependencyEdge::Action { .. }
        | DependencyEdge::Observation { .. } => {
            unreachable!("this test exercises pure-rule dependencies only")
        }
    }
}

#[test]
fn dependency_cycle_reports_the_request_chain() {
    let mut engine = Engine::new();
    let a = interface(&[Type::Bool], Type::Int);
    let b = interface(&[Type::Int], Type::Bool);
    engine.register_rule(
        rule("a provider", a.clone()),
        ForwardRule {
            dependency: request("need b", b.clone(), [Value::int(0)]),
        },
    );
    engine.register_rule(
        rule("b provider", b.clone()),
        ForwardRule {
            dependency: request("need a", a.clone(), [Value::Bool(false)]),
        },
    );

    let err = engine
        .evaluate_pure(&request("start a", a, [Value::Bool(false)]))
        .unwrap_err();
    let diagnostics: Vec<_> = err.iter().collect();
    let diagnostic = diagnostics.first().unwrap();
    assert_eq!(
        diagnostic.code,
        StableCode::from(EngineCode::DependencyCycle)
    );
    assert_eq!(
        diagnostic.message.0.as_ref(),
        "dependency cycle: start a -> need b -> need a"
    );
    assert_no_pending_attempts(&engine);
    assert!(engine.query().computations().all(|(_, node)| matches!(
        node.state,
        AttemptState::Failed { ref diagnostics }
            if diagnostics.first().map(|diagnostic| diagnostic.code)
                == Some(StableCode::from(EngineCode::DependencyCycle))
    )));
}

#[test]
fn child_failure_finalizes_the_child_and_its_ancestors() {
    let mut engine = Engine::new();
    let child = interface(&[], Type::Bool);
    let parent = interface(&[], Type::Int);
    engine.register_rule(rule("failing child", child.clone()), FailingRule);
    engine.register_rule(
        rule("parent", parent.clone()),
        ForwardRule {
            dependency: request("child", child, []),
        },
    );

    let diagnostics = engine
        .evaluate_pure(&request("root", parent, []))
        .unwrap_err();

    assert_eq!(
        diagnostics.iter().next().map(|diagnostic| diagnostic.code),
        Some(StableCode(1299))
    );
    assert_no_pending_attempts(&engine);
    assert_eq!(
        engine
            .query()
            .computations()
            .filter(|(_, node)| matches!(node.state, AttemptState::Failed { .. }))
            .count(),
        2
    );
}

#[test]
fn result_type_failure_finalizes_the_attempt() {
    let mut engine = Engine::new();
    let signature = interface(&[], Type::Int);
    engine.register_rule(
        rule("wrong result", signature.clone()),
        ConstantRule(Value::Unit),
    );

    let diagnostics = engine
        .evaluate_pure(&request("wrong result", signature, []))
        .unwrap_err();

    assert_eq!(
        diagnostics.iter().next().map(|diagnostic| diagnostic.code),
        Some(StableCode::from(EngineCode::ResultTypeMismatch))
    );
    assert_no_pending_attempts(&engine);
    assert!(engine.query().computations().all(|(_, node)| matches!(
        node.state,
        AttemptState::Failed { ref diagnostics }
            if diagnostics.first().map(|diagnostic| diagnostic.code)
                == Some(StableCode::from(EngineCode::ResultTypeMismatch))
    )));
}

#[test]
fn repeated_rule_with_different_inputs_is_not_a_cycle() {
    let mut engine = Engine::new();
    let signature = interface(&[Type::Int], Type::Int);
    engine.register_rule(
        rule("countdown", signature.clone()),
        CountdownRule {
            interface: signature.clone(),
        },
    );

    let evaluation = engine
        .evaluate_pure(&request("countdown", signature, [Value::int(3)]))
        .unwrap();

    assert_eq!(evaluation.value, Value::int(0));
    assert_eq!(
        engine
            .query()
            .dependencies_of(evaluation.computation)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn repeated_evaluations_reuse_the_completed_computation() {
    let mut engine = Engine::new();
    let leaf = interface(&[], Type::Int);
    let starts = Arc::new(AtomicUsize::new(0));
    engine.register_rule(
        rule("leaf provider", leaf.clone()),
        CountingRule {
            value: Value::int(41),
            starts: starts.clone(),
        },
    );

    let first = engine
        .evaluate_pure(&request("first evaluation", leaf.clone(), []))
        .unwrap();
    let second = engine
        .evaluate_pure(&request("second evaluation", leaf, []))
        .unwrap();

    assert_eq!(first.source, EvaluationSource::Computed);
    assert_eq!(second.source, EvaluationSource::Reused);
    assert_eq!(first.computation, second.computation);
    assert_eq!(starts.load(Ordering::Relaxed), 1);
    assert_eq!(engine.query().computations().count(), 1);
    assert!(engine.query().computation(first.computation).is_some());
}

#[test]
fn different_inputs_create_different_computations() {
    let mut engine = Engine::new();
    let signature = interface(&[Type::Int], Type::Int);
    engine.register_rule(rule("identity", signature.clone()), FirstInputRule);

    let seven = engine
        .evaluate_pure(&request("seven", signature.clone(), [Value::int(7)]))
        .unwrap();
    let eight = engine
        .evaluate_pure(&request("eight", signature, [Value::int(8)]))
        .unwrap();

    assert_ne!(seven.computation, eight.computation);
    assert_eq!(seven.value, Value::int(7));
    assert_eq!(eight.value, Value::int(8));
}

#[test]
fn distinct_parents_share_a_completed_dependency() {
    let mut engine = Engine::new();
    let leaf = interface(&[], Type::Int);
    let boolean_parent = interface(&[Type::Bool], Type::Int);
    let text_parent = interface(&[Type::Text], Type::Int);
    let leaf_starts = Arc::new(AtomicUsize::new(0));
    engine.register_rule(
        rule("leaf provider", leaf.clone()),
        CountingRule {
            value: Value::int(41),
            starts: leaf_starts.clone(),
        },
    );
    engine.register_rule(
        rule("boolean parent", boolean_parent.clone()),
        ForwardRule {
            dependency: request("shared leaf", leaf.clone(), []),
        },
    );
    engine.register_rule(
        rule("text parent", text_parent.clone()),
        ForwardRule {
            dependency: request("shared leaf", leaf, []),
        },
    );

    let boolean_result = engine
        .evaluate_pure(&request(
            "boolean root",
            boolean_parent,
            [Value::Bool(true)],
        ))
        .unwrap();
    let text_result = engine
        .evaluate_pure(&request(
            "text root",
            text_parent,
            [Value::Text("input".into())],
        ))
        .unwrap();

    assert_eq!(boolean_result.value, Value::int(41));
    assert_eq!(text_result.value, Value::int(41));
    assert_eq!(leaf_starts.load(Ordering::Relaxed), 1);
    assert_eq!(engine.query().computations().count(), 3);
}

#[test]
fn computation_ids_do_not_cross_engine_instances() {
    let signature = interface(&[], Type::Int);
    let mut first = Engine::new();
    first.register_rule(
        rule("first provider", signature.clone()),
        ConstantRule(Value::int(1)),
    );
    let first_evaluation = first
        .evaluate_pure(&request("first request", signature.clone(), []))
        .unwrap();

    let mut second = Engine::new();
    second.register_rule(
        rule("second provider", signature.clone()),
        ConstantRule(Value::int(2)),
    );
    let second_evaluation = second
        .evaluate_pure(&request("second request", signature, []))
        .unwrap();

    assert_ne!(first_evaluation.computation, second_evaluation.computation);
    assert!(
        second
            .query()
            .computation(first_evaluation.computation)
            .is_none()
    );
    assert!(
        first
            .query()
            .computation(second_evaluation.computation)
            .is_none()
    );
}

#[test]
fn the_same_request_twice_in_sequence_is_reuse_and_not_a_cycle() {
    // The property the cycle predicate's bookkeeping has to get right
    // (decision 0050): a frame's digest is live only while its own frame is on
    // the stack. Two sibling requests for one value are ordinary reuse, and the
    // cycle check runs before the reuse lookup, so a digest left behind by the
    // first would refuse the second.
    let mut engine = Engine::new();
    let root = interface(&[Type::Bool], Type::Int);
    let leaf = interface(&[Type::Int], Type::Int);
    engine.register_rule(rule("leaf", leaf.clone()), FirstInputRule);
    engine.register_rule(
        rule("root", root.clone()),
        TwiceRule {
            dependency: request("leaf", leaf, [Value::int(7)]),
        },
    );

    let evaluation = engine
        .evaluate_pure(&request("root", root, [Value::Bool(false)]))
        .unwrap();

    assert_eq!(evaluation.value, Value::int(7));
    // Both requests recorded an edge, and both name the one computation the
    // first request allocated.
    let dependencies = engine
        .query()
        .dependencies_of(evaluation.computation)
        .unwrap();
    assert_eq!(dependencies.len(), 2);
    let targets: Vec<_> = dependencies
        .iter()
        .filter_map(DependencyEdge::computation_id)
        .collect();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets.first(), targets.last());
}
