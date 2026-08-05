//! Tests that the engine crosses the sync/async boundary: pure rules that
//! depend on content-addressed blobs and on action results (decisions 0021,
//! 0022).

use pith_core::{Action, Interface, Pure, Request, Rule, Type, Value};
use pith_diag::{PithResult, Span, StableCode};
use pith_engine::{ActionRule, Engine, PureRule, PureRuleFrame, PureStep, TokioRuntime};
use pith_ids::ContentId;

struct BlobLenRule {
    blob: ContentId,
}

impl PureRule for BlobLenRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(BlobLenFrame {
            blob: self.blob,
            requested: false,
        })
    }
}

struct BlobLenFrame {
    blob: ContentId,
    requested: bool,
}

impl PureRuleFrame for BlobLenFrame {
    fn step(&mut self, _input: Option<Value>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::NeedBlob(self.blob));
        }
        let len = match _input {
            Some(Value::Bytes(b)) => b.len() as i64,
            _ => 0,
        };
        Ok(PureStep::Complete(Value::Int(len)))
    }
}

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
    fn step(&mut self, input: Option<Value>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::NeedAction(self.dependency.clone()));
        }
        Ok(PureStep::Complete(input.unwrap_or(Value::Int(0))))
    }
}

struct DoubleAction;

#[async_trait::async_trait]
impl ActionRule for DoubleAction {
    async fn execute(&self, inputs: &[Value]) -> PithResult<Value> {
        let n = match inputs.first() {
            Some(Value::Int(n)) => *n,
            _ => 0,
        };
        Ok(Value::Int(n.saturating_mul(2)))
    }
}

struct WrongTypeAction;

#[async_trait::async_trait]
impl ActionRule for WrongTypeAction {
    async fn execute(&self, _inputs: &[Value]) -> PithResult<Value> {
        Ok(Value::Int(0))
    }
}

fn interface(inputs: &[Type], output: Type) -> Interface {
    Interface {
        inputs: inputs.to_vec().into_boxed_slice(),
        output,
    }
}

fn pure_rule(label: &str, interface: Interface) -> Rule<Pure> {
    Rule::<Pure>::new(label, interface, Span::none())
}

fn pure_request(
    label: &str,
    interface: Interface,
    inputs: impl Into<Box<[Value]>>,
) -> Request<Pure> {
    Request::<Pure>::new(label, interface, inputs, Span::none())
}

fn action_rule(label: &str, interface: Interface) -> Rule<Action> {
    Rule::<Action>::new(label, interface, Span::none())
}

fn action_request(
    label: &str,
    interface: Interface,
    inputs: impl Into<Box<[Value]>>,
) -> Request<Action> {
    Request::<Action>::new(label, interface, inputs, Span::none())
}

#[test]
fn blob_dependency_resumes_with_bytes_and_records_edge() {
    let mut engine = Engine::new();
    let blob_id = engine.put_blob(b"hello").unwrap();
    engine.register_rule(
        pure_rule("length", interface(&[], Type::Int)),
        BlobLenRule { blob: blob_id },
    );

    let evaluation = engine
        .run(
            &pure_request("length", interface(&[], Type::Int), []),
            &TokioRuntime,
        )
        .unwrap()
        .unwrap();

    assert_eq!(evaluation.value, Value::Int(5));
    let deps = engine
        .query()
        .dependencies_of(evaluation.computation)
        .unwrap();
    assert!(matches!(
        deps.first(),
        Some(pith_engine::DependencyEdge::Blob { id }) if *id == blob_id
    ));
}

#[test]
fn missing_blob_reports_clean_diagnostic() {
    let mut engine = Engine::new();
    let absent = ContentId::of_blob(b"not stored");
    engine.register_rule(
        pure_rule("length", interface(&[], Type::Int)),
        BlobLenRule { blob: absent },
    );

    let result = engine
        .run(
            &pure_request("length", interface(&[], Type::Int), []),
            &TokioRuntime,
        )
        .unwrap();

    let err = result.unwrap_err();
    let diag = err.iter().next().unwrap();
    assert_eq!(diag.code, StableCode::engine(205));
}

#[test]
fn action_dependency_driven_through_run() {
    let mut engine = Engine::new();
    let action_iface = interface(&[Type::Int], Type::Int);
    let pure_iface = interface(&[], Type::Int);
    engine.register_action_rule(action_rule("double", action_iface.clone()), DoubleAction);
    engine.register_rule(
        pure_rule("entry", pure_iface.clone()),
        ActionDepRule {
            dependency: action_request("double", action_iface, [Value::Int(21)]),
        },
    );

    let evaluation = engine
        .run(&pure_request("entry", pure_iface, []), &TokioRuntime)
        .unwrap()
        .unwrap();

    assert_eq!(evaluation.value, Value::Int(42));
    let deps = engine
        .query()
        .dependencies_of(evaluation.computation)
        .unwrap();
    assert!(matches!(
        deps.first(),
        Some(pith_engine::DependencyEdge::Action { .. })
    ));
}

#[test]
fn action_result_type_checked_against_interface() {
    let mut engine = Engine::new();
    let action_iface = interface(&[], Type::Bool);
    let pure_iface = interface(&[], Type::Int);
    engine.register_action_rule(action_rule("liar", action_iface), WrongTypeAction);
    engine.register_rule(
        pure_rule("entry", pure_iface.clone()),
        ActionDepRule {
            dependency: action_request("liar", interface(&[], Type::Bool), []),
        },
    );

    let result = engine
        .run(&pure_request("entry", pure_iface, []), &TokioRuntime)
        .unwrap();

    let err = result.unwrap_err();
    let diag = err.iter().next().unwrap();
    assert_eq!(diag.code, StableCode::engine(104));
}

#[test]
fn effectful_step_in_pure_only_evaluation_is_rejected() {
    let mut engine = Engine::new();
    let blob_id = engine.put_blob(b"x").unwrap();
    engine.register_rule(
        pure_rule("length", interface(&[], Type::Int)),
        BlobLenRule { blob: blob_id },
    );

    let err = engine
        .evaluate_pure(&pure_request("length", interface(&[], Type::Int), []))
        .unwrap_err();
    let diag = err.iter().next().unwrap();
    assert_eq!(diag.code, StableCode::engine(206));
}
