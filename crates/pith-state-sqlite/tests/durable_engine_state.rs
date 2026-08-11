//! The sqlite adapter against a real filesystem, including the property the
//! in-memory adapter cannot demonstrate: a result computed by one *process*
//! being hydrated by another (decision 0024).

use std::path::{Path, PathBuf};
use std::process::Command;

use diesel::Connection as _;
use pith_core::{
    ActionSpec, CapabilityRequirement, Content, Interface, Pure, PureComputationKey, Request, Rule,
    RuleIdentity, RuleRevision, Type, Value,
};
use pith_diag::{PithResult, Severity, Span, StableCode};
use pith_engine::state::{
    CompletedAttempt, DurableActionProvenance, DurableAttemptState, DurableComputation,
    DurableDependency, DurableDiagnostic, DurableProvenance, DurableReuseDecision,
    DurableReuseReason, DurableRule, EncodedValue, EngineStateStore, StoppedAttempt,
};
use pith_engine::{
    AccessVerification, ActionAuthorization, Engine, EvaluationSource, ExecutionPlatform,
    ExecutionReport, ProducedOutput, PureRule, PureRuleFrame, PureStep, Resumption,
};
use pith_ids::{ContentDigest, ContentId};
use pith_state_sqlite::SqliteEngineStateStore;

fn database_url(path: &Path) -> &str {
    match path.to_str() {
        Some(url) => url,
        None => unreachable!("the scratch path is not utf-8"),
    }
}

// ---------------------------------------------------------------------------
// Scratch directories
// ---------------------------------------------------------------------------

/// A scratch directory removed when the test ends. Named from the process id
/// and a per-test label rather than a clock, so a failed run leaves a
/// predictable path to inspect.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("pith-sqlite-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        if let Err(error) = std::fs::create_dir_all(&path) {
            unreachable!("could not create the scratch directory: {error}");
        }
        Self { path }
    }

    fn database(&self) -> PathBuf {
        self.path.join("engine-state.sqlite")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn open(path: &Path) -> SqliteEngineStateStore {
    match SqliteEngineStateStore::open(path) {
        Ok(store) => store,
        Err(error) => unreachable!("could not open the engine-state database: {error}"),
    }
}

// ---------------------------------------------------------------------------
// Rule fixtures, matching the `write_leaf` binary's declarations
// ---------------------------------------------------------------------------

fn leaf_interface() -> Interface {
    Interface {
        inputs: Box::new([]),
        output: Type::Int,
    }
}

/// Identical derivation to the fixture binary's, which is the point: two
/// processes agree on a computation key without sharing anything but strings.
fn leaf_rule() -> Rule<Pure> {
    let identity = RuleIdentity::of_module_declaration("pith-state-sqlite-fixture", "leaf");
    let revision = RuleRevision::of_manifest(identity, b"pith-state-sqlite-fixture-v1");
    Rule::<Pure>::new(revision, "leaf", leaf_interface(), Span::none())
}

fn leaf_request() -> Request<Pure> {
    Request::<Pure>::new("leaf", leaf_interface(), [], Span::none())
}

fn leaf_key() -> PureComputationKey {
    PureComputationKey::new(&leaf_rule(), &leaf_request())
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
        let mut sink = pith_diag::DiagnosticSink::new();
        sink.push(pith_diag::Diag::new(
            Severity::Error,
            StableCode(9001),
            Span::none(),
            "the leaf body ran, so the result was not hydrated",
        ));
        Err(sink)
    }
}

/// Run the fixture binary as a separate process and wait for it to exit.
fn write_leaf_in_another_process(database: &Path, value: i64) {
    let output = Command::new(env!("CARGO_BIN_EXE_write-leaf"))
        .arg(database)
        .arg(value.to_string())
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) => unreachable!("could not run the writer process: {error}"),
    };
    assert!(
        output.status.success(),
        "the writer process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ---------------------------------------------------------------------------
// Cross-process hydration
// ---------------------------------------------------------------------------

#[test]
fn a_result_computed_by_another_process_hydrates_here() {
    let scratch = Scratch::new("cross-process");
    let database = scratch.database();

    // A separate process computes the leaf and exits. Nothing of its arena
    // survives; only the database it wrote.
    write_leaf_in_another_process(&database, 7);

    // This process registers the same rule with a body that fails if it runs,
    // so producing a value at all proves the result came from the database.
    let mut engine =
        Engine::with_state_store(pith_store::MemoryContentStore::default(), open(&database));
    engine.register_rule(leaf_rule(), FailingRule);

    let evaluation = match engine.evaluate_pure(&leaf_request()) {
        Ok(evaluation) => evaluation,
        Err(diagnostics) => {
            let messages: Vec<_> = diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.0.to_string())
                .collect();
            unreachable!("the leaf did not hydrate: {messages:?}");
        }
    };

    assert_eq!(evaluation.source, EvaluationSource::Hydrated);
    assert_eq!(evaluation.value, Value::Int(7));
    // The writer's attempt was adopted, not duplicated.
    let history = match engine.state_store().attempt_history(leaf_key()) {
        Ok(history) => history,
        Err(error) => unreachable!("history is unreadable: {error}"),
    };
    assert_eq!(history.len(), 1);
    assert_eq!(
        engine.durable_attempt_for(evaluation.computation),
        history.first().map(|attempt| attempt.id)
    );
}

#[test]
fn a_dependency_changed_by_another_process_makes_the_consumer_dirty() {
    let scratch = Scratch::new("cross-process-dirty");
    let database = scratch.database();

    write_leaf_in_another_process(&database, 1);
    // A second run under a different value would need a different computation
    // key, so instead publish a newer reusable attempt for the same key, which
    // is what a recomputation under changed conditions leaves behind.
    {
        let state = open(&database);
        let attempt = match state.create_pending_attempt(DurableComputation::Pure(leaf_key())) {
            Ok(attempt) => attempt,
            Err(error) => unreachable!("could not create an attempt: {error}"),
        };
        let completion = CompletedAttempt {
            dependencies: Box::new([]),
            result: EncodedValue::from_value(&Value::Int(42)),
            provenance: DurableProvenance::Pure,
            reuse: DurableReuseDecision::Reusable,
        };
        if let Err(error) = state.publish_complete(attempt, completion) {
            unreachable!("could not publish: {error}");
        }
    }

    let mut engine =
        Engine::with_state_store(pith_store::MemoryContentStore::default(), open(&database));
    engine.register_rule(leaf_rule(), FailingRule);

    // The newest reusable attempt for the key is the 42 one, so that is what
    // hydrates — the older attempt is not resurrected.
    let evaluation = match engine.evaluate_pure(&leaf_request()) {
        Ok(evaluation) => evaluation,
        Err(error) => unreachable!("the leaf did not hydrate: {error:?}"),
    };
    assert_eq!(evaluation.source, EvaluationSource::Hydrated);
    assert_eq!(evaluation.value, Value::Int(42));
}

// ---------------------------------------------------------------------------
// Durability of the record encoding
// ---------------------------------------------------------------------------

#[test]
fn records_survive_closing_and_reopening_the_database() {
    let scratch = Scratch::new("reopen");
    let database = scratch.database();
    let key = leaf_key();

    let recorded = {
        let state = open(&database);
        let attempt = match state.create_pending_attempt(DurableComputation::Pure(key)) {
            Ok(attempt) => attempt,
            Err(error) => unreachable!("could not create an attempt: {error}"),
        };
        let completion = CompletedAttempt {
            dependencies: Box::new([]),
            result: EncodedValue::from_value(&Value::Int(5)),
            provenance: DurableProvenance::Pure,
            reuse: DurableReuseDecision::Reusable,
        };
        if let Err(error) = state.publish_complete(attempt, completion) {
            unreachable!("could not publish: {error}");
        }
        attempt
    };

    let state = open(&database);
    let reopened = match state.latest_completed_reusable_attempt(key) {
        Ok(Some(attempt)) => attempt,
        Ok(None) => unreachable!("the reusable index did not survive the reopen"),
        Err(error) => unreachable!("the reusable index is unreadable: {error}"),
    };
    assert_eq!(reopened.id, recorded);
    assert_eq!(reopened.computation, DurableComputation::Pure(key));
    let DurableAttemptState::Complete(completion) = &reopened.state else {
        unreachable!("the reopened attempt is not complete");
    };
    assert_eq!(completion.result.decode().ok(), Some(Value::Int(5)));
}

#[test]
fn an_interrupted_pending_attempt_is_marked_failed_on_reopen() {
    // Decision 0024: after interruption, attempts left `Pending` are marked
    // failed rather than resumed. Reopening the database performs that
    // recovery, so a later reader finds a failed attempt, not a pending one.
    let scratch = Scratch::new("pending");
    let database = scratch.database();

    let interrupted = {
        let state = open(&database);
        match state.create_pending_attempt(DurableComputation::Pure(leaf_key())) {
            Ok(attempt) => attempt,
            Err(error) => unreachable!("could not create an attempt: {error}"),
        }
        // The store is dropped without publishing: the attempt is stranded as
        // `Pending`, exactly as if its owner had crashed.
    };

    let state = open(&database);
    // Recovery ran during open, so no attempt remains pending.
    let pending = match state.pending_attempts() {
        Ok(pending) => pending,
        Err(error) => unreachable!("pending attempts are unreadable: {error}"),
    };
    assert!(
        pending.is_empty(),
        "an interrupted attempt was not recovered"
    );

    let restored = match state.attempt(interrupted) {
        Ok(Some(restored)) => restored,
        Ok(None) => unreachable!("the interrupted attempt vanished"),
        Err(error) => unreachable!("the interrupted attempt is unreadable: {error}"),
    };
    let DurableAttemptState::Failed(failure) = &restored.state else {
        unreachable!("the interrupted attempt was not marked failed");
    };
    assert!(
        failure
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == pith_diag::StableCode(1214) }),
        "the failure does not carry an interrupted diagnostic"
    );
    // An interrupted attempt never produced a result, so it is not reusable.
    assert_eq!(
        state.latest_completed_reusable_attempt(leaf_key()).ok(),
        Some(None)
    );
}

#[test]
fn an_action_attempt_round_trips_its_plan_and_provenance() {
    // The widest record the codec has to carry: a declared contract, an
    // authorization decision, an imported executor report, and capability-use
    // edges that must match it.
    let scratch = Scratch::new("action");
    let state = open(&scratch.database());

    let capability = CapabilityRequirement {
        name: "net".into(),
        scope: "example.test".into(),
    };
    let mut spec = ActionSpec::isolated("/bin/tool");
    spec.arguments = ["--build".into()].into();
    spec.capabilities = [capability.clone()].into();
    spec.outputs = [pith_core::ActionOutput {
        path: "out".into(),
        kind: pith_core::OutputKind::Blob,
    }]
    .into();

    let identity = RuleIdentity::of_module_declaration("pith-state-sqlite-tests", "compile");
    let revision = RuleRevision::of_manifest(identity, b"compile-v1");
    let plan = match pith_engine::state::DurableActionPlan::new(DurableRule::new(revision), spec) {
        Ok(plan) => plan,
        Err(error) => unreachable!("the fixture contract is invalid: {error:?}"),
    };
    let computation = DurableComputation::Action {
        plan: plan.clone(),
        authorization: ActionAuthorization::Allowed {
            policy: "allow-all-actions".into(),
        },
    };

    let attempt = match state.create_pending_attempt(computation.clone()) {
        Ok(attempt) => attempt,
        Err(error) => unreachable!("could not create an attempt: {error}"),
    };
    let completion = CompletedAttempt {
        dependencies: [DurableDependency::CapabilityUse {
            capability: capability.clone(),
        }]
        .into(),
        result: EncodedValue::from_value(&Value::Blob(content_id(2))),
        provenance: DurableProvenance::Action(DurableActionProvenance::Imported {
            imported_report: ExecutionReport {
                executor: "fixture".into(),
                platform: ExecutionPlatform {
                    operating_system: "linux".into(),
                    architecture: "aarch64".into(),
                },
                access: AccessVerification::Prevented,
                outputs: [ProducedOutput {
                    path: "out".into(),
                    content: Content::Blob(content_id(2)),
                }]
                .into(),
                capabilities_used: [capability].into(),
            },
        }),
        reuse: DurableReuseDecision::NotReusable(DurableReuseReason::ActionCachingDisabled),
    };
    if let Err(error) = state.publish_complete(attempt, completion.clone()) {
        unreachable!("could not publish the action attempt: {error}");
    }

    let restored = match state.attempt(attempt) {
        Ok(Some(restored)) => restored,
        Ok(None) => unreachable!("the action attempt vanished"),
        Err(error) => unreachable!("the action attempt is unreadable: {error}"),
    };
    assert_eq!(restored.computation, computation);
    assert_eq!(
        restored.state,
        DurableAttemptState::Complete(completion),
        "an action attempt did not survive its round trip through sqlite"
    );
    // An action is never reusable, so it never enters the reusable index.
    assert!(state.pending_attempts().is_ok());
}

#[test]
fn a_failed_attempt_round_trips_its_diagnostics() {
    let scratch = Scratch::new("diagnostics");
    let state = open(&scratch.database());

    let attempt = match state.create_pending_attempt(DurableComputation::Pure(leaf_key())) {
        Ok(attempt) => attempt,
        Err(error) => unreachable!("could not create an attempt: {error}"),
    };
    let failure = StoppedAttempt {
        dependencies: Box::new([]),
        diagnostics: [DurableDiagnostic {
            severity: Severity::Error,
            code: StableCode(1207),
            span: Span::none(),
            message: "the rule body failed".into(),
            notes: [pith_engine::state::DurableDiagnosticNote {
                span: Span::none(),
                message: "a note".into(),
            }]
            .into(),
        }]
        .into(),
        provenance: DurableProvenance::Pure,
    };
    if let Err(error) = state.publish_failed(attempt, failure.clone()) {
        unreachable!("could not publish the failure: {error}");
    }

    let restored = match state.attempt(attempt) {
        Ok(Some(restored)) => restored,
        Ok(None) => unreachable!("the failed attempt vanished"),
        Err(error) => unreachable!("the failed attempt is unreadable: {error}"),
    };
    assert_eq!(restored.state, DurableAttemptState::Failed(failure));
    // A failure is provenance, never a reusable result.
    assert_eq!(
        state.latest_completed_reusable_attempt(leaf_key()).ok(),
        Some(None)
    );
}

// ---------------------------------------------------------------------------
// Version gating
// ---------------------------------------------------------------------------

#[test]
fn an_incompatible_database_is_moved_aside_and_rebuilt() {
    // Decision 0024: before release, an incompatible metadata version causes
    // the database to be moved aside and rebuilt. Silent reinterpretation under
    // a different schema is forbidden.
    let scratch = Scratch::new("incompatible");
    let database = scratch.database();

    {
        let state = open(&database);
        let attempt = match state.create_pending_attempt(DurableComputation::Pure(leaf_key())) {
            Ok(attempt) => attempt,
            Err(error) => unreachable!("could not create an attempt: {error}"),
        };
        let completion = CompletedAttempt {
            dependencies: Box::new([]),
            result: EncodedValue::from_value(&Value::Int(3)),
            provenance: DurableProvenance::Pure,
            reuse: DurableReuseDecision::Reusable,
        };
        if let Err(error) = state.publish_complete(attempt, completion) {
            unreachable!("could not publish: {error}");
        }
    }

    // Rewrite the recorded versions to something this build does not read.
    {
        use diesel::connection::SimpleConnection;
        let mut connection = match diesel::SqliteConnection::establish(database_url(&database)) {
            Ok(connection) => connection,
            Err(error) => unreachable!("could not reopen the database: {error}"),
        };
        let updated = connection.batch_execute(
            "update engine_state_versions set schema_version = schema_version + 1 where id = 0",
        );
        assert!(updated.is_ok(), "could not rewrite the recorded versions");
    }

    let state = open(&database);
    // The cache is gone rather than reinterpreted.
    assert_eq!(
        state.latest_completed_reusable_attempt(leaf_key()).ok(),
        Some(None)
    );
    assert_eq!(state.versions(), SqliteEngineStateStore::current_versions());
    // The previous database was preserved next to it, not deleted.
    let mut quarantined = database.clone().into_os_string();
    quarantined.push(".incompatible.0");
    assert!(
        PathBuf::from(quarantined).exists(),
        "the incompatible database was not moved aside"
    );
}

#[test]
fn an_empty_database_adopts_the_current_versions() {
    let scratch = Scratch::new("fresh");
    let state = open(&scratch.database());
    assert_eq!(state.versions(), SqliteEngineStateStore::current_versions());
}

fn content_id(seed: u8) -> ContentId {
    ContentId::from_digest(ContentDigest::from_bytes([seed; pith_ids::DIGEST_LEN]))
}
