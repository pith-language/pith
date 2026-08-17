use super::*;

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
