//! Live-engine coverage for the publish and reuse wiring to
//! [`EngineStateStore`] (decision 0024). The conformance suite in
//! `engine_state.rs` exercises the store through the trait directly; these
//! tests exercise the live [`Engine`] publishing real computations and
//! revalidating durable reuse.

use pith_core::{
    Action, ActionComputationKey, ActionInput, ActionOutput, ActionSpec, CapabilityRequirement,
    Content, Interface, OutputKind, PlatformRequirement, Pure, PureComputationKey, Request, Rule,
    RuleIdentity, RuleRevision, Type, Value,
};
use pith_diag::{DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_engine::state::{
    CompletedAttempt, DurableActionProvenance, DurableAttempt, DurableAttemptId,
    DurableAttemptState, DurableComputation, DurableDependency, DurableProvenance,
    DurableReuseDecision, EncodedValue, EngineStateError, EngineStateStore, EngineStateVersions,
    InvalidationExplanation, MemoryEngineStateStore, StoppedAttempt,
};
use pith_engine::{
    AccessVerification, ActionAuthorization, ActionExecution, ActionInvocation, ActionRule,
    AllowAllActions, AttemptState, CapturedActionExecution, CapturedExecutionReport,
    CapturedOutput, CapturedOutputContent, ComputationKind, Engine, EvaluationSource,
    ExecutionPlatform, Executor, ExecutorIdentity, PureRule, PureRuleFrame, PureStep, Resumption,
    ReuseContext, ReuseDecision, TokioRuntime,
};
use pith_ids::ContentId;
use pith_store::{ContentStore, MemoryContentStore};
use std::sync::Arc;

/// A runtime for one test. Built per call: constructing a thread pool is
/// cheap next to what these tests do, and it keeps each test independent.
fn runtime() -> TokioRuntime {
    match TokioRuntime::new() {
        Ok(runtime) => runtime,
        Err(error) => unreachable!("could not build a tokio runtime: {error:?}"),
    }
}

// ---------------------------------------------------------------------------
// Pure-rule fixtures
// ---------------------------------------------------------------------------

struct ConstantRule(Value);

impl PureRule for ConstantRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(ConstantFrame(self.0.clone()))
    }
}

struct ConstantFrame(Value);

impl PureRuleFrame for ConstantFrame {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        Ok(PureStep::Complete(self.0.clone()))
    }
}

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
            Some(Value::Int(n)) => Ok(PureStep::Complete(Value::Int(n.saturating_add(1)))),
            Some(value) => Ok(PureStep::Complete(value)),
            None => Ok(PureStep::Complete(Value::Unit)),
        }
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
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::NeedAction(self.dependency.clone()));
        }
        input
            .and_then(Resumption::one)
            .map(PureStep::Complete)
            .ok_or_else(|| fixture_diag("action dependency completed without a value"))
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

fn fixture_diag(message: &str) -> DiagnosticSink {
    let mut diagnostics = DiagnosticSink::new();
    diagnostics.push(pith_diag::Diag::new(
        Severity::Error,
        StableCode(1211),
        Span::none(),
        message,
    ));
    diagnostics
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
    Rule::<Pure>::new(revision, label, interface, Span::none())
}

/// The same rule at a different revision, which is what decision 0023 has an
/// author bump when the rule's semantics change.
fn revised_pure_rule(label: &str, interface: Interface, revision: &[u8]) -> Rule<Pure> {
    let identity = RuleIdentity::of_module_declaration("engine-state-wiring-tests", label);
    Rule::<Pure>::new(
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
    Rule::<Action>::new(revision, label, interface, Span::none())
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

/// A content store several engines share, the way a filesystem store is shared
/// by successive runs of a build.
#[derive(Clone, Default)]
struct SharedContentStore(Arc<std::sync::Mutex<MemoryContentStore>>);

impl SharedContentStore {
    fn put_fixture(&self, bytes: &[u8]) -> ContentId {
        match self.0.lock() {
            Ok(mut store) => put_fixture_blob(&mut store, bytes),
            Err(_) => unreachable!("the shared content store was poisoned"),
        }
    }
}

impl ContentStore for SharedContentStore {
    fn put_blob(&mut self, bytes: &[u8]) -> Result<ContentId, pith_store::StoreError> {
        match self.0.lock() {
            Ok(mut store) => store.put_blob(bytes),
            Err(_) => Err(pith_store::StoreError::new("shared content store poisoned")),
        }
    }

    fn get_blob(&self, id: ContentId) -> Result<Option<pith_store::Blob>, pith_store::StoreError> {
        match self.0.lock() {
            Ok(store) => store.get_blob(id),
            Err(_) => Err(pith_store::StoreError::new("shared content store poisoned")),
        }
    }

    fn put_tree(&mut self, tree: pith_store::Tree) -> Result<ContentId, pith_store::StoreError> {
        match self.0.lock() {
            Ok(mut store) => store.put_tree(tree),
            Err(_) => Err(pith_store::StoreError::new("shared content store poisoned")),
        }
    }

    fn get_tree(&self, id: ContentId) -> Result<Option<pith_store::Tree>, pith_store::StoreError> {
        match self.0.lock() {
            Ok(store) => store.get_tree(id),
            Err(_) => Err(pith_store::StoreError::new("shared content store poisoned")),
        }
    }
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

/// A store adapter whose `create_pending_attempt` always fails. Used to
/// exercise the error-hygiene path where the live engine cannot create a
/// durable attempt: the arena must not be left with an orphaned `Pending` node.
struct CreateFailingStore {
    inner: MemoryEngineStateStore,
}

impl CreateFailingStore {
    fn failure() -> EngineStateError {
        EngineStateError::Adapter {
            message: "fixture: create_pending_attempt disabled".into(),
        }
    }
}

impl EngineStateStore for CreateFailingStore {
    fn versions(&self) -> EngineStateVersions {
        self.inner.versions()
    }

    fn create_pending_attempt(
        &self,
        _computation: DurableComputation,
    ) -> Result<DurableAttemptId, EngineStateError> {
        Err(Self::failure())
    }

    fn publish_complete(
        &self,
        attempt: DurableAttemptId,
        completion: CompletedAttempt,
    ) -> Result<(), EngineStateError> {
        self.inner.publish_complete(attempt, completion)
    }

    fn publish_failed(
        &self,
        attempt: DurableAttemptId,
        failure: StoppedAttempt,
    ) -> Result<(), EngineStateError> {
        self.inner.publish_failed(attempt, failure)
    }

    fn publish_cancelled(
        &self,
        attempt: DurableAttemptId,
        cancellation: StoppedAttempt,
    ) -> Result<(), EngineStateError> {
        self.inner.publish_cancelled(attempt, cancellation)
    }

    fn attempt(
        &self,
        attempt: DurableAttemptId,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.inner.attempt(attempt)
    }

    fn attempt_history(
        &self,
        computation: PureComputationKey,
    ) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.inner.attempt_history(computation)
    }

    fn latest_completed_reusable_attempt(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.inner.latest_completed_reusable_attempt(computation)
    }

    fn latest_completed_reusable_action_attempt(
        &self,
        computation: ActionComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.inner
            .latest_completed_reusable_action_attempt(computation)
    }

    fn explain_invalidation(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<InvalidationExplanation>, EngineStateError> {
        self.inner.explain_invalidation(computation)
    }

    fn pending_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.inner.pending_attempts()
    }
}

/// A store adapter whose reusable-index read always fails. Decision 0024 treats
/// adapter failure as an error rather than a cache miss, so a broken adapter
/// must surface diagnostics instead of silently degrading into "recompute".
#[derive(Default)]
struct ReadFailingStore {
    inner: MemoryEngineStateStore,
}

impl ReadFailingStore {
    fn failure() -> EngineStateError {
        EngineStateError::Adapter {
            message: "fixture: reusable index unreadable".into(),
        }
    }
}

impl EngineStateStore for ReadFailingStore {
    fn versions(&self) -> EngineStateVersions {
        self.inner.versions()
    }

    fn create_pending_attempt(
        &self,
        computation: DurableComputation,
    ) -> Result<DurableAttemptId, EngineStateError> {
        self.inner.create_pending_attempt(computation)
    }

    fn publish_complete(
        &self,
        attempt: DurableAttemptId,
        completion: CompletedAttempt,
    ) -> Result<(), EngineStateError> {
        self.inner.publish_complete(attempt, completion)
    }

    fn publish_failed(
        &self,
        attempt: DurableAttemptId,
        failure: StoppedAttempt,
    ) -> Result<(), EngineStateError> {
        self.inner.publish_failed(attempt, failure)
    }

    fn publish_cancelled(
        &self,
        attempt: DurableAttemptId,
        cancellation: StoppedAttempt,
    ) -> Result<(), EngineStateError> {
        self.inner.publish_cancelled(attempt, cancellation)
    }

    fn attempt(
        &self,
        attempt: DurableAttemptId,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.inner.attempt(attempt)
    }

    fn attempt_history(
        &self,
        computation: PureComputationKey,
    ) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.inner.attempt_history(computation)
    }

    fn latest_completed_reusable_attempt(
        &self,
        _computation: PureComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        Err(Self::failure())
    }

    fn latest_completed_reusable_action_attempt(
        &self,
        _computation: ActionComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        Err(Self::failure())
    }

    fn explain_invalidation(
        &self,
        _computation: PureComputationKey,
    ) -> Result<Option<InvalidationExplanation>, EngineStateError> {
        Err(Self::failure())
    }

    fn pending_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.inner.pending_attempts()
    }
}

/// One durable substrate behind several [`Engine`] instances, which is how
/// decision 0024 describes a single process owning the writable engine
/// database. Hydration is not observable within one instance — the arena index
/// answers first — so these tests need a store that outlives an engine.
#[derive(Clone, Default)]
struct SharedEngineStateStore(Arc<std::sync::Mutex<MemoryEngineStateStore>>);

impl SharedEngineStateStore {
    fn read<T>(
        &self,
        read: impl FnOnce(&MemoryEngineStateStore) -> Result<T, EngineStateError>,
    ) -> Result<T, EngineStateError> {
        match self.0.lock() {
            Ok(store) => read(&store),
            Err(_) => Err(lock_poisoned()),
        }
    }

    fn write<T>(
        &self,
        write: impl FnOnce(&mut MemoryEngineStateStore) -> Result<T, EngineStateError>,
    ) -> Result<T, EngineStateError> {
        match self.0.lock() {
            Ok(mut store) => write(&mut store),
            Err(_) => Err(lock_poisoned()),
        }
    }
}

fn lock_poisoned() -> EngineStateError {
    EngineStateError::Adapter {
        message: "fixture: shared engine state lock was poisoned".into(),
    }
}

impl EngineStateStore for SharedEngineStateStore {
    fn versions(&self) -> EngineStateVersions {
        match self.0.lock() {
            Ok(store) => store.versions(),
            Err(_) => pith_engine::state::CURRENT_ENGINE_STATE_VERSIONS,
        }
    }

    fn create_pending_attempt(
        &self,
        computation: DurableComputation,
    ) -> Result<DurableAttemptId, EngineStateError> {
        self.write(|store| store.create_pending_attempt(computation))
    }

    fn publish_complete(
        &self,
        attempt: DurableAttemptId,
        completion: CompletedAttempt,
    ) -> Result<(), EngineStateError> {
        self.write(|store| store.publish_complete(attempt, completion))
    }

    fn publish_failed(
        &self,
        attempt: DurableAttemptId,
        failure: StoppedAttempt,
    ) -> Result<(), EngineStateError> {
        self.write(|store| store.publish_failed(attempt, failure))
    }

    fn publish_cancelled(
        &self,
        attempt: DurableAttemptId,
        cancellation: StoppedAttempt,
    ) -> Result<(), EngineStateError> {
        self.write(|store| store.publish_cancelled(attempt, cancellation))
    }

    fn attempt(
        &self,
        attempt: DurableAttemptId,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.read(|store| store.attempt(attempt))
    }

    fn attempt_history(
        &self,
        computation: PureComputationKey,
    ) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.read(|store| store.attempt_history(computation))
    }

    fn latest_completed_reusable_attempt(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.read(|store| store.latest_completed_reusable_attempt(computation))
    }

    fn latest_completed_reusable_action_attempt(
        &self,
        computation: ActionComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.read(|store| store.latest_completed_reusable_action_attempt(computation))
    }

    fn explain_invalidation(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<InvalidationExplanation>, EngineStateError> {
        self.read(|store| store.explain_invalidation(computation))
    }

    fn pending_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.read(|store| store.pending_attempts())
    }
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

#[test]
fn pure_leaf_evaluation_publishes_a_reusable_complete_attempt() {
    let mut engine = engine_with_fixtures();
    let leaf = interface(&[], Type::Int);
    engine.register_rule(
        pure_rule("leaf", leaf.clone()),
        ConstantRule(Value::Int(41)),
    );

    let evaluation = engine
        .evaluate_pure(&pure_request("leaf", leaf, []))
        .unwrap();

    let store = engine.state_store();
    assert_eq!(
        store.versions(),
        pith_engine::state::CURRENT_ENGINE_STATE_VERSIONS
    );
    let completion = completed_record(store, durable_id(&engine, evaluation.computation));
    assert_eq!(completion.result.decode(), Ok(Value::Int(41)));
    assert_eq!(completion.reuse, DurableReuseDecision::Reusable);
    assert!(completion.dependencies.is_empty());
    assert_eq!(completion.provenance, DurableProvenance::Pure);
    assert_eq!(store.pending_attempts().unwrap().len(), 0);
}

#[test]
fn pure_parent_with_child_dependency_publishes_both_with_the_pure_edge() {
    let mut engine = engine_with_fixtures();
    let leaf = interface(&[], Type::Int);
    let parent = interface(&[Type::Bool], Type::Int);
    engine.register_rule(
        pure_rule("leaf", leaf.clone()),
        ConstantRule(Value::Int(41)),
    );
    engine.register_rule(
        pure_rule("increment", parent.clone()),
        IncrementRule {
            dependency: pure_request("base", leaf, []),
        },
    );

    let evaluation = engine
        .evaluate_pure(&pure_request("root", parent, [Value::Bool(false)]))
        .unwrap();

    let store = engine.state_store();
    let leaf_computation = leaf_dependency_of(&engine, evaluation.computation);

    let parent_record = completed_record(store, durable_id(&engine, evaluation.computation));
    let leaf_record = completed_record(store, durable_id(&engine, leaf_computation));

    assert_eq!(parent_record.result.decode(), Ok(Value::Int(42)));
    assert_eq!(parent_record.reuse, DurableReuseDecision::Reusable);
    assert_eq!(leaf_record.result.decode(), Ok(Value::Int(41)));
    assert_eq!(leaf_record.reuse, DurableReuseDecision::Reusable);
    assert_eq!(
        parent_record.dependencies.as_ref(),
        [DurableDependency::Pure {
            computation: pure_key_of(&engine, leaf_computation),
            attempt: durable_id(&engine, leaf_computation),
        }]
    );
}

#[test]
fn failed_pure_evaluation_publishes_a_failed_attempt_with_diagnostics() {
    let mut engine = engine_with_fixtures();
    let failing = interface(&[], Type::Int);
    engine.register_rule(pure_rule("failing", failing.clone()), FailingRule);

    let diagnostics = engine
        .evaluate_pure(&pure_request("failing", failing, []))
        .err()
        .unwrap();

    let store = engine.state_store();
    let (computation, _) = sole_pure_computation(&engine);
    let failure = failed_record(store, durable_id(&engine, computation));
    assert_eq!(failure.provenance, DurableProvenance::Pure);
    assert_eq!(
        failure
            .diagnostics
            .first()
            .map(|diag| diag.message.as_ref()),
        Some("fixture pure failure")
    );
    assert_eq!(diagnostics.iter().count(), failure.diagnostics.len());
}

#[test]
fn pure_create_failure_reconciles_the_orphaned_arena_node() {
    // When the store cannot create a durable attempt (only a failing adapter
    // reaches this; the memory adapter is infallible), the pure path must not
    // leave an orphaned `Pending` arena node behind. It mirrors the action
    // path's error hygiene by failing the node in the arena. No durable record
    // is published because no durable attempt exists.
    let mut engine = Engine::with_state_store(
        MemoryContentStore::default(),
        CreateFailingStore {
            inner: MemoryEngineStateStore::default(),
        },
    );
    let signature = interface(&[], Type::Int);
    engine.register_rule(
        pure_rule("constant", signature.clone()),
        ConstantRule(Value::Int(7)),
    );

    let diagnostics = engine
        .evaluate_pure(&pure_request("constant", signature, []))
        .err()
        .unwrap();

    // The adapter failure surfaces as an internal-invariant diagnostic.
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == pith_diag::EngineCode::InternalInvariant.into())
    );
    // The orphaned arena node was reconciled to a terminal state: no
    // `Pending` computation remains.
    assert!(
        engine
            .query()
            .computations()
            .all(|(_, node)| !matches!(node.state, AttemptState::Pending))
    );
    let (computation, node) = sole_pure_computation(&engine);
    let _ = computation;
    assert!(matches!(node.state, AttemptState::Failed { .. }));
    // No durable attempt was recorded for the orphan.
    assert!(engine.durable_attempt_for(computation).is_none());
}

// ---------------------------------------------------------------------------
// Publish path: action computations
// ---------------------------------------------------------------------------

#[test]
fn action_dependency_publishes_a_reusable_action_and_a_reusable_parent() {
    let mut engine = engine_with_fixtures();
    let action_interface = interface(&[], Type::Blob);
    let root_interface = interface(&[], Type::Blob);
    engine.register_action_rule(
        action_rule("produce", action_interface.clone()),
        BlobProducingAction,
    );
    engine.register_rule(
        pure_rule("entry", root_interface.clone()),
        ActionDepRule {
            dependency: action_request("produce", action_interface, []),
        },
    );

    let evaluation = engine
        .run(
            &pure_request("entry", root_interface, []),
            &runtime(),
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap()
        .unwrap();

    let store = engine.state_store();
    let action_computation = sole_action_computation(&engine).0;
    let action_id = durable_id(&engine, action_computation);
    let action_completion = completed_record(store, action_id);

    // An action's only edges are capability use, which never blocks reuse.
    assert_eq!(action_completion.reuse, DurableReuseDecision::Reusable);
    // A completed action retains the imported report (decision 0024).
    match &action_completion.provenance {
        DurableProvenance::Action(DurableActionProvenance::Imported { imported_report }) => {
            assert_eq!(imported_report.executor.as_ref(), "fixture");
            assert_eq!(imported_report.access, AccessVerification::Prevented);
        }
        provenance => unreachable!("action provenance was {provenance:?}, expected Imported"),
    }
    // Capability-use edges equal the canonicalized reported capabilities.
    assert_eq!(
        action_completion.dependencies.as_ref(),
        [DurableDependency::CapabilityUse {
            capability: action_capability(),
        }]
    );

    // The parent enters the index too (decision 0033), and the gap its key
    // leaves is closed when it is read back.
    let parent_record = completed_record(store, durable_id(&engine, evaluation.computation));
    assert_eq!(parent_record.reuse, DurableReuseDecision::Reusable);
    assert_eq!(
        parent_record.dependencies.as_ref(),
        [DurableDependency::Action { attempt: action_id }]
    );
    assert_eq!(parent_record.provenance, DurableProvenance::Pure);
}

#[test]
fn denied_action_publishes_failed_attempt_with_not_executed_provenance() {
    let mut engine = engine_with_fixtures();
    let action_interface = interface(&[], Type::Blob);
    let root_interface = interface(&[], Type::Blob);
    engine.register_action_rule(
        action_rule("produce", action_interface.clone()),
        BlobProducingAction,
    );
    engine.register_rule(
        pure_rule("entry", root_interface.clone()),
        ActionDepRule {
            dependency: action_request("produce", action_interface, []),
        },
    );

    let diagnostics = engine
        .run(
            &pure_request("entry", root_interface, []),
            &runtime(),
            &DenyCapability,
            &FixtureExecutor,
        )
        .unwrap()
        .err()
        .unwrap();

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == pith_diag::EngineCode::PolicyDenied.into())
    );
    let store = engine.state_store();
    let action_computation = sole_action_computation(&engine).0;
    let failure = failed_record(store, durable_id(&engine, action_computation));
    // A denied action never reached execution: no executor report.
    assert_eq!(
        failure.provenance,
        DurableProvenance::Action(DurableActionProvenance::NotExecuted)
    );
    assert!(failure.dependencies.is_empty());
}

// ---------------------------------------------------------------------------
// Reuse path: durable revalidation (decision 0024)
// ---------------------------------------------------------------------------

#[test]
fn durable_reuse_is_valid_until_a_dependency_result_identity_changes() {
    let mut engine = engine_with_fixtures();
    let leaf = interface(&[], Type::Int);
    let root = interface(&[Type::Bool], Type::Int);
    engine.register_rule(pure_rule("leaf", leaf.clone()), ConstantRule(Value::Int(1)));
    engine.register_rule(
        pure_rule("root", root.clone()),
        ForwardRule {
            dependency: pure_request("leaf", leaf, []),
        },
    );

    let root_evaluation = engine
        .evaluate_pure(&pure_request("root", root, [Value::Bool(false)]))
        .unwrap();

    // After computing both, root's durable reuse is valid.
    assert!(
        engine
            .durable_reuse_is_valid(root_evaluation.computation, &ReuseContext::PureOnly)
            .unwrap()
    );

    // Simulate `leaf` being recomputed under a new attempt whose durable result
    // identity changed (e.g. a new revision produced different bytes). Publish a
    // second completed attempt for the leaf's key directly through the shared
    // store, so it becomes the latest reusable attempt.
    let leaf_computation = leaf_dependency_of(&engine, root_evaluation.computation);
    let leaf_key = pure_key_of(&engine, leaf_computation);
    let original_leaf_attempt = durable_id(&engine, leaf_computation);
    let changed_leaf = engine
        .state_store()
        .create_pending_attempt(DurableComputation::Pure(leaf_key))
        .unwrap();
    engine
        .state_store()
        .publish_complete(
            changed_leaf,
            CompletedAttempt {
                dependencies: Box::new([]),
                result: EncodedValue::from_value(&Value::Int(99)),
                provenance: DurableProvenance::Pure,
                reuse: DurableReuseDecision::Reusable,
                capabilities: Box::new([]),
            },
        )
        .unwrap();

    // The leaf's latest reusable attempt is now a different attempt with a
    // different result: root's durable reuse is dirty.
    assert_ne!(changed_leaf, original_leaf_attempt);
    assert!(
        !engine
            .durable_reuse_is_valid(root_evaluation.computation, &ReuseContext::PureOnly)
            .unwrap()
    );
}

#[test]
fn durable_reuse_remains_valid_when_a_dependency_result_is_canonically_equal() {
    let mut engine = engine_with_fixtures();
    let leaf = interface(&[], Type::Int);
    let root = interface(&[Type::Bool], Type::Int);
    engine.register_rule(pure_rule("leaf", leaf.clone()), ConstantRule(Value::Int(1)));
    engine.register_rule(
        pure_rule("root", root.clone()),
        ForwardRule {
            dependency: pure_request("leaf", leaf, []),
        },
    );

    let root_evaluation = engine
        .evaluate_pure(&pure_request("root", root, [Value::Bool(false)]))
        .unwrap();
    assert!(
        engine
            .durable_reuse_is_valid(root_evaluation.computation, &ReuseContext::PureOnly)
            .unwrap()
    );

    // Publish a second leaf attempt under a new id but with an equal result.
    // Decision 0024: downstream propagation stops even though a new attempt
    // records the changed upstream provenance.
    let leaf_computation = leaf_dependency_of(&engine, root_evaluation.computation);
    let leaf_key = pure_key_of(&engine, leaf_computation);
    let equal_leaf = engine
        .state_store()
        .create_pending_attempt(DurableComputation::Pure(leaf_key))
        .unwrap();
    engine
        .state_store()
        .publish_complete(
            equal_leaf,
            CompletedAttempt {
                dependencies: Box::new([]),
                result: EncodedValue::from_value(&Value::Int(1)),
                provenance: DurableProvenance::Pure,
                reuse: DurableReuseDecision::Reusable,
                capabilities: Box::new([]),
            },
        )
        .unwrap();

    assert!(
        engine
            .durable_reuse_is_valid(root_evaluation.computation, &ReuseContext::PureOnly)
            .unwrap()
    );
}

#[test]
fn durable_reuse_observed_through_engine_reuses_unchanged_computations() {
    // A smoke test that the durable gate does not block ordinary in-memory
    // reuse within one engine instance: a second evaluation of the same root
    // returns the cached computation as Reused.
    let mut engine = engine_with_fixtures();
    let leaf = interface(&[], Type::Int);
    let root = interface(&[Type::Bool], Type::Int);
    engine.register_rule(pure_rule("leaf", leaf.clone()), ConstantRule(Value::Int(1)));
    engine.register_rule(
        pure_rule("root", root.clone()),
        ForwardRule {
            dependency: pure_request("leaf", leaf, []),
        },
    );

    let first = engine
        .evaluate_pure(&pure_request("root", root.clone(), [Value::Bool(false)]))
        .unwrap();
    let second = engine
        .evaluate_pure(&pure_request("root", root, [Value::Bool(false)]))
        .unwrap();

    assert_eq!(first.source, EvaluationSource::Computed);
    assert_eq!(second.source, EvaluationSource::Reused);
    assert_eq!(first.computation, second.computation);
}

// ---------------------------------------------------------------------------
// Hydration: durable reuse across engine instances (decision 0024)
// ---------------------------------------------------------------------------

#[test]
fn hydrates_a_completed_pure_result_into_a_fresh_engine() {
    let state = SharedEngineStateStore::default();
    let leaf = interface(&[], Type::Int);

    let mut first = engine_with_state(state.clone());
    first.register_rule(pure_rule("leaf", leaf.clone()), ConstantRule(Value::Int(7)));
    let computed = first
        .evaluate_pure(&pure_request("leaf", leaf.clone(), []))
        .unwrap();
    assert_eq!(computed.source, EvaluationSource::Computed);
    let original_attempt = durable_id(&first, computed.computation);
    let key = pure_key_of(&first, computed.computation);
    assert_eq!(attempt_history_len(&state, key), 1);

    // A fresh engine over the same durable substrate: new arena, no in-process
    // reuse available. The rule is registered with a body that fails if it runs,
    // so reaching a value at all proves the result came from engine state.
    let mut second = engine_with_state(state.clone());
    second.register_rule(pure_rule("leaf", leaf.clone()), FailingRule);
    let hydrated = second
        .evaluate_pure(&pure_request("leaf", leaf, []))
        .unwrap();

    assert_eq!(hydrated.source, EvaluationSource::Hydrated);
    assert_eq!(hydrated.value, Value::Int(7));
    // Hydration maps the fresh arena node onto the attempt it loaded, and
    // records no new attempt: loading a result is not an evaluation of it.
    assert_eq!(durable_id(&second, hydrated.computation), original_attempt);
    assert_eq!(attempt_history_len(&state, key), 1);
    // The hydrated node is terminal and reusable in the new arena, so a third
    // request inside the same instance takes the in-process path.
    assert!(
        second
            .durable_reuse_is_valid(hydrated.computation, &ReuseContext::PureOnly)
            .unwrap()
    );
}

/// A second engine over the same durable substrate hydrates the consumer of the
/// first engine's action (decision 0033), so neither the consumer's rule body
/// nor the action runs. Revalidating the recorded action edge re-plans it and
/// finds the recorded attempt still admissible; nothing below the consumer is
/// allocated, because there is nothing left to ask.
#[test]
fn hydrates_a_consumer_of_an_action_into_a_fresh_engine() {
    let state = SharedEngineStateStore::default();
    let content = SharedContentStore::default();
    let action_interface = interface(&[], Type::Blob);
    let root_interface = interface(&[], Type::Blob);

    let mut first = engine_with_shared_substrate(state.clone(), content.clone());
    register_action_fixtures(&mut first, &action_interface, &root_interface);
    let computed = first
        .run(
            &pure_request("entry", root_interface.clone(), []),
            &runtime(),
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap()
        .unwrap();
    let original_root = durable_id(&first, computed.computation);

    let mut second = engine_with_shared_substrate(state.clone(), content.clone());
    register_action_fixtures(&mut second, &action_interface, &root_interface);
    let hydrated = second
        .run(
            &pure_request("entry", root_interface, []),
            &runtime(),
            &AllowAllActions,
            &NeverRunsExecutor,
        )
        .unwrap()
        .unwrap();

    assert_eq!(hydrated.source, EvaluationSource::Hydrated);
    assert_eq!(hydrated.value, computed.value);
    // On the terms 0024 set for pure hydration, the node is mapped onto the
    // attempt it was loaded from and records no new one.
    assert_eq!(durable_id(&second, hydrated.computation), original_root);
    assert_eq!(second.query().computations().count(), 1);
}

/// The action half of the same substrate, reached when the consumer cannot be
/// served. The second engine registers the consumer at a later revision, so its
/// pure key differs and its body runs; the `NeedAction` step it reaches is then
/// answered from the reusable action index (decision 0031).
#[test]
fn hydrates_a_completed_action_when_its_consumer_must_rerun() {
    let state = SharedEngineStateStore::default();
    let content = SharedContentStore::default();
    let action_interface = interface(&[], Type::Blob);
    let root_interface = interface(&[], Type::Blob);

    let mut first = engine_with_shared_substrate(state.clone(), content.clone());
    register_action_fixtures(&mut first, &action_interface, &root_interface);
    let computed = first
        .run(
            &pure_request("entry", root_interface.clone(), []),
            &runtime(),
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap()
        .unwrap();
    let original_attempt = durable_id(&first, sole_action_computation(&first).0);

    let mut second = engine_with_shared_substrate(state.clone(), content.clone());
    second.register_action_rule(
        action_rule("produce", action_interface.clone()),
        BlobProducingAction,
    );
    second.register_rule(
        revised_pure_rule("entry", root_interface.clone(), b"revised"),
        ActionDepRule {
            dependency: action_request("produce", action_interface, []),
        },
    );
    let recomputed = second
        .run(
            &pure_request("entry", root_interface, []),
            &runtime(),
            &AllowAllActions,
            &NeverRunsExecutor,
        )
        .unwrap()
        .unwrap();

    assert_eq!(recomputed.source, EvaluationSource::Computed);
    assert_eq!(recomputed.value, computed.value);
    let (hydrated_action, hydrated_node) = sole_action_computation(&second);
    assert_eq!(durable_id(&second, hydrated_action), original_attempt);
    assert!(matches!(
        hydrated_node.state,
        AttemptState::Complete {
            reuse: ReuseDecision::Reusable,
            ..
        }
    ));
}

/// The same two engines, with an empty content store under the second. The
/// index still names the attempt, and the bytes it produced are gone, so the
/// action runs again instead of handing back an unresolvable identity.
#[test]
fn an_action_whose_output_content_is_missing_is_not_reused() {
    let state = SharedEngineStateStore::default();
    let action_interface = interface(&[], Type::Blob);
    let root_interface = interface(&[], Type::Blob);

    let mut first = engine_with_shared_substrate(state.clone(), SharedContentStore::default());
    register_action_fixtures(&mut first, &action_interface, &root_interface);
    first
        .run(
            &pure_request("entry", root_interface.clone(), []),
            &runtime(),
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap()
        .unwrap();

    let mut second = engine_with_shared_substrate(state.clone(), SharedContentStore::default());
    register_action_fixtures(&mut second, &action_interface, &root_interface);
    let second_result = second.run(
        &pure_request("entry", root_interface, []),
        &runtime(),
        &AllowAllActions,
        &NeverRunsExecutor,
    );

    assert!(matches!(second_result, Ok(Err(_))));
}

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

#[test]
fn a_hydrated_computation_serves_as_a_dependency_of_a_new_computation() {
    // The load-bearing case: a hydrated node must carry its durable identity,
    // not just its value, so a computation built on top of it publishes an edge
    // naming the original attempt rather than a duplicate.
    let state = SharedEngineStateStore::default();
    let leaf = interface(&[], Type::Int);
    let root = interface(&[Type::Bool], Type::Int);

    let mut first = engine_with_state(state.clone());
    first.register_rule(pure_rule("leaf", leaf.clone()), ConstantRule(Value::Int(1)));
    let leaf_evaluation = first
        .evaluate_pure(&pure_request("leaf", leaf.clone(), []))
        .unwrap();
    let leaf_attempt = durable_id(&first, leaf_evaluation.computation);
    let leaf_key = pure_key_of(&first, leaf_evaluation.computation);

    // The second engine has never evaluated the leaf, and its leaf body fails
    // if it runs: the root can only complete by hydrating its dependency.
    let mut second = engine_with_state(state.clone());
    second.register_rule(pure_rule("leaf", leaf.clone()), FailingRule);
    second.register_rule(
        pure_rule("root", root.clone()),
        IncrementRule {
            dependency: pure_request("leaf", leaf, []),
        },
    );
    let root_evaluation = second
        .evaluate_pure(&pure_request("root", root, [Value::Bool(false)]))
        .unwrap();

    assert_eq!(root_evaluation.source, EvaluationSource::Computed);
    assert_eq!(root_evaluation.value, Value::Int(2));

    // The root's published edge names the leaf's original durable attempt, and
    // the root is itself reusable because that dependency is.
    let root_record = completed_record(&state, durable_id(&second, root_evaluation.computation));
    assert_eq!(root_record.reuse, DurableReuseDecision::Reusable);
    assert_eq!(
        root_record.dependencies.as_ref(),
        [DurableDependency::Pure {
            computation: leaf_key,
            attempt: leaf_attempt,
        }]
    );
    // Still one attempt for the leaf: hydration did not re-record it.
    assert_eq!(attempt_history_len(&state, leaf_key), 1);
}

#[test]
fn hydration_is_refused_when_a_recorded_dependency_changed() {
    let state = SharedEngineStateStore::default();
    let leaf = interface(&[], Type::Int);
    let root = interface(&[Type::Bool], Type::Int);

    let mut first = engine_with_state(state.clone());
    first.register_rule(pure_rule("leaf", leaf.clone()), ConstantRule(Value::Int(1)));
    first.register_rule(
        pure_rule("root", root.clone()),
        ForwardRule {
            dependency: pure_request("leaf", leaf.clone(), []),
        },
    );
    let first_root = first
        .evaluate_pure(&pure_request("root", root.clone(), [Value::Bool(false)]))
        .unwrap();
    let leaf_key = pure_key_of(&first, leaf_dependency_of(&first, first_root.computation));

    // Another engine recomputed the leaf to a different result, so the root's
    // recorded dependency set no longer describes the current graph.
    publish_reusable_attempt(&state, leaf_key, &Value::Int(99));

    let mut second = engine_with_state(state.clone());
    second.register_rule(pure_rule("leaf", leaf.clone()), ConstantRule(Value::Int(1)));
    second.register_rule(
        pure_rule("root", root.clone()),
        ForwardRule {
            dependency: pure_request("leaf", leaf, []),
        },
    );
    let second_root = second
        .evaluate_pure(&pure_request("root", root, [Value::Bool(false)]))
        .unwrap();

    // The root was dirty, so it re-evaluated. Its leaf dependency hydrated from
    // the newer attempt, so the recomputed root observes the changed value.
    assert_eq!(second_root.source, EvaluationSource::Computed);
    assert_eq!(second_root.value, Value::Int(99));
}

#[test]
fn hydration_survives_a_dependency_recomputed_to_an_equal_result() {
    let state = SharedEngineStateStore::default();
    let leaf = interface(&[], Type::Int);
    let root = interface(&[Type::Bool], Type::Int);

    let mut first = engine_with_state(state.clone());
    first.register_rule(pure_rule("leaf", leaf.clone()), ConstantRule(Value::Int(1)));
    first.register_rule(
        pure_rule("root", root.clone()),
        ForwardRule {
            dependency: pure_request("leaf", leaf.clone(), []),
        },
    );
    let first_root = first
        .evaluate_pure(&pure_request("root", root.clone(), [Value::Bool(false)]))
        .unwrap();
    let root_attempt = durable_id(&first, first_root.computation);
    let leaf_key = pure_key_of(&first, leaf_dependency_of(&first, first_root.computation));

    // A new leaf attempt with a canonically equal result. Decision 0024: the
    // consumer is not dirty, so downstream propagation stops here.
    let equal_leaf = publish_reusable_attempt(&state, leaf_key, &Value::Int(1));

    let mut second = engine_with_state(state.clone());
    second.register_rule(pure_rule("leaf", leaf.clone()), FailingRule);
    second.register_rule(
        pure_rule("root", root.clone()),
        ForwardRule {
            dependency: pure_request("leaf", leaf, []),
        },
    );
    let second_root = second
        .evaluate_pure(&pure_request("root", root, [Value::Bool(false)]))
        .unwrap();

    assert_eq!(second_root.source, EvaluationSource::Hydrated);
    assert_eq!(second_root.value, Value::Int(1));
    assert_eq!(durable_id(&second, second_root.computation), root_attempt);

    // A hydrated node has no arena subgraph; its recorded dependency set stays
    // authoritative on the durable attempt and is reachable through the query
    // interface. The edge still names the attempt the root was published with,
    // not the equal-result attempt that superseded it.
    assert_eq!(
        second
            .query()
            .dependencies_of(second_root.computation)
            .map(<[_]>::len),
        Some(0)
    );
    let recorded = second
        .query()
        .durable_attempt_of(second_root.computation)
        .unwrap()
        .unwrap();
    let DurableAttemptState::Complete(recorded) = &recorded.state else {
        unreachable!("the hydrated attempt is complete")
    };
    assert_eq!(
        recorded.dependencies.as_ref(),
        [DurableDependency::Pure {
            computation: leaf_key,
            attempt: durable_id(&first, leaf_dependency_of(&first, first_root.computation)),
        }]
    );
    assert_ne!(
        equal_leaf,
        durable_id(&first, leaf_dependency_of(&first, first_root.computation))
    );
}

#[test]
fn hydration_reports_an_engine_state_read_failure() {
    // A broken adapter must not read as "nothing cached": decision 0024 makes
    // corruption an adapter error, never a cache miss.
    let mut engine = engine_with_state(ReadFailingStore::default());
    let leaf = interface(&[], Type::Int);
    engine.register_rule(pure_rule("leaf", leaf.clone()), ConstantRule(Value::Int(7)));

    let diagnostics = engine
        .evaluate_pure(&pure_request("leaf", leaf, []))
        .err()
        .unwrap();

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == pith_diag::EngineCode::InternalInvariant.into())
    );
}
