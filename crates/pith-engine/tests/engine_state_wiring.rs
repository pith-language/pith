//! Live-engine coverage for the publish and reuse wiring to
//! [`EngineStateStore`] (decision 0024). The conformance suite in
//! `engine_state.rs` exercises the store through the trait directly; these
//! tests exercise the live [`Engine`] publishing real computations and
//! revalidating durable reuse.

use pith_core::{
    Action, ActionComputationKey, ActionInput, ActionOutput, ActionSpec, CapabilityRequirement,
    Content, Int, Interface, OutputKind, PlatformRequirement, Pure, PureComputationKey, Request,
    Rule, RuleIdentity, RuleRevision, Type, Value,
};
use pith_diag::{PithResult, Span};
use pith_engine::state::{
    AttemptStatistics, CompletedAttempt, DurableActionProvenance, DurableAttempt, DurableAttemptId,
    DurableAttemptState, DurableComputation, DurableDependency, DurableProvenance,
    DurableReuseDecision, EncodedValue, EngineStateError, EngineStateReader, EngineStateStore,
    EngineStateVersions, InvalidationExplanation, MemoryEngineStateStore, StoppedAttempt,
};
use pith_engine::{
    AccessVerification, ActionAuthorization, ActionExecution, ActionInvocation, ActionRule,
    AllowAllActions, AttemptState, CapturedActionExecution, CapturedExecutionReport,
    CapturedOutput, CapturedOutputContent, ComputationKind, Engine, EvaluationSource,
    ExecutionPlatform, Executor, ExecutorIdentity, PureRule, PureRuleFrame, PureStep, Resumption,
    ReuseContext, ReuseDecision,
};
use pith_ids::ContentId;
use pith_store::{ContentStore, MemoryContentStore};
use std::sync::Arc;

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
use diagnostic::fixture_error as fixture_diag;
use runtime_support::runtime;

// ---------------------------------------------------------------------------
// Pure-rule fixtures
// ---------------------------------------------------------------------------

struct FailingRule;

impl PureRule for FailingRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(FailingFrame)
    }
}

struct FailingFrame;

impl PureRuleFrame for FailingFrame {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        Err(fixture_diag("fixture pure failure"))
    }
}

struct ForwardRule {
    dependency: Request<Pure>,
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
    dependency: Request<Pure>,
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

struct IncrementRule {
    dependency: Request<Pure>,
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
    dependency: Request<Pure>,
    requested: bool,
}

impl PureRuleFrame for IncrementFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::Need(self.dependency.clone()));
        }
        match input.and_then(Resumption::one) {
            Some(Value::Int(n)) => Ok(PureStep::Complete(Value::Int(n.added(&Int::from(1))))),
            Some(value) => Ok(PureStep::Complete(value)),
            None => Ok(PureStep::Complete(Value::Unit)),
        }
    }
}

// ---------------------------------------------------------------------------
// Action fixtures
// ---------------------------------------------------------------------------

struct BlobProducingAction;

impl ActionRule for BlobProducingAction {
    fn plan(&self, _inputs: &[Value]) -> PithResult<ActionSpec> {
        let mut spec = ActionSpec::isolated(action_executable());
        spec.inputs = [ActionInput {
            path: "operand".into(),
            content: Content::Blob(action_input()),
        }]
        .into();
        spec.capabilities = [action_capability()].into();
        spec.outputs = [ActionOutput {
            path: "result".into(),
            kind: OutputKind::Blob,
        }]
        .into();
        spec.platform = PlatformRequirement::Exact {
            operating_system: "fixture".into(),
            architecture: "fixture".into(),
        };
        Ok(spec)
    }

    fn complete(&self, _inputs: &[Value], execution: &ActionExecution) -> PithResult<Value> {
        match execution.report.outputs.first() {
            Some(output) => match &output.content {
                Content::Blob(id) => Ok(Value::Blob(*id)),
                Content::Tree(_) => Err(fixture_diag("action produced a tree, not a blob")),
            },
            None => Err(fixture_diag("action produced no output")),
        }
    }
}

struct FixtureExecutor;

#[async_trait::async_trait]
impl Executor for FixtureExecutor {
    fn identity(&self) -> ExecutorIdentity {
        ExecutorIdentity {
            executor: "fixture".into(),
            platform: fixture_platform(),
        }
    }

    async fn execute(&self, invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        let spec = &invocation.spec;
        let input = invocation
            .inputs
            .first()
            .ok_or_else(|| fixture_diag("action fixture requires one input"))?;
        match &input.content {
            pith_engine::MaterializedContent::Blob(blob)
                if blob.id == action_input() && blob.bytes.as_ref() == b"fixture input" => {}
            _ => {
                return Err(fixture_diag(
                    "action fixture received the wrong input content",
                ));
            }
        };
        let output = spec
            .outputs
            .first()
            .ok_or_else(|| fixture_diag("action fixture requires one declared output"))?;
        let identity = self.identity();
        Ok(CapturedActionExecution {
            report: CapturedExecutionReport {
                executor: identity.executor,
                platform: identity.platform,
                access: AccessVerification::Prevented,
                outputs: [CapturedOutput {
                    path: output.path.clone(),
                    content: CapturedOutputContent::Blob(b"42".to_vec().into_boxed_slice()),
                }]
                .into(),
                capabilities_used: [action_capability()].into(),
            },
            exit: None,
        })
    }
}

struct DenyCapability;

impl pith_engine::ActionPolicy for DenyCapability {
    fn authorize(&self, plan: &pith_engine::ActionPlan) -> ActionAuthorization {
        if plan.spec.capabilities.contains(&action_capability()) {
            return ActionAuthorization::Denied {
                policy: "deny-capability".into(),
                reason: "fixture capability is disabled".into(),
            };
        }
        ActionAuthorization::Allowed {
            policy: "deny-capability".into(),
        }
    }
}

fn action_executable() -> &'static str {
    "/bin/fixture-action"
}

fn action_input() -> ContentId {
    ContentId::of_blob(b"fixture input")
}

fn action_capability() -> CapabilityRequirement {
    CapabilityRequirement {
        name: "fixture.compute".into(),
        scope: "result".into(),
    }
}

fn fixture_platform() -> ExecutionPlatform {
    ExecutionPlatform {
        operating_system: "fixture".into(),
        architecture: "fixture".into(),
    }
}

// ---------------------------------------------------------------------------
// Shared constructors
// ---------------------------------------------------------------------------

fn interface(inputs: &[Type], output: Type) -> Interface {
    Interface {
        inputs: inputs.to_vec().into_boxed_slice(),
        output,
    }
}

fn pure_rule(label: &str, interface: Interface) -> Rule<Pure> {
    let identity = RuleIdentity::of_module_declaration("engine-state-wiring-tests", label);
    let revision = RuleRevision::of_manifest(identity, b"engine-state-wiring-tests-v1");
    Rule::<Pure>::new(
        "engine-state-wiring-tests",
        revision,
        label,
        interface,
        Span::none(),
    )
}

/// The same rule at a different revision, which is what decision 0023 has an
/// author bump when the rule's semantics change.
fn revised_pure_rule(label: &str, interface: Interface, revision: &[u8]) -> Rule<Pure> {
    let identity = RuleIdentity::of_module_declaration("engine-state-wiring-tests", label);
    Rule::<Pure>::new(
        "engine-state-wiring-tests",
        RuleRevision::of_manifest(identity, revision),
        label,
        interface,
        Span::none(),
    )
}

fn pure_request(
    label: &str,
    interface: Interface,
    inputs: impl Into<Box<[Value]>>,
) -> Request<Pure> {
    Request::<Pure>::new(label, interface, inputs, Span::none())
}

fn action_rule(label: &str, interface: Interface) -> Rule<Action> {
    let identity = RuleIdentity::of_module_declaration("engine-state-wiring-tests", label);
    let revision = RuleRevision::of_manifest(identity, b"engine-state-wiring-tests-v1");
    Rule::<Action>::new(
        "engine-state-wiring-tests",
        revision,
        label,
        interface,
        Span::none(),
    )
}

fn action_request(
    label: &str,
    interface: Interface,
    inputs: impl Into<Box<[Value]>>,
) -> Request<Action> {
    Request::<Action>::new(label, interface, inputs, Span::none())
}

fn engine_with_fixtures() -> Engine {
    engine_with_state(MemoryEngineStateStore::default())
}

/// An engine with the fixture content already stored, over an explicit
/// engine-state adapter. Hydration is only observable across engine instances,
/// so those tests build several engines over one shared adapter.
fn engine_with_state(state: impl EngineStateStore + 'static) -> Engine {
    let mut content = MemoryContentStore::default();
    // The executable is a host path (decision 0030); only the declared input
    // blob is stored.
    let input = put_fixture_blob(&mut content, b"fixture input");
    assert_eq!(input, action_input());
    Engine::with_state_store(content, state)
}

/// An engine over content *and* engine state that outlive it. Reusing an action
/// across instances needs both: the index says which attempt, and the content
/// store still has to hold what that attempt produced (decision 0031).
fn engine_with_shared_substrate(
    state: impl EngineStateStore + 'static,
    content: SharedContentStore,
) -> Engine {
    let input = content.put_fixture(b"fixture input");
    assert_eq!(input, action_input());
    Engine::with_state_store(content, state)
}

fn put_fixture_blob(store: &mut MemoryContentStore, bytes: &[u8]) -> ContentId {
    match store.put_blob(bytes) {
        Ok(identity) => identity,
        Err(error) => unreachable!("memory content store failed to store fixture blob: {error}"),
    }
}

/// The pure computation key an engine assigned to `computation`. Lets tests
/// correlate arena nodes with durable records without exposing the engine's
/// private durable side-table.
fn pure_key_of(engine: &Engine, computation: pith_ids::ComputationId) -> PureComputationKey {
    let node = engine
        .query()
        .computation(computation)
        .unwrap_or_else(|| unreachable!("computation {computation:?} is missing"));
    let ComputationKind::Pure(request) = &node.kind else {
        unreachable!("computation {computation:?} is not pure")
    };
    let rule = engine
        .query()
        .rule(node.rule)
        .unwrap_or_else(|| unreachable!("rule {:?} is missing", node.rule));
    PureComputationKey::new(rule, request)
}

fn durable_id(engine: &Engine, computation: pith_ids::ComputationId) -> DurableAttemptId {
    engine
        .durable_attempt_for(computation)
        .unwrap_or_else(|| unreachable!("no durable attempt published for {computation:?}"))
}

fn completed_record(
    store: &dyn EngineStateStore,
    attempt: DurableAttemptId,
) -> pith_engine::state::CompletedAttempt {
    let record = read_attempt(store, attempt);
    match &record.state {
        DurableAttemptState::Complete(completion) => completion.clone(),
        state => unreachable!("durable attempt {attempt} is {state:?}, not Complete"),
    }
}

fn failed_record(
    store: &dyn EngineStateStore,
    attempt: DurableAttemptId,
) -> pith_engine::state::StoppedAttempt {
    let record = read_attempt(store, attempt);
    match &record.state {
        DurableAttemptState::Failed(failure) => failure.clone(),
        state => unreachable!("durable attempt {attempt} is {state:?}, not Failed"),
    }
}

fn read_attempt(
    store: &dyn EngineStateStore,
    attempt: DurableAttemptId,
) -> std::sync::Arc<pith_engine::state::DurableAttempt> {
    match store.attempt(attempt) {
        Ok(Some(record)) => record,
        Ok(None) => unreachable!("durable attempt {attempt} was not published"),
        Err(error) => unreachable!("durable attempt {attempt} is unreadable: {error}"),
    }
}

fn sole_pure_computation(
    engine: &Engine,
) -> (pith_ids::ComputationId, &pith_engine::ComputationNode) {
    sole_computation(engine, ComputationKindMatcher::Pure)
}

fn sole_action_computation(
    engine: &Engine,
) -> (pith_ids::ComputationId, &pith_engine::ComputationNode) {
    sole_computation(engine, ComputationKindMatcher::Action)
}

#[derive(Debug)]
enum ComputationKindMatcher {
    Pure,
    Action,
}

fn sole_computation(
    engine: &Engine,
    matcher: ComputationKindMatcher,
) -> (pith_ids::ComputationId, &pith_engine::ComputationNode) {
    let matches: Vec<_> = engine
        .query()
        .computations()
        .filter(|(_, node)| match matcher {
            ComputationKindMatcher::Pure => matches!(node.kind, ComputationKind::Pure(_)),
            ComputationKindMatcher::Action => matches!(node.kind, ComputationKind::Action(_)),
        })
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one {matcher:?} computation, found {}",
        matches.len()
    );
    matches
        .into_iter()
        .next()
        .unwrap_or_else(|| unreachable!("matches was asserted to contain exactly one element"))
}

fn leaf_dependency_of(engine: &Engine, parent: pith_ids::ComputationId) -> pith_ids::ComputationId {
    engine
        .query()
        .dependencies_of(parent)
        .and_then(|deps| deps.first())
        .and_then(pith_engine::DependencyEdge::computation_id)
        .unwrap_or_else(|| unreachable!("parent {parent:?} has no pure dependency edge"))
}

/// Publish a completed, reusable attempt for `computation` directly through the
/// store, standing in for a recomputation performed by another engine instance.
fn publish_reusable_attempt(
    state: &SharedEngineStateStore,
    computation: PureComputationKey,
    result: &Value,
) -> DurableAttemptId {
    let attempt = match state.create_pending_attempt(DurableComputation::Pure(computation)) {
        Ok(attempt) => attempt,
        Err(error) => unreachable!("shared engine state rejected a pending attempt: {error}"),
    };
    let completion = CompletedAttempt {
        dependencies: Box::new([]),
        result: EncodedValue::from_value(result),
        provenance: DurableProvenance::Pure,
        reuse: DurableReuseDecision::Reusable,
        capabilities: Box::new([]),
    };
    if let Err(error) = state.publish_complete(attempt, completion) {
        unreachable!("shared engine state rejected attempt {attempt}: {error}");
    }
    attempt
}

fn attempt_history_len(state: &SharedEngineStateStore, computation: PureComputationKey) -> usize {
    match state.attempt_history(computation) {
        Ok(history) => history.len(),
        Err(error) => unreachable!("shared engine state history is unreadable: {error}"),
    }
}

// ---------------------------------------------------------------------------
// Publish path: pure computations
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Publish path: action computations
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Reuse path: durable revalidation (decision 0024)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Hydration: durable reuse across engine instances (decision 0024)
// ---------------------------------------------------------------------------

fn register_action_fixtures(
    engine: &mut Engine,
    action_interface: &Interface,
    root_interface: &Interface,
) {
    engine.register_action_rule(
        action_rule("produce", action_interface.clone()),
        BlobProducingAction,
    );
    engine.register_rule(
        pure_rule("entry", root_interface.clone()),
        ActionDepRule {
            dependency: action_request("produce", action_interface.clone(), []),
        },
    );
}

/// An executor that fails if it is called at all.
struct NeverRunsExecutor;

#[async_trait::async_trait]
impl Executor for NeverRunsExecutor {
    fn identity(&self) -> ExecutorIdentity {
        FixtureExecutor.identity()
    }

    async fn execute(&self, _invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        Err(fixture_diag("the executor ran when the result was cached"))
    }
}

#[path = "engine_state_wiring/stores.rs"]
mod stores;

use stores::{CreateFailingStore, ReadFailingStore, SharedContentStore, SharedEngineStateStore};

#[path = "engine_state_wiring/action_hydration.rs"]
mod action_hydration;
#[path = "engine_state_wiring/action_publication.rs"]
mod action_publication;
#[path = "engine_state_wiring/dependency_hydration.rs"]
mod dependency_hydration;
#[path = "engine_state_wiring/durable_reuse.rs"]
mod durable_reuse;
#[path = "engine_state_wiring/pure_publication.rs"]
mod pure_publication;
