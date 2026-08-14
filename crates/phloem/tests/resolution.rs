//! Resolution as a computation in the graph (decision 0040): the solver is
//! an ordinary pure rule selected by 0015, its four inputs are values that
//! participate in the computation key, and the reusable index serves and
//! invalidates resolutions through the machinery that already exists. These
//! tests measure that; the solver's own behavior is pinned in the module
//! tests under `src/`.

use std::sync::{Arc, Mutex};

use phloem::constraint::{Bound, Constraint, Range, constraint_set_value};
use phloem::identity::{DomainIdentity, NumericSegments, PackageIdentity};
use phloem::preference::{Preference, PreferenceList, preference_list_value};
use phloem::resolve::{RESOLUTION, Resolution, ResolveSolver, resolve_interface, resolve_request};
use phloem::source::SourceBinding;
use phloem::universe::{Candidate, CandidateUniverse};
use pith_core::ActionComputationKey;
use pith_core::{Interface, Pure, PureComputationKey, Request, Type, Value};
use pith_diag::{DiagnosticSink, PithResult};
use pith_engine::state::{
    CompletedAttempt, DurableAttempt, DurableAttemptId, DurableComputation, EngineStateError,
    EngineStateStore, EngineStateVersions, InvalidationExplanation, MemoryEngineStateStore,
    StoppedAttempt,
};
use pith_engine::{Engine, EvaluationSource, PureRule, PureRuleFrame, PureStep, Resumption};
use pith_ids::ContentId;
use pith_store::MemoryContentStore;

fn identity(name: &str) -> PackageIdentity {
    PackageIdentity::declare(DomainIdentity::new("pithpkgs"), name)
}

fn candidate(name: &str, version: &str) -> Candidate {
    Candidate {
        identity: identity(name),
        version: version.into(),
        features: Box::new([]),
        provenance: SourceBinding::Archive {
            archive: ContentId::of_blob(format!("{name}-{version}").as_bytes()),
        },
        requires: Box::new([]),
    }
}

fn root_constraint(name: &str, range: Range) -> Constraint {
    Constraint {
        subject: identity(name),
        range,
        features: Box::new([]),
        attribution: "root".into(),
    }
}

fn zlib_constraint() -> Constraint {
    root_constraint("zlib", Range::AtLeast(Bound::new("1.0", true)))
}

fn preferences_newest() -> Value {
    preference_list_value(&PreferenceList(Box::new([Preference::Newest])))
}

fn solver() -> ResolveSolver {
    ResolveSolver::new(NumericSegments)
}

fn engine_with(state: &SharedState) -> Engine {
    let mut engine = Engine::with_state_store(MemoryContentStore::default(), state.clone());
    let solver = solver();
    engine.register_rule(solver.rule(), solver);
    engine
}

fn request_over(
    constraints: &Value,
    universe: &Value,
    preferences: &Value,
    budget: u64,
) -> Request<Pure> {
    resolve_request(constraints, universe, preferences, budget)
}

/// One durable substrate behind several engines, the arrangement 0024
/// describes and the only way hydration is observable.
#[derive(Clone, Default)]
struct SharedState(Arc<Mutex<MemoryEngineStateStore>>);

impl SharedState {
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

impl EngineStateStore for SharedState {
    fn versions(&self) -> EngineStateVersions {
        match self.0.lock() {
            Ok(store) => store.versions(),
            Err(_) => unreachable!("the shared engine state lock was poisoned"),
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

/// A failing body, registered in the place of the solver in engines that must
/// not run it: reaching a value at all proves the result came from state.
struct FailingRule;

impl PureRule for FailingRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(FailingFrame)
    }
}

struct FailingFrame;

impl PureRuleFrame for FailingFrame {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        let mut sink = DiagnosticSink::new();
        sink.push(pith_diag::Diag::new(
            pith_diag::Severity::Error,
            pith_diag::StableCode(0),
            pith_diag::Span::none(),
            "the solver body ran where a served result was expected",
        ));
        Err(sink)
    }
}

#[test]
fn a_resolution_computes_then_serves_from_the_arena_index() {
    let mut engine = engine_with(&SharedState::default());
    let universe = CandidateUniverse::new(vec![candidate("zlib", "1.2"), candidate("zlib", "1.3")]);
    let constraints = constraint_set_value(&[zlib_constraint()]);
    let request = request_over(
        &constraints,
        &universe.to_value(),
        &preferences_newest(),
        100,
    );

    let computed = engine.evaluate_pure(&request).unwrap();
    assert_eq!(computed.source, EvaluationSource::Computed);
    let resolution = Resolution::from_value(&computed.value).unwrap();
    let Resolution::Solved { choice, .. } = &resolution else {
        unreachable!("expected a solved resolution, found {resolution:?}");
    };
    assert_eq!(choice.first().unwrap().version, Box::from("1.3"));

    let reused = engine.evaluate_pure(&request).unwrap();
    assert_eq!(reused.source, EvaluationSource::Reused);
    assert_eq!(reused.value, computed.value);
}

#[test]
fn a_resolution_hydrates_into_a_fresh_engine_over_the_same_state() {
    let state = SharedState::default();
    let universe = CandidateUniverse::new(vec![candidate("zlib", "1.3")]);
    let constraints = constraint_set_value(&[zlib_constraint()]);
    let request = request_over(
        &constraints,
        &universe.to_value(),
        &preferences_newest(),
        100,
    );

    let mut first = engine_with(&state);
    let computed = first.evaluate_pure(&request).unwrap();
    assert_eq!(computed.source, EvaluationSource::Computed);

    let mut second = Engine::with_state_store(MemoryContentStore::default(), state.clone());
    let solver = solver();
    second.register_rule(solver.rule(), FailingRule);
    let hydrated = second.evaluate_pure(&request).unwrap();
    assert_eq!(hydrated.source, EvaluationSource::Hydrated);
    assert_eq!(hydrated.value, computed.value);
}

#[test]
fn a_changed_candidate_universe_is_a_new_computation_and_the_old_answer_stands() {
    // The four inputs are values in the computation key, so a changed
    // universe is not an invalidation the engine explains; it is a different
    // computation. What the engine guarantees is the other half: the old
    // answer is still served for the old key, and the new answer names the
    // universe it resolved against — the digest a lock diff reads to say
    // which input moved.
    let state = SharedState::default();
    let before = CandidateUniverse::new(vec![candidate("zlib", "1.3")]);
    let after = CandidateUniverse::new(vec![candidate("zlib", "1.3.1")]);
    let constraints = constraint_set_value(&[zlib_constraint()]);
    let old_request = request_over(&constraints, &before.to_value(), &preferences_newest(), 100);
    let new_request = request_over(&constraints, &after.to_value(), &preferences_newest(), 100);

    let mut first = engine_with(&state);
    first.evaluate_pure(&old_request).unwrap();
    let moved = first.evaluate_pure(&new_request).unwrap();
    assert_eq!(
        moved.source,
        EvaluationSource::Computed,
        "a changed universe must not be served the old answer"
    );
    let Resolution::Solved { universe, .. } = Resolution::from_value(&moved.value).unwrap() else {
        unreachable!("the moved universe still resolves");
    };
    assert_eq!(universe, after.content_id());

    // The other three inputs are unchanged by value, so the universe is the
    // one input that moved; and the recorded answer for the old key still
    // serves, which is what reproducible-under-the-same-universe means.
    assert_eq!(
        old_request.inputs.first().unwrap(),
        new_request.inputs.first().unwrap()
    );
    assert_eq!(
        old_request.inputs.get(2).unwrap(),
        new_request.inputs.get(2).unwrap()
    );
    assert_eq!(
        old_request.inputs.get(3).unwrap(),
        new_request.inputs.get(3).unwrap()
    );
    let mut second = Engine::with_state_store(MemoryContentStore::default(), state.clone());
    let solver = solver();
    second.register_rule(solver.rule(), FailingRule);
    let served = second.evaluate_pure(&old_request).unwrap();
    assert_eq!(served.source, EvaluationSource::Hydrated);
}

#[test]
fn a_budget_exhausted_answer_is_a_function_of_the_inputs_and_is_served_from_the_index() {
    // 0040's determinism clause, made concrete: the budget is a declared
    // input in deterministic units, so an exhausted answer caches like any
    // other — the refusal a second run receives is a recorded answer, not a
    // re-run that might now succeed.
    let mut engine = engine_with(&SharedState::default());
    let universe = CandidateUniverse::new(vec![
        candidate("a", "1.0"),
        candidate("a", "1.1"),
        candidate("b", "1.5"),
    ]);
    let constraints = constraint_set_value(&[
        root_constraint("a", Range::Any),
        root_constraint("b", Range::AtLeast(Bound::new("2.0", true))),
    ]);
    let request = request_over(&constraints, &universe.to_value(), &preferences_newest(), 0);

    let computed = engine.evaluate_pure(&request).unwrap();
    let resolution = Resolution::from_value(&computed.value).unwrap();
    let Resolution::BudgetExhausted { budget, decisions } = resolution else {
        unreachable!("a budget of zero decisions is exhaustion, found {resolution:?}");
    };
    assert_eq!(budget, 0);
    assert!(decisions > budget);

    let reused = engine.evaluate_pure(&request).unwrap();
    assert_eq!(reused.source, EvaluationSource::Reused);
    assert_eq!(reused.value, computed.value);
}

#[test]
fn the_resolution_interface_selects_exactly_one_rule() {
    // Resolution rides 0015 like any other request: the interface names it,
    // a request against it validates, and registration is what makes it
    // available — no pre-pass, no special entry point.
    let engine = engine_with(&SharedState::default());
    let query = engine.query();
    let universe = CandidateUniverse::new(vec![candidate("zlib", "1.3")]);
    let request = request_over(
        &constraint_set_value(&[zlib_constraint()]),
        &universe.to_value(),
        &preferences_newest(),
        100,
    );
    let selection = query.select(&request).unwrap();
    assert_eq!(selection.interface, resolve_interface());

    let mismatched = Request::<Pure>::new(
        "resolve",
        Interface {
            inputs: Box::new([Type::Int]),
            output: Type::Int,
        },
        [Value::Int(1)],
        pith_diag::Span::none(),
    );
    assert!(mismatched.validate_inputs().is_ok());
    assert!(query.select(&mismatched).is_err());
    let _ = RESOLUTION;
}
