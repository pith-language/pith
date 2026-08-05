//! Tests that the engine crosses the sync/async boundary: pure rules that
//! depend on content-addressed blobs and on action results (decisions 0021,
//! 0022).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use pith_core::{
    Action, ActionInput, ActionInputContent, ActionOutput, ActionOutputKind, ActionSpec,
    CapabilityRequirement, Interface, PlatformRequirement, Pure, Request, Rule, RuleIdentity,
    RuleRevision, Type, Value,
};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_engine::{
    AccessVerification, ActionAuthorization, ActionExecution, ActionInvocation, ActionPlan,
    ActionPolicy, ActionRule, AllowAllActions, CapturedActionExecution, CapturedExecutionReport,
    CapturedOutput, CapturedOutputContent, ComputationKind, Engine, EvaluationSource,
    ExecutionPlatform, Executor, MaterializedContent, PureRule, PureRuleFrame, PureStep,
    ReuseDecision, ReuseReason, TokioRuntime,
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
        match input {
            Some(value) => Ok(PureStep::Complete(value)),
            None => Err(fixture_error("action dependency completed without a value")),
        }
    }
}

struct DoubleAction;

impl ActionRule for DoubleAction {
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec> {
        let n = match inputs.first() {
            Some(Value::Int(n)) => *n,
            _ => 0,
        };
        let mut spec = ActionSpec::isolated(double_executable());
        spec.arguments = [n.to_string().into_boxed_str()].into();
        spec.platform = PlatformRequirement::Exact {
            operating_system: "fixture".into(),
            architecture: "fixture".into(),
        };
        spec.inputs = [ActionInput {
            path: "operand".into(),
            content: ActionInputContent::Blob(double_input()),
        }]
        .into();
        spec.capabilities = declared_double_capabilities().into();
        spec.outputs = [ActionOutput {
            path: "result".into(),
            kind: ActionOutputKind::Blob,
        }]
        .into();
        Ok(spec)
    }

    fn complete(&self, _inputs: &[Value], execution: &ActionExecution) -> PithResult<Value> {
        match execution.report.outputs.first() {
            Some(output) => Ok(Value::Blob(output.content)),
            None => Err(fixture_error("double action produced no output")),
        }
    }
}

struct WrongTypeAction;

impl ActionRule for WrongTypeAction {
    fn plan(&self, _inputs: &[Value]) -> PithResult<ActionSpec> {
        let mut spec = ActionSpec::isolated(wrong_type_executable());
        spec.outputs = [ActionOutput {
            path: "result".into(),
            kind: ActionOutputKind::Blob,
        }]
        .into();
        Ok(spec)
    }

    fn complete(&self, _inputs: &[Value], _execution: &ActionExecution) -> PithResult<Value> {
        Ok(Value::Int(0))
    }
}

struct FixtureExecutor;

#[async_trait::async_trait]
impl Executor for FixtureExecutor {
    async fn execute(&self, invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        let spec = &invocation.spec;
        let content = if spec.executable == double_executable() {
            let Some(input) = invocation.inputs.first() else {
                return Err(fixture_error("double action fixture requires one input"));
            };
            if invocation.inputs.len() != 1 || input.path.as_ref() != "operand" {
                return Err(fixture_error(
                    "double action fixture received undeclared inputs",
                ));
            }
            match &input.content {
                MaterializedContent::Blob { id, bytes }
                    if *id == double_input() && bytes.as_ref() == b"fixture input" => {}
                _ => {
                    return Err(fixture_error(
                        "double action fixture received the wrong input content",
                    ));
                }
            }
            let Some(argument) = spec.arguments.first() else {
                return Err(fixture_error("double action fixture requires one argument"));
            };
            let Ok(number) = argument.parse::<i64>() else {
                return Err(fixture_error(
                    "double action fixture requires an integer argument",
                ));
            };
            CapturedOutputContent::Blob(double_result_bytes(number))
        } else {
            CapturedOutputContent::Blob(b"wrong type result".as_slice().into())
        };
        let Some(output) = spec.outputs.first() else {
            return Err(fixture_error("fixture action requires one declared output"));
        };
        Ok(CapturedActionExecution {
            report: CapturedExecutionReport {
                executor: "fixture".into(),
                platform: fixture_platform(),
                access: AccessVerification::Prevented,
                outputs: [CapturedOutput {
                    path: output.path.clone(),
                    kind: output.kind,
                    content,
                }]
                .into(),
                capabilities_used: Box::new([]),
            },
        })
    }
}

struct UndeclaredCapabilityExecutor;

struct ObservedCapabilityExecutor;

struct WrongPlatformExecutor;

struct DenyDoubleCapability;

impl ActionPolicy for DenyDoubleCapability {
    fn authorize(&self, plan: &ActionPlan) -> ActionAuthorization {
        if plan.spec.capabilities.contains(&double_capability()) {
            return ActionAuthorization::Denied {
                policy: "deny-double-capability".into(),
                reason: "fixture compute access is disabled".into(),
            };
        }
        ActionAuthorization::Allowed {
            policy: "deny-double-capability".into(),
        }
    }
}

#[async_trait::async_trait]
impl Executor for UndeclaredCapabilityExecutor {
    async fn execute(&self, _invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        Ok(CapturedActionExecution {
            report: CapturedExecutionReport {
                executor: "fixture".into(),
                platform: fixture_platform(),
                access: AccessVerification::Observed,
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

#[async_trait::async_trait]
impl Executor for ObservedCapabilityExecutor {
    async fn execute(&self, invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        let mut execution = FixtureExecutor.execute(invocation).await?;
        execution.report.access = AccessVerification::Observed;
        execution.report.capabilities_used = [double_capability()].into();
        Ok(execution)
    }
}

#[async_trait::async_trait]
impl Executor for WrongPlatformExecutor {
    async fn execute(&self, invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        let mut execution = FixtureExecutor.execute(invocation).await?;
        execution.report.platform.operating_system = "other".into();
        Ok(execution)
    }
}

struct CountingExecutor {
    executions: Arc<AtomicUsize>,
}

struct UnverifiedCountingExecutor {
    executions: Arc<AtomicUsize>,
}

struct NeverExecutor {
    executions: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl Executor for UnverifiedCountingExecutor {
    async fn execute(&self, invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        self.executions.fetch_add(1, Ordering::Relaxed);
        let mut execution = FixtureExecutor.execute(invocation).await?;
        execution.report.access = AccessVerification::Unverified;
        Ok(execution)
    }
}

#[async_trait::async_trait]
impl Executor for CountingExecutor {
    async fn execute(&self, invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        self.executions.fetch_add(1, Ordering::Relaxed);
        FixtureExecutor.execute(invocation).await
    }
}

#[async_trait::async_trait]
impl Executor for NeverExecutor {
    async fn execute(&self, _invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        self.executions.fetch_add(1, Ordering::Relaxed);
        Err(fixture_error("executor should not be called"))
    }
}

fn double_executable() -> ContentId {
    ContentId::of_blob(b"fixture:double")
}

fn double_input() -> ContentId {
    ContentId::of_blob(b"fixture input")
}

fn wrong_type_executable() -> ContentId {
    ContentId::of_blob(b"fixture:wrong-type")
}

fn double_result_bytes(number: i64) -> Box<[u8]> {
    number
        .saturating_mul(2)
        .to_string()
        .into_bytes()
        .into_boxed_slice()
}

fn double_result(number: i64) -> ContentId {
    ContentId::of_blob(&double_result_bytes(number))
}

fn double_capability() -> CapabilityRequirement {
    CapabilityRequirement {
        name: "fixture.compute".into(),
        scope: "double".into(),
    }
}

fn double_audit_capability() -> CapabilityRequirement {
    CapabilityRequirement {
        name: "fixture.audit".into(),
        scope: "result".into(),
    }
}

fn declared_double_capabilities() -> [CapabilityRequirement; 2] {
    [double_capability(), double_audit_capability()]
}

fn effective_double_capabilities() -> [CapabilityRequirement; 2] {
    [double_audit_capability(), double_capability()]
}

fn fixture_platform() -> ExecutionPlatform {
    ExecutionPlatform {
        operating_system: "fixture".into(),
        architecture: "fixture".into(),
    }
}

fn fixture_error(message: &str) -> DiagnosticSink {
    let mut diagnostics = DiagnosticSink::new();
    diagnostics.push(Diag::new(
        Severity::Error,
        StableCode::engine(211),
        Span::none(),
        message,
    ));
    diagnostics
}

fn fixture_engine() -> Engine {
    let mut engine = Engine::new();
    assert_eq!(
        put_fixture_blob(&mut engine, b"fixture:double"),
        double_executable()
    );
    assert_eq!(
        put_fixture_blob(&mut engine, b"fixture:wrong-type"),
        wrong_type_executable()
    );
    assert_eq!(
        put_fixture_blob(&mut engine, b"fixture input"),
        double_input()
    );
    engine
}

fn put_fixture_blob(engine: &mut Engine, bytes: &[u8]) -> ContentId {
    match engine.put_blob(bytes) {
        Ok(identity) => identity,
        Err(error) => unreachable!("memory content store failed to store fixture blob: {error}"),
    }
}

fn interface(inputs: &[Type], output: Type) -> Interface {
    Interface {
        inputs: inputs.to_vec().into_boxed_slice(),
        output,
    }
}

fn pure_rule(label: &str, interface: Interface) -> Rule<Pure> {
    let identity = RuleIdentity::of_module_declaration("action-and-blob-tests", label);
    let revision = RuleRevision::of_manifest(identity, b"action-and-blob-tests-v1");
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
    let identity = RuleIdentity::of_module_declaration("action-and-blob-tests", label);
    let revision = RuleRevision::of_manifest(identity, b"action-and-blob-tests-v1");
    Rule::<Action>::new(revision, label, interface, Span::none())
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
    let mut engine = fixture_engine();
    let blob_id = engine.put_blob(b"hello").unwrap();
    engine.register_rule(
        pure_rule("length", interface(&[], Type::Int)),
        BlobLenRule { blob: blob_id },
    );

    let evaluation = engine
        .run(
            &pure_request("length", interface(&[], Type::Int), []),
            &TokioRuntime,
            &AllowAllActions,
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
    let mut engine = fixture_engine();
    let absent = ContentId::of_blob(b"not stored");
    engine.register_rule(
        pure_rule("length", interface(&[], Type::Int)),
        BlobLenRule { blob: absent },
    );

    let result = engine
        .run(
            &pure_request("length", interface(&[], Type::Int), []),
            &TokioRuntime,
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap();

    let err = result.unwrap_err();
    let diag = err.iter().next().unwrap();
    assert_eq!(diag.code, StableCode::engine(205));
}

#[test]
fn action_dependency_driven_through_run() {
    let mut engine = fixture_engine();
    let action_iface = interface(&[Type::Int], Type::Blob);
    let pure_iface = interface(&[], Type::Blob);
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
            interface(&[Type::Int], Type::Blob),
            [Value::Int(21)],
        ))
        .unwrap();
    assert_eq!(plan.spec.executable, double_executable());
    assert_eq!(plan.spec_digest, plan.spec.digest().unwrap());
    assert_eq!(
        plan.spec.capabilities.as_ref(),
        declared_double_capabilities().as_slice()
    );
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
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap()
        .unwrap();

    assert_eq!(evaluation.value, Value::Blob(double_result(21)));
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
        action.authorization,
        ActionAuthorization::Allowed {
            policy: "allow-all-actions".into(),
        }
    );
    assert_eq!(
        action.report.as_ref().map(|report| report.access),
        Some(AccessVerification::Prevented)
    );
    assert_eq!(
        engine.query().capabilities_of(action_computation),
        Some(effective_double_capabilities().as_slice())
    );
    assert_eq!(
        engine.query().capabilities_of(evaluation.computation),
        Some(effective_double_capabilities().as_slice())
    );
}

#[test]
fn actual_capability_uses_are_dependency_edges() {
    let mut engine = fixture_engine();
    let action_iface = interface(&[Type::Int], Type::Blob);
    let pure_iface = interface(&[], Type::Blob);
    engine.register_action_rule(action_rule("double", action_iface.clone()), DoubleAction);
    engine.register_rule(
        pure_rule("entry", pure_iface.clone()),
        ActionDepRule {
            dependency: action_request("double", action_iface, [Value::Int(21)]),
        },
    );

    let evaluation = match engine.run(
        &pure_request("entry", pure_iface, []),
        &TokioRuntime,
        &AllowAllActions,
        &ObservedCapabilityExecutor,
    ) {
        Ok(Ok(evaluation)) => evaluation,
        Ok(Err(_)) => unreachable!("observed capability fixture failed evaluation"),
        Err(_) => unreachable!("observed capability fixture failed to drive runtime"),
    };

    let query = engine.query();
    let Some(action_computation) = query
        .dependencies_of(evaluation.computation)
        .and_then(|dependencies| dependencies.first())
        .and_then(pith_engine::DependencyEdge::computation_id)
    else {
        unreachable!("pure evaluation has no action dependency");
    };
    let Some(uses) = query.capability_uses_of(action_computation) else {
        unreachable!("action computation is missing");
    };
    let uses: Vec<_> = uses.cloned().collect();
    let Some(mut parent_uses) = query.capability_uses_of(evaluation.computation) else {
        unreachable!("pure computation is missing");
    };

    assert_eq!(uses, [double_capability()]);
    assert!(parent_uses.next().is_none());
    assert_eq!(
        query.capabilities_of(evaluation.computation),
        Some(effective_double_capabilities().as_slice())
    );
}

#[test]
fn action_output_bytes_are_imported_by_the_engine() {
    let mut engine = fixture_engine();
    let action_iface = interface(&[Type::Int], Type::Blob);
    let pure_iface = interface(&[], Type::Blob);
    engine.register_action_rule(action_rule("double", action_iface.clone()), DoubleAction);
    engine.register_rule(
        pure_rule("entry", pure_iface.clone()),
        ActionDepRule {
            dependency: action_request("double", action_iface, [Value::Int(21)]),
        },
    );

    let action_evaluation = engine
        .run(
            &pure_request("entry", pure_iface, []),
            &TokioRuntime,
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap()
        .unwrap();
    let Value::Blob(output) = action_evaluation.value else {
        unreachable!("fixture action returns a blob")
    };

    engine.register_rule(
        pure_rule("length", interface(&[], Type::Int)),
        BlobLenRule { blob: output },
    );
    let bytes_evaluation = engine
        .run(
            &pure_request("length", interface(&[], Type::Int), []),
            &TokioRuntime,
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap()
        .unwrap();

    assert_eq!(output, double_result(21));
    assert_eq!(bytes_evaluation.value, Value::Int(2));
}

#[test]
fn missing_action_input_is_rejected_before_executor_call() {
    let mut engine = Engine::new();
    assert_eq!(
        put_fixture_blob(&mut engine, b"fixture:double"),
        double_executable()
    );
    let action_interface = interface(&[Type::Int], Type::Blob);
    let root_interface = interface(&[], Type::Blob);
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
    let executor = NeverExecutor {
        executions: executions.clone(),
    };

    let diagnostics = engine
        .run(
            &pure_request("entry", root_interface, []),
            &TokioRuntime,
            &AllowAllActions,
            &executor,
        )
        .unwrap()
        .unwrap_err();
    let diagnostic = diagnostics.iter().next().unwrap();

    assert_eq!(diagnostic.code, StableCode::engine(205));
    assert_eq!(executions.load(Ordering::Relaxed), 0);
}

#[test]
fn action_result_type_checked_against_interface() {
    let mut engine = fixture_engine();
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
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap();

    let err = result.unwrap_err();
    let diag = err.iter().next().unwrap();
    assert_eq!(diag.code, StableCode::engine(104));
}

#[test]
fn undeclared_capability_use_is_rejected() {
    let mut engine = fixture_engine();
    let action_iface = interface(&[Type::Int], Type::Blob);
    let pure_iface = interface(&[], Type::Blob);
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
            &AllowAllActions,
            &UndeclaredCapabilityExecutor,
        )
        .unwrap();

    let err = result.unwrap_err();
    let diag = err.iter().next().unwrap();
    assert_eq!(diag.code, StableCode::engine(208));

    let query = engine.query();
    let Some((computation, node)) = query.computations().find(|(_, node)| node.action.is_some())
    else {
        unreachable!("rejected action has no computation");
    };
    let Some(uses) = query.capability_uses_of(computation) else {
        unreachable!("rejected action computation is missing");
    };
    let uses: Vec<_> = uses.cloned().collect();
    let reported = node
        .action
        .as_ref()
        .and_then(|action| action.report.as_ref())
        .map(|report| report.capabilities_used.as_ref());

    assert_eq!(uses.len(), 1);
    assert_eq!(
        uses.first().map(|use_| use_.name.as_ref()),
        Some("fixture.clock")
    );
    assert_eq!(reported, Some(uses.as_slice()));
}

#[test]
fn policy_denial_is_recorded_before_execution() {
    let mut engine = fixture_engine();
    let action_interface = interface(&[Type::Int], Type::Blob);
    let root_interface = interface(&[], Type::Blob);
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

    let runtime_result = engine
        .run(
            &pure_request("entry", root_interface, []),
            &TokioRuntime,
            &DenyDoubleCapability,
            &executor,
        )
        .unwrap();

    let diagnostics = runtime_result.unwrap_err();
    let diagnostic = diagnostics.iter().next().unwrap();
    assert_eq!(diagnostic.code, StableCode::engine(213));
    assert_eq!(executions.load(Ordering::Relaxed), 0);

    let query = engine.query();
    let (denied_computation, denied_node, denied_action) = query
        .computations()
        .find_map(|(computation, node)| {
            node.action
                .as_ref()
                .map(|action| (computation, node, action))
        })
        .unwrap();
    assert_eq!(
        denied_action.authorization,
        ActionAuthorization::Denied {
            policy: "deny-double-capability".into(),
            reason: "fixture compute access is disabled".into(),
        }
    );
    assert!(denied_action.report.is_none());
    assert_eq!(
        denied_node.reuse,
        ReuseDecision::NotReusable(ReuseReason::PolicyDenied)
    );
    let denied_parent = query
        .computations()
        .find(|(_, node)| matches!(node.kind, ComputationKind::Pure(_)))
        .map(|(_, node)| node)
        .unwrap();
    assert!(denied_parent.dependencies.iter().any(|dependency| {
        matches!(
            dependency,
            pith_engine::DependencyEdge::Action { computation, .. }
                if *computation == denied_computation
        )
    }));
}

#[test]
fn executor_must_report_the_planned_platform() {
    let mut engine = fixture_engine();
    let action_interface = interface(&[Type::Int], Type::Blob);
    let root_interface = interface(&[], Type::Blob);
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

    let runtime_result = engine
        .run(
            &pure_request("entry", root_interface, []),
            &TokioRuntime,
            &AllowAllActions,
            &WrongPlatformExecutor,
        )
        .unwrap();

    let diagnostics = runtime_result.unwrap_err();
    let diagnostic = diagnostics.iter().next().unwrap();
    assert_eq!(diagnostic.code, StableCode::engine(212));
    let query = engine.query();
    let (failed_computation, failed_action) = query
        .computations()
        .find_map(|(computation, node)| node.action.as_ref().map(|action| (computation, action)))
        .unwrap();
    assert_eq!(
        failed_action
            .report
            .as_ref()
            .map(|report| report.platform.operating_system.as_ref()),
        Some("other")
    );
    let failed_parent = query
        .computations()
        .find(|(_, node)| matches!(node.kind, ComputationKind::Pure(_)))
        .map(|(_, node)| node)
        .unwrap();
    assert!(failed_parent.dependencies.iter().any(|dependency| {
        matches!(
            dependency,
            pith_engine::DependencyEdge::Action { computation, .. }
                if *computation == failed_computation
        )
    }));
}

#[test]
fn action_dependencies_are_not_reused_without_a_cache_identity() {
    let mut engine = fixture_engine();
    let action_interface = interface(&[Type::Int], Type::Blob);
    let root_interface = interface(&[], Type::Blob);
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

    let first_runtime_result =
        engine.run(&root_request, &TokioRuntime, &AllowAllActions, &executor);
    assert!(matches!(&first_runtime_result, Ok(Ok(_))));
    let first_evaluation_result = first_runtime_result.unwrap();
    let first = first_evaluation_result.unwrap();

    let second_runtime_result =
        engine.run(&root_request, &TokioRuntime, &AllowAllActions, &executor);
    assert!(matches!(&second_runtime_result, Ok(Ok(_))));
    let second_evaluation_result = second_runtime_result.unwrap();
    let second = second_evaluation_result.unwrap();

    assert_eq!(first.source, EvaluationSource::Computed);
    assert_eq!(second.source, EvaluationSource::Computed);
    assert_ne!(first.computation, second.computation);
    assert_eq!(executions.load(Ordering::Relaxed), 2);

    let action_computation = engine
        .query()
        .dependencies_of(first.computation)
        .and_then(|dependencies| dependencies.first())
        .and_then(pith_engine::DependencyEdge::computation_id)
        .unwrap();
    let action_reuse = &engine
        .query()
        .computation(action_computation)
        .unwrap()
        .reuse;
    assert_eq!(
        action_reuse,
        &ReuseDecision::NotReusable(ReuseReason::ActionCachingDisabled)
    );
    assert_eq!(
        &engine.query().computation(first.computation).unwrap().reuse,
        &ReuseDecision::NotReusable(ReuseReason::DependencyNotReusable {
            computation: action_computation,
        })
    );
}

#[test]
fn distinct_parents_do_not_share_action_results() {
    let mut engine = fixture_engine();
    let action_interface = interface(&[Type::Int], Type::Blob);
    let boolean_parent_interface = interface(&[Type::Bool], Type::Blob);
    let text_parent_interface = interface(&[Type::Text], Type::Blob);
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
        &AllowAllActions,
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
        &AllowAllActions,
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
    assert_ne!(boolean_action, text_action);
    assert_eq!(executions.load(Ordering::Relaxed), 2);
}

#[test]
fn unverified_action_dependencies_are_not_reused() {
    let mut engine = fixture_engine();
    let action_interface = interface(&[Type::Int], Type::Blob);
    let root_interface = interface(&[], Type::Blob);
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

    let first_runtime_result =
        engine.run(&root_request, &TokioRuntime, &AllowAllActions, &executor);
    assert!(matches!(&first_runtime_result, Ok(Ok(_))));
    let first_evaluation_result = first_runtime_result.unwrap();
    let first = first_evaluation_result.unwrap();

    let second_runtime_result =
        engine.run(&root_request, &TokioRuntime, &AllowAllActions, &executor);
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
    let mut engine = fixture_engine();
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
