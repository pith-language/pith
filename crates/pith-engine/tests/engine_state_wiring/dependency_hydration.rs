use super::*;

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
