use super::*;

#[test]
fn an_identical_action_is_served_from_the_reusable_index() {
    let mut engine = fixture_engine();
    let action_interface = interface(&[Type::Int], Type::Blob);
    let root_interface = interface(&[], Type::Blob);
    engine.register_action_rule(
        action_rule("double", action_interface.clone()),
        DoubleAction,
    );
    engine.register_rule(
        pure_rule("entry", root_interface.clone()),
        ActionDepRule {
            dependency: action_request("double", action_interface, [Value::Int(21)]),
        },
    );
    let executions = Arc::new(AtomicUsize::new(0));
    let executor = CountingExecutor {
        executions: executions.clone(),
    };
    let root_request = pure_request("entry", root_interface, []);

    let first_runtime_result = engine.run(&root_request, &runtime(), &AllowAllActions, &executor);
    assert!(matches!(&first_runtime_result, Ok(Ok(_))));
    let first_evaluation_result = first_runtime_result.unwrap();
    let first = first_evaluation_result.unwrap();

    let second_runtime_result = engine.run(&root_request, &runtime(), &AllowAllActions, &executor);
    assert!(matches!(&second_runtime_result, Ok(Ok(_))));
    let second_evaluation_result = second_runtime_result.unwrap();
    let second = second_evaluation_result.unwrap();

    // The consumer is served whole (decision 0033): revalidating its action
    // edge re-selects the rule, re-plans the recorded request, derives the key
    // it was recorded under, and finds the attempt still admissible. Its rule
    // body does not run a second time, and neither does the action.
    assert_eq!(first.source, EvaluationSource::Computed);
    assert_eq!(second.source, EvaluationSource::Reused);
    assert_eq!(first.computation, second.computation);
    assert_eq!(first.value, second.value);
    assert_eq!(executions.load(Ordering::Relaxed), 1);

    let action_computation = action_dependency_of(&engine, first.computation);
    let action_state = &engine
        .query()
        .computation(action_computation)
        .unwrap()
        .state;
    assert!(matches!(
        action_state,
        AttemptState::Complete {
            reuse: ReuseDecision::Reusable,
            ..
        }
    ));
    assert!(matches!(
        &engine.query().computation(first.computation).unwrap().state,
        AttemptState::Complete {
            reuse: ReuseDecision::Reusable,
            ..
        }
    ));
}

/// The consumer of an action is revalidated against a re-plan, so a contract
/// that changed for a reason the consumer's own key cannot see makes the
/// consumer dirty (decision 0033). `HeldStateAction` plans from state it holds
/// rather than from the request, which is the shape of a compile rule holding a
/// header: the header is not a request input and participates in no rule
/// revision, so only the planned contract's digest records it.
#[test]
fn a_consumer_reruns_when_its_action_replans_a_different_contract() {
    let mut engine = fixture_engine();
    let action_interface = interface(&[Type::Int], Type::Blob);
    let root_interface = interface(&[], Type::Blob);
    let held = Arc::new(AtomicI64::new(21));
    engine.register_action_rule(
        action_rule("double", action_interface.clone()),
        HeldStateAction {
            argument: held.clone(),
        },
    );
    engine.register_rule(
        pure_rule("entry", root_interface.clone()),
        ActionDepRule {
            dependency: action_request("double", action_interface, [Value::Int(21)]),
        },
    );
    let executions = Arc::new(AtomicUsize::new(0));
    let executor = CountingExecutor {
        executions: executions.clone(),
    };
    let root_request = pure_request("entry", root_interface, []);

    let computed = engine
        .run(&root_request, &runtime(), &AllowAllActions, &executor)
        .unwrap()
        .unwrap();
    assert_eq!(computed.source, EvaluationSource::Computed);
    assert_eq!(executions.load(Ordering::Relaxed), 1);

    // Nothing in the consumer's key changed, and nothing in the request did
    // either. The plan is the only place the difference is visible.
    held.store(22, Ordering::Relaxed);
    let rerun = engine
        .run(&root_request, &runtime(), &AllowAllActions, &executor)
        .unwrap()
        .unwrap();

    assert_eq!(rerun.source, EvaluationSource::Computed);
    assert_ne!(rerun.computation, computed.computation);
    assert_eq!(
        executions.load(Ordering::Relaxed),
        2,
        "the re-planned contract has no recorded attempt, so the action runs"
    );
}

/// A run with no policy and no executor cannot revalidate an action edge, so
/// [`Engine::evaluate_pure`] declines a result that rests on one. It refuses to
/// take an effectful step for the same reason.
#[test]
fn a_pure_only_evaluation_will_not_serve_a_consumer_of_an_action() {
    let mut engine = fixture_engine();
    let action_interface = interface(&[Type::Int], Type::Blob);
    let root_interface = interface(&[], Type::Blob);
    engine.register_action_rule(
        action_rule("double", action_interface.clone()),
        DoubleAction,
    );
    engine.register_rule(
        pure_rule("entry", root_interface.clone()),
        ActionDepRule {
            dependency: action_request("double", action_interface, [Value::Int(21)]),
        },
    );
    let executions = Arc::new(AtomicUsize::new(0));
    let executor = CountingExecutor {
        executions: executions.clone(),
    };
    let root_request = pure_request("entry", root_interface, []);
    let computed = engine
        .run(&root_request, &runtime(), &AllowAllActions, &executor)
        .unwrap()
        .unwrap();
    assert_eq!(computed.source, EvaluationSource::Computed);

    // The completed root is reusable and its action edge revalidates under a
    // run. Reaching it through the pure entry point instead runs the body,
    // which stops at `NeedAction` and is rejected.
    let diagnostics = engine.evaluate_pure(&root_request).unwrap_err();

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == pith_diag::EngineCode::EffectfulStepInPure.into())
    );
}

#[test]
fn an_engine_with_caching_off_executes_every_action() {
    let mut engine = fixture_engine();
    engine.set_action_caching(false);
    let action_interface = interface(&[Type::Int], Type::Blob);
    let root_interface = interface(&[], Type::Blob);
    engine.register_action_rule(
        action_rule("double", action_interface.clone()),
        DoubleAction,
    );
    engine.register_rule(
        pure_rule("entry", root_interface.clone()),
        ActionDepRule {
            dependency: action_request("double", action_interface, [Value::Int(21)]),
        },
    );
    let executions = Arc::new(AtomicUsize::new(0));
    let executor = CountingExecutor {
        executions: executions.clone(),
    };
    let root_request = pure_request("entry", root_interface, []);

    let first = engine
        .run(&root_request, &runtime(), &AllowAllActions, &executor)
        .unwrap()
        .unwrap();
    engine
        .run(&root_request, &runtime(), &AllowAllActions, &executor)
        .unwrap()
        .unwrap();

    assert_eq!(executions.load(Ordering::Relaxed), 2);
    let action_computation = action_dependency_of(&engine, first.computation);
    assert!(matches!(
        &engine
            .query()
            .computation(action_computation)
            .unwrap()
            .state,
        AttemptState::Complete {
            reuse: ReuseDecision::NotReusable(ReuseReason::ActionCachingDisabled),
            ..
        }
    ));
}

/// The computation the first action edge of `parent` points at.
fn action_dependency_of(
    engine: &Engine,
    parent: pith_ids::ComputationId,
) -> pith_ids::ComputationId {
    engine
        .query()
        .dependencies_of(parent)
        .and_then(|dependencies| dependencies.first())
        .and_then(pith_engine::DependencyEdge::computation_id)
        .unwrap_or_else(|| unreachable!("{parent:?} records no action dependency"))
}

#[test]
fn distinct_parents_share_one_action_result() {
    let mut engine = fixture_engine();
    let action_interface = interface(&[Type::Int], Type::Blob);
    let boolean_parent_interface = interface(&[Type::Bool], Type::Blob);
    let text_parent_interface = interface(&[Type::Text], Type::Blob);
    let dependency = action_request("double", action_interface.clone(), [Value::Int(21)]);
    engine.register_action_rule(action_rule("double", action_interface), DoubleAction);
    engine.register_rule(
        pure_rule("boolean parent", boolean_parent_interface.clone()),
        ActionDepRule {
            dependency: dependency.clone(),
        },
    );
    engine.register_rule(
        pure_rule("text parent", text_parent_interface.clone()),
        ActionDepRule { dependency },
    );
    let executions = Arc::new(AtomicUsize::new(0));
    let executor = CountingExecutor {
        executions: executions.clone(),
    };

    let boolean_runtime_result = engine.run(
        &pure_request(
            "boolean parent",
            boolean_parent_interface,
            [Value::Bool(true)],
        ),
        &runtime(),
        &AllowAllActions,
        &executor,
    );
    assert!(matches!(&boolean_runtime_result, Ok(Ok(_))));
    let boolean_evaluation_result = boolean_runtime_result.unwrap();
    let boolean_parent = boolean_evaluation_result.unwrap();

    let text_runtime_result = engine.run(
        &pure_request(
            "text parent",
            text_parent_interface,
            [Value::Text("input".into())],
        ),
        &runtime(),
        &AllowAllActions,
        &executor,
    );
    assert!(matches!(&text_runtime_result, Ok(Ok(_))));
    let text_evaluation_result = text_runtime_result.unwrap();
    let text_parent = text_evaluation_result.unwrap();

    let boolean_action = engine
        .query()
        .dependencies_of(boolean_parent.computation)
        .and_then(|dependencies| dependencies.first())
        .and_then(pith_engine::DependencyEdge::computation_id)
        .unwrap();
    let text_action = engine
        .query()
        .dependencies_of(text_parent.computation)
        .and_then(|dependencies| dependencies.first())
        .and_then(pith_engine::DependencyEdge::computation_id)
        .unwrap();

    // The action key comes from the action request and the contract it plans,
    // so two parents asking for one action share its attempt (decision 0031).
    assert_eq!(boolean_parent.source, EvaluationSource::Computed);
    assert_eq!(text_parent.source, EvaluationSource::Computed);
    assert_eq!(boolean_action, text_action);
    assert_eq!(executions.load(Ordering::Relaxed), 1);
}

#[test]
fn an_action_below_the_confinement_floor_is_not_reused() {
    let mut engine = fixture_engine();
    // `UnverifiedCountingExecutor` reports `Unverified`, which this floor
    // refuses (decision 0031).
    engine.set_minimum_access_verification(AccessVerification::Observed);
    let action_interface = interface(&[Type::Int], Type::Blob);
    let root_interface = interface(&[], Type::Blob);
    engine.register_action_rule(
        action_rule("double", action_interface.clone()),
        DoubleAction,
    );
    engine.register_rule(
        pure_rule("entry", root_interface.clone()),
        ActionDepRule {
            dependency: action_request("double", action_interface, [Value::Int(21)]),
        },
    );
    let executions = Arc::new(AtomicUsize::new(0));
    let executor = UnverifiedCountingExecutor {
        executions: executions.clone(),
    };
    let root_request = pure_request("entry", root_interface, []);

    let first_runtime_result = engine.run(&root_request, &runtime(), &AllowAllActions, &executor);
    assert!(matches!(&first_runtime_result, Ok(Ok(_))));
    let first_evaluation_result = first_runtime_result.unwrap();
    let first = first_evaluation_result.unwrap();

    let second_runtime_result = engine.run(&root_request, &runtime(), &AllowAllActions, &executor);
    assert!(matches!(&second_runtime_result, Ok(Ok(_))));
    let second_evaluation_result = second_runtime_result.unwrap();
    let second = second_evaluation_result.unwrap();

    assert_eq!(first.source, EvaluationSource::Computed);
    assert_eq!(second.source, EvaluationSource::Computed);
    assert_ne!(first.computation, second.computation);
    assert_eq!(executions.load(Ordering::Relaxed), 2);
}

#[test]
fn effectful_step_in_pure_only_evaluation_is_rejected() {
    let mut engine = fixture_engine();
    let blob_id = engine.put_blob(b"x").unwrap();
    engine.register_rule(
        pure_rule("length", interface(&[], Type::Int)),
        BlobLenRule { blob: blob_id },
    );

    let err = engine
        .evaluate_pure(&pure_request("length", interface(&[], Type::Int), []))
        .unwrap_err();
    let diag = err.iter().next().unwrap();
    assert_eq!(diag.code, StableCode::from(EngineCode::EffectfulStepInPure));
}
