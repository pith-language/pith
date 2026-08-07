//! Live-engine coverage for the publish and reuse wiring to
//! [`EngineStateStore`] (decision 0024). The conformance suite in
//! `engine_state.rs` exercises the store through the trait directly; these
//! tests exercise the live [`Engine`] publishing real computations and
//! revalidating durable reuse.

use pith_core::{
    Action, ActionInput, ActionOutput, ActionSpec, CapabilityRequirement, Content, Interface,
    OutputKind, PlatformRequirement, Pure, PureComputationKey, Request, Rule, RuleIdentity,
    RuleRevision, Type, Value,
};
use pith_diag::{DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_engine::state::{
    CompletedAttempt, DurableActionProvenance, DurableAttemptId, DurableAttemptState,
    DurableComputation, DurableDependency, DurableProvenance, DurableReuseDecision,
    DurableReuseReason, EncodedValue, EngineStateStore, MemoryEngineStateStore,
};
use pith_engine::{
    AccessVerification, ActionAuthorization, ActionExecution, ActionInvocation, ActionRule,
    AllowAllActions, CapturedActionExecution, CapturedExecutionReport, CapturedOutput,
    CapturedOutputContent, ComputationKind, Engine, EvaluationSource, ExecutionPlatform, Executor,
    PureRule, PureRuleFrame, PureStep,
};
use pith_ids::ContentId;
use pith_store::{ContentStore, MemoryContentStore};

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
    fn step(&mut self, _input: Option<Value>) -> PithResult<PureStep> {
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
    fn step(&mut self, _input: Option<Value>) -> PithResult<PureStep> {
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
    fn step(&mut self, input: Option<Value>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::Need(self.dependency.clone()));
        }
        Ok(PureStep::Complete(input.unwrap_or(Value::Unit)))
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
    fn step(&mut self, input: Option<Value>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::Need(self.dependency.clone()));
        }
        match input {
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
    fn step(&mut self, input: Option<Value>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::NeedAction(self.dependency.clone()));
        }
        input
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
        Ok(CapturedActionExecution {
            report: CapturedExecutionReport {
                executor: "fixture".into(),
                platform: fixture_platform(),
                access: AccessVerification::Prevented,
                outputs: [CapturedOutput {
                    path: output.path.clone(),
                    content: CapturedOutputContent::Blob(b"42".to_vec().into_boxed_slice()),
                }]
                .into(),
                capabilities_used: [action_capability()].into(),
            },
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

fn action_executable() -> ContentId {
    ContentId::of_blob(b"fixture:action-executable")
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
    let mut content = MemoryContentStore::default();
    let executable = put_fixture_blob(&mut content, b"fixture:action-executable");
    assert_eq!(executable, action_executable());
    let input = put_fixture_blob(&mut content, b"fixture input");
    assert_eq!(input, action_input());
    Engine::with_state_store(content, MemoryEngineStateStore::default())
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
) -> pith_engine::state::FailedAttempt {
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

// ---------------------------------------------------------------------------
// Publish path: action computations
// ---------------------------------------------------------------------------

#[test]
fn action_dependency_publishes_non_reusable_action_and_not_reusable_parent() {
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
            &pith_engine::TokioRuntime,
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap()
        .unwrap();

    let store = engine.state_store();
    let action_computation = sole_action_computation(&engine).0;
    let action_id = durable_id(&engine, action_computation);
    let action_completion = completed_record(store, action_id);

    // Action attempts are non-reusable: caching is disabled (decision 0023).
    assert_eq!(
        action_completion.reuse,
        DurableReuseDecision::NotReusable(DurableReuseReason::ActionCachingDisabled)
    );
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

    // The pure parent of a non-reusable action is itself NotReusable, pointing
    // at that dependency's durable attempt (cross-cutting invariant).
    let parent_record = completed_record(store, durable_id(&engine, evaluation.computation));
    assert_eq!(
        parent_record.reuse,
        DurableReuseDecision::NotReusable(DurableReuseReason::DependencyNotReusable {
            attempt: action_id,
        })
    );
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
            &pith_engine::TokioRuntime,
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
    assert!(engine.durable_reuse_is_valid(root_evaluation.computation));

    // Simulate `leaf` being recomputed under a new attempt whose durable result
    // identity changed (e.g. a new revision produced different bytes). Publish a
    // second completed attempt for the leaf's key directly through the shared
    // store, so it becomes the latest reusable attempt.
    let leaf_computation = leaf_dependency_of(&engine, root_evaluation.computation);
    let leaf_key = pure_key_of(&engine, leaf_computation);
    let original_leaf_attempt = durable_id(&engine, leaf_computation);
    let changed_leaf = engine
        .state_store_mut()
        .create_pending_attempt(DurableComputation::Pure(leaf_key))
        .unwrap();
    engine
        .state_store_mut()
        .publish_complete(
            changed_leaf,
            CompletedAttempt {
                dependencies: Box::new([]),
                result: EncodedValue::from_value(&Value::Int(99)),
                provenance: DurableProvenance::Pure,
                reuse: DurableReuseDecision::Reusable,
            },
        )
        .unwrap();

    // The leaf's latest reusable attempt is now a different attempt with a
    // different result: root's durable reuse is dirty.
    assert_ne!(changed_leaf, original_leaf_attempt);
    assert!(!engine.durable_reuse_is_valid(root_evaluation.computation));
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
    assert!(engine.durable_reuse_is_valid(root_evaluation.computation));

    // Publish a second leaf attempt under a new id but with an equal result.
    // Decision 0024: downstream propagation stops even though a new attempt
    // records the changed upstream provenance.
    let leaf_computation = leaf_dependency_of(&engine, root_evaluation.computation);
    let leaf_key = pure_key_of(&engine, leaf_computation);
    let equal_leaf = engine
        .state_store_mut()
        .create_pending_attempt(DurableComputation::Pure(leaf_key))
        .unwrap();
    engine
        .state_store_mut()
        .publish_complete(
            equal_leaf,
            CompletedAttempt {
                dependencies: Box::new([]),
                result: EncodedValue::from_value(&Value::Int(1)),
                provenance: DurableProvenance::Pure,
                reuse: DurableReuseDecision::Reusable,
            },
        )
        .unwrap();

    assert!(engine.durable_reuse_is_valid(root_evaluation.computation));
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
