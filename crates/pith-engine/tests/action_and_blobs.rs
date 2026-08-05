//! Tests that the engine crosses the sync/async boundary: pure rules that
//! depend on content-addressed blobs and on action results (decisions 0021,
//! 0022).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use pith_core::{
    Action, ActionSpec, CapabilityRequirement, Interface, Pure, Request, Rule, Type, Value,
};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_engine::{
    ActionExecution, ActionPlanner, ContractVerification, Engine, EvaluationSource,
    ExecutionEvidence, Executor, PureRule, PureRuleFrame, PureStep, TokioRuntime,
};
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

impl ActionPlanner for DoubleAction {
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec> {
        let n = match inputs.first() {
            Some(Value::Int(n)) => *n,
            _ => 0,
        };
        let mut spec = ActionSpec::isolated(double_executable());
        spec.arguments = [n.to_string().into_boxed_str()].into();
        Ok(spec)
    }
}

struct WrongTypeAction;

impl ActionPlanner for WrongTypeAction {
    fn plan(&self, _inputs: &[Value]) -> PithResult<ActionSpec> {
        Ok(ActionSpec::isolated(wrong_type_executable()))
    }
}

struct FixtureExecutor;

#[async_trait::async_trait]
impl Executor for FixtureExecutor {
    async fn execute(&self, spec: &ActionSpec) -> PithResult<ActionExecution> {
        let value = if spec.executable == double_executable() {
            let Some(argument) = spec.arguments.first() else {
                let mut diagnostics = DiagnosticSink::new();
                diagnostics.push(Diag::new(
                    Severity::Error,
                    StableCode::engine(211),
                    Span::none(),
                    "double action fixture requires one argument",
                ));
                return Err(diagnostics);
            };
            let Ok(number) = argument.parse::<i64>() else {
                let mut diagnostics = DiagnosticSink::new();
                diagnostics.push(Diag::new(
                    Severity::Error,
                    StableCode::engine(211),
                    Span::none(),
                    "double action fixture requires an integer argument",
                ));
                return Err(diagnostics);
            };
            Value::Int(number.saturating_mul(2))
        } else {
            Value::Int(0)
        };
        Ok(ActionExecution {
            value,
            evidence: ExecutionEvidence {
                executor: "fixture".into(),
                contract: ContractVerification::Enforced,
                outputs: Box::new([]),
                capabilities_used: Box::new([]),
            },
        })
    }
}

struct UndeclaredCapabilityExecutor;

#[async_trait::async_trait]
impl Executor for UndeclaredCapabilityExecutor {
    async fn execute(&self, _spec: &ActionSpec) -> PithResult<ActionExecution> {
        Ok(ActionExecution {
            value: Value::Int(42),
            evidence: ExecutionEvidence {
                executor: "fixture".into(),
                contract: ContractVerification::Observed,
                outputs: Box::new([]),
                capabilities_used: [CapabilityRequirement {
                    name: "fixture.clock".into(),
                    scope: "wall".into(),
                }]
                .into(),
            },
        })
    }
}

struct CountingExecutor {
    executions: Arc<AtomicUsize>,
}

struct UnverifiedCountingExecutor {
    executions: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl Executor for UnverifiedCountingExecutor {
    async fn execute(&self, spec: &ActionSpec) -> PithResult<ActionExecution> {
        self.executions.fetch_add(1, Ordering::Relaxed);
        let mut execution = FixtureExecutor.execute(spec).await?;
        execution.evidence.contract = ContractVerification::Unverified;
        Ok(execution)
    }
}

#[async_trait::async_trait]
impl Executor for CountingExecutor {
    async fn execute(&self, spec: &ActionSpec) -> PithResult<ActionExecution> {
        self.executions.fetch_add(1, Ordering::Relaxed);
        FixtureExecutor.execute(spec).await
    }
}

fn double_executable() -> ContentId {
    ContentId::of_blob(b"fixture:double")
}

fn wrong_type_executable() -> ContentId {
    ContentId::of_blob(b"fixture:wrong-type")
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
            &FixtureExecutor,
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
            &FixtureExecutor,
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

    let plan = engine
        .query()
        .plan_action(&action_request(
            "double",
            interface(&[Type::Int], Type::Int),
            [Value::Int(21)],
        ))
        .unwrap();
    assert_eq!(plan.spec.executable, double_executable());
    assert_eq!(plan.spec_digest, plan.spec.digest().unwrap());
    assert_eq!(
        plan.spec
            .arguments
            .first()
            .map(|argument| argument.as_ref()),
        Some("21")
    );

    let evaluation = engine
        .run(
            &pure_request("entry", pure_iface, []),
            &TokioRuntime,
            &FixtureExecutor,
        )
        .unwrap()
        .unwrap();

    assert_eq!(evaluation.value, Value::Int(42));
    let deps = engine
        .query()
        .dependencies_of(evaluation.computation)
        .unwrap();
    let action_computation = deps
        .first()
        .and_then(|edge| match edge {
            pith_engine::DependencyEdge::Action { computation, .. } => Some(*computation),
            _ => None,
        })
        .unwrap();
    let action = engine
        .query()
        .computation(action_computation)
        .and_then(|node| node.action.as_ref())
        .unwrap();
    assert_eq!(action.spec_digest, action.spec.digest().unwrap());
    assert_eq!(action.spec.executable, double_executable());
    assert_eq!(
        action.evidence.as_ref().map(|evidence| evidence.contract),
        Some(ContractVerification::Enforced)
    );
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
        .run(
            &pure_request("entry", pure_iface, []),
            &TokioRuntime,
            &FixtureExecutor,
        )
        .unwrap();

    let err = result.unwrap_err();
    let diag = err.iter().next().unwrap();
    assert_eq!(diag.code, StableCode::engine(104));
}

#[test]
fn undeclared_capability_use_is_rejected() {
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

    let result = engine
        .run(
            &pure_request("entry", pure_iface, []),
            &TokioRuntime,
            &UndeclaredCapabilityExecutor,
        )
        .unwrap();

    let err = result.unwrap_err();
    let diag = err.iter().next().unwrap();
    assert_eq!(diag.code, StableCode::engine(208));
}

#[test]
fn enforced_action_dependencies_are_reused() {
    let mut engine = Engine::new();
    let action_interface = interface(&[Type::Int], Type::Int);
    let root_interface = interface(&[], Type::Int);
    engine.register_action_rule(
        action_rule("double", action_interface.clone()),
        DoubleAction,
    );
    engine.register_rule(
        pure_rule("entry", root_interface.clone()),
        ActionDepRule {
            dependency: action_request("double", action_interface, [Value::Int(21)]),
        },
    );
    let executions = Arc::new(AtomicUsize::new(0));
    let executor = CountingExecutor {
        executions: executions.clone(),
    };
    let root_request = pure_request("entry", root_interface, []);

    let first_runtime_result = engine.run(&root_request, &TokioRuntime, &executor);
    assert!(matches!(&first_runtime_result, Ok(Ok(_))));
    let first_evaluation_result = first_runtime_result.unwrap();
    let first = first_evaluation_result.unwrap();

    let second_runtime_result = engine.run(&root_request, &TokioRuntime, &executor);
    assert!(matches!(&second_runtime_result, Ok(Ok(_))));
    let second_evaluation_result = second_runtime_result.unwrap();
    let second = second_evaluation_result.unwrap();

    assert_eq!(first.source, EvaluationSource::Computed);
    assert_eq!(second.source, EvaluationSource::Reused);
    assert_eq!(first.computation, second.computation);
    assert_eq!(executions.load(Ordering::Relaxed), 1);
}

#[test]
fn distinct_parents_share_an_enforced_action() {
    let mut engine = Engine::new();
    let action_interface = interface(&[Type::Int], Type::Int);
    let boolean_parent_interface = interface(&[Type::Bool], Type::Int);
    let text_parent_interface = interface(&[Type::Text], Type::Int);
    let dependency = action_request("double", action_interface.clone(), [Value::Int(21)]);
    engine.register_action_rule(action_rule("double", action_interface), DoubleAction);
    engine.register_rule(
        pure_rule("boolean parent", boolean_parent_interface.clone()),
        ActionDepRule {
            dependency: dependency.clone(),
        },
    );
    engine.register_rule(
        pure_rule("text parent", text_parent_interface.clone()),
        ActionDepRule { dependency },
    );
    let executions = Arc::new(AtomicUsize::new(0));
    let executor = CountingExecutor {
        executions: executions.clone(),
    };

    let boolean_runtime_result = engine.run(
        &pure_request(
            "boolean parent",
            boolean_parent_interface,
            [Value::Bool(true)],
        ),
        &TokioRuntime,
        &executor,
    );
    assert!(matches!(&boolean_runtime_result, Ok(Ok(_))));
    let boolean_evaluation_result = boolean_runtime_result.unwrap();
    let boolean_parent = boolean_evaluation_result.unwrap();

    let text_runtime_result = engine.run(
        &pure_request(
            "text parent",
            text_parent_interface,
            [Value::Text("input".into())],
        ),
        &TokioRuntime,
        &executor,
    );
    assert!(matches!(&text_runtime_result, Ok(Ok(_))));
    let text_evaluation_result = text_runtime_result.unwrap();
    let text_parent = text_evaluation_result.unwrap();

    let boolean_action = engine
        .query()
        .dependencies_of(boolean_parent.computation)
        .and_then(|dependencies| dependencies.first())
        .and_then(pith_engine::DependencyEdge::computation_id)
        .unwrap();
    let text_action = engine
        .query()
        .dependencies_of(text_parent.computation)
        .and_then(|dependencies| dependencies.first())
        .and_then(pith_engine::DependencyEdge::computation_id)
        .unwrap();

    assert_eq!(boolean_parent.source, EvaluationSource::Computed);
    assert_eq!(text_parent.source, EvaluationSource::Computed);
    assert_eq!(boolean_action, text_action);
    assert_eq!(executions.load(Ordering::Relaxed), 1);
}

#[test]
fn unverified_action_dependencies_are_not_reused() {
    let mut engine = Engine::new();
    let action_interface = interface(&[Type::Int], Type::Int);
    let root_interface = interface(&[], Type::Int);
    engine.register_action_rule(
        action_rule("double", action_interface.clone()),
        DoubleAction,
    );
    engine.register_rule(
        pure_rule("entry", root_interface.clone()),
        ActionDepRule {
            dependency: action_request("double", action_interface, [Value::Int(21)]),
        },
    );
    let executions = Arc::new(AtomicUsize::new(0));
    let executor = UnverifiedCountingExecutor {
        executions: executions.clone(),
    };
    let root_request = pure_request("entry", root_interface, []);

    let first_runtime_result = engine.run(&root_request, &TokioRuntime, &executor);
    assert!(matches!(&first_runtime_result, Ok(Ok(_))));
    let first_evaluation_result = first_runtime_result.unwrap();
    let first = first_evaluation_result.unwrap();

    let second_runtime_result = engine.run(&root_request, &TokioRuntime, &executor);
    assert!(matches!(&second_runtime_result, Ok(Ok(_))));
    let second_evaluation_result = second_runtime_result.unwrap();
    let second = second_evaluation_result.unwrap();

    assert_eq!(first.source, EvaluationSource::Computed);
    assert_eq!(second.source, EvaluationSource::Computed);
    assert_ne!(first.computation, second.computation);
    assert_eq!(executions.load(Ordering::Relaxed), 2);
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
