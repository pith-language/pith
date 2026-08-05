use pith_core::{Interface, Request, Rule, Value};
use pith_diag::{PithResult, Span, StableCode};
use pith_engine::{DependencyEdge, Engine, PureRule, PureRuleFrame, PureStep};

struct ConstantRule(Value);

impl PureRule for ConstantRule {
    fn start(&self) -> Box<dyn PureRuleFrame> {
        Box::new(ConstantFrame(self.0.clone()))
    }
}

struct ConstantFrame(Value);

impl PureRuleFrame for ConstantFrame {
    fn step(&mut self, _input: Option<Value>) -> PithResult<PureStep> {
        Ok(PureStep::Complete(self.0.clone()))
    }
}

struct IncrementRule {
    dependency: Box<str>,
}

impl PureRule for IncrementRule {
    fn start(&self) -> Box<dyn PureRuleFrame> {
        Box::new(IncrementFrame {
            dependency: self.dependency.clone(),
            requested: false,
        })
    }
}

struct IncrementFrame {
    dependency: Box<str>,
    requested: bool,
}

impl PureRuleFrame for IncrementFrame {
    fn step(&mut self, input: Option<Value>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::Need(request(&self.dependency)));
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
    dependency: Box<str>,
}

impl PureRule for ForwardRule {
    fn start(&self) -> Box<dyn PureRuleFrame> {
        Box::new(ForwardFrame {
            dependency: self.dependency.clone(),
            requested: false,
        })
    }
}

struct ForwardFrame {
    dependency: Box<str>,
    requested: bool,
}

impl PureRuleFrame for ForwardFrame {
    fn step(&mut self, input: Option<Value>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::Need(request(&self.dependency)));
        }
        Ok(PureStep::Complete(input.unwrap_or(Value::Unit)))
    }
}

fn rule(label: &str) -> Rule {
    Rule {
        interface: Interface {
            label: label.into(),
            output_kind: "Artifact".into(),
        },
        span: Span::none(),
    }
}

fn request(label: &str) -> Request {
    Request {
        label: label.into(),
        span: Span::none(),
    }
}

#[test]
fn leaf_rule_returns_its_value() {
    let mut engine = Engine::new();
    engine.register_rule(rule("leaf"), ConstantRule(Value::Int(41)));

    let evaluation = engine.evaluate(&request("leaf")).unwrap();

    assert!(matches!(evaluation.value, Value::Int(41)));
    let node = engine.computation(evaluation.computation).unwrap();
    assert!(matches!(node.result, Some(Value::Int(41))));
}

#[test]
fn parent_resumes_with_child_value_and_records_the_edge() {
    let mut engine = Engine::new();
    engine.register_rule(rule("leaf"), ConstantRule(Value::Int(41)));
    engine.register_rule(
        rule("parent"),
        IncrementRule {
            dependency: "leaf".into(),
        },
    );

    let evaluation = engine.evaluate(&request("parent")).unwrap();

    assert!(matches!(evaluation.value, Value::Int(42)));
    let dependencies = engine.dependencies_of(evaluation.computation).unwrap();
    assert_eq!(dependencies.len(), 1);
    match dependencies.first().unwrap() {
        DependencyEdge::Request {
            request,
            computation,
        } => {
            assert_eq!(request.label.as_ref(), "leaf");
            let child = engine.computation(*computation).unwrap();
            assert!(matches!(child.result, Some(Value::Int(41))));
        }
    }
}

#[test]
fn dependency_cycle_reports_the_request_chain() {
    let mut engine = Engine::new();
    engine.register_rule(
        rule("a"),
        ForwardRule {
            dependency: "b".into(),
        },
    );
    engine.register_rule(
        rule("b"),
        ForwardRule {
            dependency: "a".into(),
        },
    );

    let err = engine.evaluate(&request("a")).unwrap_err();
    let diagnostics: Vec<_> = err.iter().collect();
    let diagnostic = diagnostics.first().unwrap();
    assert_eq!(diagnostic.code, StableCode::engine(203));
    assert_eq!(
        diagnostic.message.0.as_ref(),
        "dependency cycle: a -> b -> a"
    );
}

#[test]
fn independent_evaluations_keep_distinct_computations() {
    let mut engine = Engine::new();
    engine.register_rule(rule("leaf"), ConstantRule(Value::Int(41)));

    let first = engine.evaluate(&request("leaf")).unwrap();
    let second = engine.evaluate(&request("leaf")).unwrap();

    assert_ne!(first.computation, second.computation);
    assert!(engine.computation(first.computation).is_some());
    assert!(engine.computation(second.computation).is_some());
}
