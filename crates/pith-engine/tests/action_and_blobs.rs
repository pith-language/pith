//! Tests that the engine crosses the sync/async boundary: pure rules that
//! depend on content-addressed blobs and on action results (decisions 0021,
//! 0022).

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use pith_core::{
    Action, ActionInput, ActionOutput, ActionSpec, CapabilityRequirement, Content, Interface,
    OutputKind, PlatformRequirement, Pure, Request, Rule, RuleIdentity, RuleRevision, Type, Value,
};
use pith_diag::{EngineCode, PithResult, Span, StableCode};
use pith_engine::{
    AccessVerification, ActionAuthorization, ActionExecution, ActionInvocation, ActionPlan,
    ActionPolicy, ActionRule, AllowAllActions, AttemptState, CapturedActionExecution,
    CapturedExecutionReport, CapturedOutput, CapturedOutputContent, ComputationKind, Engine,
    EvaluationSource, ExecutionPlatform, Executor, ExecutorIdentity, MaterializedContent, PureRule,
    PureRuleFrame, PureStep, Resumption, ReuseDecision, ReuseReason,
};
use pith_ids::ContentId;
use pith_store::{Blob, ContentStore, MemoryContentStore, StoreError, Tree};

#[path = "support/action_dependency.rs"]
mod action_dependency_support;
#[path = "support/diagnostic.rs"]
mod diagnostic;
#[path = "support/runtime.rs"]
mod runtime_support;

use action_dependency_support::ActionDepRule;
use diagnostic::fixture_error;
use runtime_support::runtime;

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
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::NeedBlob(self.blob));
        }
        let len = match _input.and_then(Resumption::one) {
            Some(Value::Bytes(b)) => b.len() as i64,
            _ => 0,
        };
        Ok(PureStep::Complete(Value::Int(len)))
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
            content: Content::Blob(double_input()),
        }]
        .into();
        spec.capabilities = declared_double_capabilities().into();
        spec.outputs = [ActionOutput {
            path: "result".into(),
            kind: OutputKind::Blob,
        }]
        .into();
        Ok(spec)
    }

    fn complete(&self, _inputs: &[Value], execution: &ActionExecution) -> PithResult<Value> {
        match execution.report.outputs.first() {
            Some(output) => match &output.content {
                Content::Blob(id) => Ok(Value::Blob(*id)),
                Content::Tree(_) => Err(fixture_error("double action produced a tree, not a blob")),
            },
            None => Err(fixture_error("double action produced no output")),
        }
    }
}

/// `DoubleAction` with its planned argument taken from state the rule holds.
/// Delegating keeps one description of the contract, so the two rules differ
/// only in where the argument comes from.
struct HeldStateAction {
    argument: Arc<AtomicI64>,
}

impl ActionRule for HeldStateAction {
    fn plan(&self, _inputs: &[Value]) -> PithResult<ActionSpec> {
        DoubleAction.plan(&[Value::Int(self.argument.load(Ordering::Relaxed))])
    }

    fn complete(&self, inputs: &[Value], execution: &ActionExecution) -> PithResult<Value> {
        DoubleAction.complete(inputs, execution)
    }
}

struct WrongTypeAction;

impl ActionRule for WrongTypeAction {
    fn plan(&self, _inputs: &[Value]) -> PithResult<ActionSpec> {
        let mut spec = ActionSpec::isolated(wrong_type_executable());
        spec.outputs = [ActionOutput {
            path: "result".into(),
            kind: OutputKind::Blob,
        }]
        .into();
        Ok(spec)
    }

    fn complete(&self, _inputs: &[Value], _execution: &ActionExecution) -> PithResult<Value> {
        Ok(Value::Int(0))
    }
}

struct FailingPlanner;

impl ActionRule for FailingPlanner {
    fn plan(&self, _inputs: &[Value]) -> PithResult<ActionSpec> {
        Err(fixture_error("action planning failed"))
    }

    fn complete(&self, _inputs: &[Value], _execution: &ActionExecution) -> PithResult<Value> {
        Err(fixture_error("unplanned action cannot complete"))
    }
}

struct FailingCompletionAction;

impl ActionRule for FailingCompletionAction {
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec> {
        DoubleAction.plan(inputs)
    }

    fn complete(&self, _inputs: &[Value], _execution: &ActionExecution) -> PithResult<Value> {
        Err(fixture_error("action completion failed"))
    }
}

struct FixtureExecutor;

fn fixture_identity() -> ExecutorIdentity {
    ExecutorIdentity {
        executor: "fixture".into(),
        platform: fixture_platform(),
    }
}

#[async_trait::async_trait]
impl Executor for FixtureExecutor {
    fn identity(&self) -> ExecutorIdentity {
        fixture_identity()
    }

    async fn execute(&self, invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        let spec = &invocation.spec;
        let content = if spec.executable.host_path() == Some(double_executable()) {
            let Some(input) = invocation.inputs.first() else {
                return Err(fixture_error("double action fixture requires one input"));
            };
            if invocation.inputs.len() != 1 || input.path.as_ref() != "operand" {
                return Err(fixture_error(
                    "double action fixture received undeclared inputs",
                ));
            }
            match &input.content {
                MaterializedContent::Blob(blob)
                    if blob.id == double_input() && blob.bytes.as_ref() == b"fixture input" => {}
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
        let identity = self.identity();
        Ok(CapturedActionExecution {
            report: CapturedExecutionReport {
                executor: identity.executor,
                platform: identity.platform,
                access: AccessVerification::Prevented,
                outputs: [CapturedOutput {
                    path: output.path.clone(),
                    content,
                }]
                .into(),
                capabilities_used: Box::new([]),
            },
            exit: None,
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
    fn identity(&self) -> ExecutorIdentity {
        fixture_identity()
    }

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
            exit: None,
        })
    }
}

#[async_trait::async_trait]
impl Executor for ObservedCapabilityExecutor {
    fn identity(&self) -> ExecutorIdentity {
        fixture_identity()
    }

    async fn execute(&self, invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        let mut execution = FixtureExecutor.execute(invocation).await?;
        execution.report.access = AccessVerification::Observed;
        execution.report.capabilities_used = [double_capability()].into();
        Ok(execution)
    }
}

#[async_trait::async_trait]
impl Executor for WrongPlatformExecutor {
    fn identity(&self) -> ExecutorIdentity {
        let mut identity = fixture_identity();
        identity.platform.operating_system = "other".into();
        identity
    }

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

struct FailingExecutor;

struct RejectOutputImportsStore {
    content: MemoryContentStore,
}

impl ContentStore for RejectOutputImportsStore {
    fn put_blob(&mut self, _bytes: &[u8]) -> Result<ContentId, StoreError> {
        Err(StoreError::new("fixture rejected output import"))
    }

    fn get_blob(&self, id: ContentId) -> Result<Option<Blob>, StoreError> {
        self.content.get_blob(id)
    }

    fn put_tree(&mut self, _tree: Tree) -> Result<ContentId, StoreError> {
        Err(StoreError::new("fixture rejected output import"))
    }

    fn get_tree(&self, id: ContentId) -> Result<Option<Tree>, StoreError> {
        self.content.get_tree(id)
    }
}

#[async_trait::async_trait]
impl Executor for UnverifiedCountingExecutor {
    fn identity(&self) -> ExecutorIdentity {
        fixture_identity()
    }

    async fn execute(&self, invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        self.executions.fetch_add(1, Ordering::Relaxed);
        let mut execution = FixtureExecutor.execute(invocation).await?;
        execution.report.access = AccessVerification::Unverified;
        Ok(execution)
    }
}

#[async_trait::async_trait]
impl Executor for CountingExecutor {
    fn identity(&self) -> ExecutorIdentity {
        fixture_identity()
    }

    async fn execute(&self, invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        self.executions.fetch_add(1, Ordering::Relaxed);
        FixtureExecutor.execute(invocation).await
    }
}

#[async_trait::async_trait]
impl Executor for NeverExecutor {
    fn identity(&self) -> ExecutorIdentity {
        fixture_identity()
    }

    async fn execute(&self, _invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        self.executions.fetch_add(1, Ordering::Relaxed);
        Err(fixture_error("executor should not be called"))
    }
}

#[async_trait::async_trait]
impl Executor for FailingExecutor {
    fn identity(&self) -> ExecutorIdentity {
        fixture_identity()
    }

    async fn execute(&self, _invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        Err(fixture_error("executor failed"))
    }
}

fn double_executable() -> &'static str {
    "/bin/fixture-double"
}

fn double_input() -> ContentId {
    ContentId::of_blob(b"fixture input")
}

fn wrong_type_executable() -> &'static str {
    "/bin/fixture-wrong-type"
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

/// A synthetic executor-emitted diagnostic. The code is deliberately not a
/// named `EngineCode`: it stands in for an opaque code supplied by an executor
/// adapter, which the engine must pass through unchanged.
fn fixture_engine() -> Engine {
    let mut engine = Engine::with_content_store(MemoryContentStore::default());
    // The executable is a host path now (decision 0030), so only the declared
    // input blob needs to be in the store for the fixture action to materialize.
    assert_eq!(
        put_fixture_blob(&mut engine, b"fixture input"),
        double_input()
    );
    engine
}

fn import_failing_engine() -> Engine {
    let mut content = MemoryContentStore::default();
    let input = match content.put_blob(b"fixture input") {
        Ok(identity) => identity,
        Err(error) => unreachable!("memory content store failed to store the input: {error}"),
    };
    assert_eq!(input, double_input());
    Engine::with_content_store(RejectOutputImportsStore { content })
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

fn assert_no_pending_attempts(engine: &Engine) {
    assert!(
        engine
            .query()
            .computations()
            .all(|(_, node)| !matches!(node.state, AttemptState::Pending))
    );
}

#[path = "action_and_blobs/action_cases.rs"]
mod action_cases;
#[path = "action_and_blobs/blob_cases.rs"]
mod blob_cases;
#[path = "action_and_blobs/failure_cases.rs"]
mod failure_cases;
#[path = "action_and_blobs/reuse_cases.rs"]
mod reuse_cases;
