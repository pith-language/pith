//! Resolution as a computation in the graph (decision 0040): the solver is
//! an ordinary pure rule selected by 0015, its inputs — the declared version
//! ordering, the constraint set, the candidate universe, the preference
//! list, the budget — are values that participate in the computation key,
//! and the reusable index serves and invalidates resolutions through the
//! machinery that already exists. These tests measure that; the solver's own
//! behavior is pinned in the module tests under `src/`.

#[path = "support/engine_state.rs"]
mod engine_state_support;

use engine_state_support::SharedState;
use phloem::constraint::{Bound, Constraint, Range, constraint_set_value};
use phloem::identity::{
    DEBIAN, DomainIdentity, NUMERIC_SEGMENTS, PackageIdentity, version_scheme_value,
};
use phloem::lock::Origin;
use phloem::preference::{Preference, PreferenceList, preference_list_value};
use phloem::resolution::{RESOLUTION, Resolution, resolve_interface, resolve_request};
use phloem::resolve::{ResolveSolver, Schemes};
use phloem::source::SourceBinding;
use phloem::universe::{Candidate, CandidateUniverse};
use pith_core::{Interface, Pure, Request, Type, Value};
use pith_diag::{DiagnosticSink, PithResult};
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
        origin: Origin::Registry("pkgs.pith-lang.org".into()),
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
    ResolveSolver::new(Schemes::standard())
}

fn numeric_scheme() -> Value {
    version_scheme_value(NUMERIC_SEGMENTS)
}

fn engine_with(state: &SharedState) -> Engine {
    let mut engine = Engine::with_state_store(MemoryContentStore::default(), state.clone());
    let solver = solver();
    engine.register_rule(solver.rule(), solver);
    engine
}

fn request_over(
    scheme: &Value,
    constraints: &Value,
    universe: &Value,
    preferences: &Value,
    budget: u64,
) -> Request<Pure> {
    resolve_request(scheme, constraints, universe, preferences, budget)
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
        &numeric_scheme(),
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
        &numeric_scheme(),
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
    let old_request = request_over(
        &numeric_scheme(),
        &constraints,
        &before.to_value(),
        &preferences_newest(),
        100,
    );
    let new_request = request_over(
        &numeric_scheme(),
        &constraints,
        &after.to_value(),
        &preferences_newest(),
        100,
    );

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

    // Every input but the universe is unchanged by value, so the universe
    // is the one that moved; and the recorded answer for the old key still
    // serves, which is what reproducible-under-the-same-universe means.
    for unchanged in [0, 1, 3, 4] {
        assert_eq!(
            old_request.inputs.get(unchanged).unwrap(),
            new_request.inputs.get(unchanged).unwrap()
        );
    }
    assert_ne!(
        old_request.inputs.get(2).unwrap(),
        new_request.inputs.get(2).unwrap()
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
    let request = request_over(
        &numeric_scheme(),
        &constraints,
        &universe.to_value(),
        &preferences_newest(),
        0,
    );

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
        &numeric_scheme(),
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
        [Value::int(1)],
        pith_diag::Span::none(),
    );
    assert!(mismatched.validate_inputs().is_ok());
    assert!(query.select(&mismatched).is_err());
    let _ = RESOLUTION;
}

#[test]
fn a_different_declared_ordering_is_a_different_computation() {
    // The scheme input closes the hole 0038's third class of state names:
    // the ordering a resolution ran under is part of the computation key, so
    // an answer recorded under one declared ordering is never served under
    // another. The two spellings below order differently under the two
    // schemes — tilde is lowest for Debian, a non-numeric segment sorts
    // above a numeric one for the numeric scheme — so `newest` picks
    // different winners, and neither engine sees the other's answer.
    let state = SharedState::default();
    let universe =
        CandidateUniverse::new(vec![candidate("zlib", "1.0"), candidate("zlib", "1.0~rc1")]);
    let constraints = constraint_set_value(&[zlib_constraint()]);

    let mut first = engine_with(&state);
    let numeric_answer = first
        .evaluate_pure(&request_over(
            &numeric_scheme(),
            &constraints,
            &universe.to_value(),
            &preferences_newest(),
            100,
        ))
        .unwrap();
    let Resolution::Solved {
        choice: numeric_choice,
        ..
    } = Resolution::from_value(&numeric_answer.value).unwrap()
    else {
        unreachable!("the numeric scheme resolves");
    };
    assert_eq!(
        numeric_choice.first().unwrap().version,
        Box::from("1.0~rc1"),
        "a non-numeric segment sorts above a numeric one under the numeric scheme"
    );

    // A fresh engine over the same durable substrate: the debian-keyed
    // request has no recorded answer under its key, so it computes, while
    // the numeric-keyed one is served from the reusable index — hydration,
    // not re-evaluation, because this engine's arena has never seen the key.
    let mut second = engine_with(&state);
    let deb_answer = second
        .evaluate_pure(&request_over(
            &version_scheme_value(DEBIAN),
            &constraints,
            &universe.to_value(),
            &preferences_newest(),
            100,
        ))
        .unwrap();
    assert_eq!(
        deb_answer.source,
        EvaluationSource::Computed,
        "an answer under one ordering must not be served under another"
    );
    let Resolution::Solved {
        choice: deb_choice, ..
    } = Resolution::from_value(&deb_answer.value).unwrap()
    else {
        unreachable!("the debian scheme resolves");
    };
    assert_eq!(
        deb_choice.first().unwrap().version,
        Box::from("1.0"),
        "tilde sorts lowest under the debian scheme"
    );
    let served = second
        .evaluate_pure(&request_over(
            &numeric_scheme(),
            &constraints,
            &universe.to_value(),
            &preferences_newest(),
            100,
        ))
        .unwrap();
    assert_eq!(served.source, EvaluationSource::Hydrated);
    assert_eq!(served.value, numeric_answer.value);
}
