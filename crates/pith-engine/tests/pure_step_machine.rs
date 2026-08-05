use pith_core::{Interface, Request, Rule, Type, Value};
use pith_diag::{PithResult, Span, StableCode};
use pith_engine::{DependencyEdge, Engine, PureRule, PureRuleFrame, PureStep};

struct ConstantRule(Value);

impl PureRule for ConstantRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(ConstantFrame(self.0.clone()))
    }
}

struct ConstantFrame(Value);

impl PureRuleFrame for ConstantFrame {
    fn step(&mut self, _input: Option<Value>) -> PithResult<PureStep> {
        Ok(PureStep::Complete(self.0.clone()))
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
    fn step(&mut self, input: Option<Value>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::Need(self.dependency.clone()));
        }

        let value = match input {
            Some(Value::Int(value)) => Value::Int(value.saturating_add(1)),
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
    fn step(&mut self, input: Option<Value>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::Need(self.dependency.clone()));
        }
        Ok(PureStep::Complete(input.unwrap_or(Value::Unit)))
    }
}

struct CountdownRule {
    interface: Interface,
}

impl PureRule for CountdownRule {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        let remaining = match inputs.first() {
            Some(Value::Int(value)) => *value,
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
    fn step(&mut self, input: Option<Value>) -> PithResult<PureStep> {
        if self.remaining > 0 && !self.requested {
            self.requested = true;
            return Ok(PureStep::Need(request(
                "countdown",
                self.interface.clone(),
                [Value::Int(self.remaining.saturating_sub(1))],
            )));
        }
        Ok(PureStep::Complete(
            input.unwrap_or(Value::Int(self.remaining)),
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
    Rule::new(label, interface, Span::none())
}

fn request(label: &str, interface: Interface, inputs: impl Into<Box<[Value]>>) -> Request {
    Request::new(label, interface, inputs, Span::none())
}

#[test]
fn leaf_rule_returns_its_value() {
    let mut engine = Engine::new();
    let leaf = interface(&[], Type::Int);
    engine.register_rule(
        rule("leaf provider", leaf.clone()),
        ConstantRule(Value::Int(41)),
    );

    let evaluation = engine
        .evaluate_pure(&request("diagnostic leaf label", leaf, []))
        .unwrap();

    assert!(matches!(evaluation.value, Value::Int(41)));
    let node = engine.query().computation(evaluation.computation).unwrap();
    assert!(matches!(node.result, Some(Value::Int(41))));
}

#[test]
fn selection_query_does_not_evaluate_the_rule() {
    let mut engine = Engine::new();
    let signature = interface(&[], Type::Int);
    let rule = engine.register_rule(
        rule("provider", signature.clone()),
        ConstantRule(Value::Int(41)),
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
        .evaluate_pure(&request("seven", signature, [Value::Int(7)]))
        .unwrap();

    assert_eq!(evaluation.value, Value::Int(7));
}

#[test]
fn parent_resumes_with_child_value_and_records_the_edge() {
    let mut engine = Engine::new();
    let leaf = interface(&[], Type::Int);
    let parent = interface(&[Type::Int], Type::Int);
    engine.register_rule(
        rule("leaf provider", leaf.clone()),
        ConstantRule(Value::Int(41)),
    );
    engine.register_rule(
        rule("increment provider", parent.clone()),
        IncrementRule {
            dependency: request("base value", leaf.clone(), []),
        },
    );

    let evaluation = engine
        .evaluate_pure(&request("requested answer", parent, [Value::Int(0)]))
        .unwrap();

    assert!(matches!(evaluation.value, Value::Int(42)));
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
            assert!(matches!(child.result, Some(Value::Int(41))));
            let dependents: Vec<_> = query.dependents_of(*computation).collect();
            assert_eq!(dependents.len(), 1);
            assert_eq!(dependents.first().unwrap().0, evaluation.computation);
        }
        DependencyEdge::Blob { .. } | DependencyEdge::Action { .. } => {
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
            dependency: request("need b", b.clone(), [Value::Int(0)]),
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
    assert_eq!(diagnostic.code, StableCode::engine(203));
    assert_eq!(
        diagnostic.message.0.as_ref(),
        "dependency cycle: start a -> need b -> need a"
    );
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
        .evaluate_pure(&request("countdown", signature, [Value::Int(3)]))
        .unwrap();

    assert_eq!(evaluation.value, Value::Int(0));
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
fn independent_evaluations_keep_distinct_computations() {
    let mut engine = Engine::new();
    let leaf = interface(&[], Type::Int);
    engine.register_rule(
        rule("leaf provider", leaf.clone()),
        ConstantRule(Value::Int(41)),
    );

    let first = engine
        .evaluate_pure(&request("first evaluation", leaf.clone(), []))
        .unwrap();
    let second = engine
        .evaluate_pure(&request("second evaluation", leaf, []))
        .unwrap();

    assert_ne!(first.computation, second.computation);
    assert!(engine.query().computation(first.computation).is_some());
    assert!(engine.query().computation(second.computation).is_some());
}

#[test]
fn computation_ids_do_not_cross_engine_instances() {
    let signature = interface(&[], Type::Int);
    let mut first = Engine::new();
    first.register_rule(
        rule("first provider", signature.clone()),
        ConstantRule(Value::Int(1)),
    );
    let first_evaluation = first
        .evaluate_pure(&request("first request", signature.clone(), []))
        .unwrap();

    let mut second = Engine::new();
    second.register_rule(
        rule("second provider", signature.clone()),
        ConstantRule(Value::Int(2)),
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
