use super::*;

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
