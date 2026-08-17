use super::*;

#[test]
fn durable_reuse_is_valid_until_a_dependency_result_identity_changes() {
    let mut engine = engine_with_fixtures();
    let leaf = interface(&[], Type::Int);
    let root = interface(&[Type::Bool], Type::Int);
    engine.register_rule(pure_rule("leaf", leaf.clone()), ConstantRule(Value::int(1)));
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
                result: EncodedValue::from_value(&Value::int(99)),
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
    engine.register_rule(pure_rule("leaf", leaf.clone()), ConstantRule(Value::int(1)));
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
                result: EncodedValue::from_value(&Value::int(1)),
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
    engine.register_rule(pure_rule("leaf", leaf.clone()), ConstantRule(Value::int(1)));
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
