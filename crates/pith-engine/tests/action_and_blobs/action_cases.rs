use super::*;

#[test]
fn action_dependency_driven_through_run() {
    let mut engine = fixture_engine();
    let action_iface = interface(&[Type::Int], Type::Blob);
    let pure_iface = interface(&[], Type::Blob);
    engine.register_action_rule(action_rule("double", action_iface.clone()), DoubleAction);
    engine.register_rule(
        pure_rule("entry", pure_iface.clone()),
        ActionDepRule {
            dependency: action_request("double", action_iface, [Value::Int(21)]),
        },
    );

    let plan = engine
        .query()
        .plan_action(&action_request(
            "double",
            interface(&[Type::Int], Type::Blob),
            [Value::Int(21)],
        ))
        .unwrap();
    assert_eq!(plan.spec.executable.host_path(), Some(double_executable()));
    assert_eq!(plan.spec_digest, plan.spec.digest().unwrap());
    assert_eq!(
        plan.spec.capabilities.as_ref(),
        declared_double_capabilities().as_slice()
    );
    assert_eq!(
        plan.spec
            .arguments
            .first()
            .map(|argument| argument.as_ref()),
        Some("21")
    );

    let evaluation = engine
        .run(
            &pure_request("entry", pure_iface, []),
            &runtime(),
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap()
        .unwrap();

    assert_eq!(evaluation.value, Value::Blob(double_result(21)));
    let deps = engine
        .query()
        .dependencies_of(evaluation.computation)
        .unwrap();
    let action_computation = deps
        .first()
        .and_then(|edge| match edge {
            pith_engine::DependencyEdge::Action { computation, .. } => Some(*computation),
            _ => None,
        })
        .unwrap();
    let action = engine
        .query()
        .computation(action_computation)
        .and_then(|node| node.action.as_ref())
        .unwrap();
    assert_eq!(action.spec_digest, action.spec.digest().unwrap());
    assert_eq!(
        action.spec.executable.host_path(),
        Some(double_executable())
    );
    assert_eq!(
        action.authorization,
        ActionAuthorization::Allowed {
            policy: "allow-all-actions".into(),
        }
    );
    assert_eq!(
        action.imported_report.as_ref().map(|report| report.access),
        Some(AccessVerification::Prevented)
    );
    assert_eq!(
        engine.query().capabilities_of(action_computation),
        Some(effective_double_capabilities().as_slice())
    );
    assert_eq!(
        engine.query().capabilities_of(evaluation.computation),
        Some(effective_double_capabilities().as_slice())
    );
}

#[test]
fn actual_capability_uses_are_dependency_edges() {
    let mut engine = fixture_engine();
    let action_iface = interface(&[Type::Int], Type::Blob);
    let pure_iface = interface(&[], Type::Blob);
    engine.register_action_rule(action_rule("double", action_iface.clone()), DoubleAction);
    engine.register_rule(
        pure_rule("entry", pure_iface.clone()),
        ActionDepRule {
            dependency: action_request("double", action_iface, [Value::Int(21)]),
        },
    );

    let evaluation = match engine.run(
        &pure_request("entry", pure_iface, []),
        &runtime(),
        &AllowAllActions,
        &ObservedCapabilityExecutor,
    ) {
        Ok(Ok(evaluation)) => evaluation,
        Ok(Err(_)) => unreachable!("observed capability fixture failed evaluation"),
        Err(_) => unreachable!("observed capability fixture failed to drive runtime"),
    };

    let query = engine.query();
    let Some(action_computation) = query
        .dependencies_of(evaluation.computation)
        .and_then(|dependencies| dependencies.first())
        .and_then(pith_engine::DependencyEdge::computation_id)
    else {
        unreachable!("pure evaluation has no action dependency");
    };
    let Some(uses) = query.capability_uses_of(action_computation) else {
        unreachable!("action computation is missing");
    };
    let uses: Vec<_> = uses.cloned().collect();
    let Some(mut parent_uses) = query.capability_uses_of(evaluation.computation) else {
        unreachable!("pure computation is missing");
    };

    assert_eq!(uses, [double_capability()]);
    assert!(parent_uses.next().is_none());
    assert_eq!(
        query.capabilities_of(evaluation.computation),
        Some(effective_double_capabilities().as_slice())
    );
}

#[test]
fn action_output_bytes_are_imported_by_the_engine() {
    let mut engine = fixture_engine();
    let action_iface = interface(&[Type::Int], Type::Blob);
    let pure_iface = interface(&[], Type::Blob);
    engine.register_action_rule(action_rule("double", action_iface.clone()), DoubleAction);
    engine.register_rule(
        pure_rule("entry", pure_iface.clone()),
        ActionDepRule {
            dependency: action_request("double", action_iface, [Value::Int(21)]),
        },
    );

    let action_evaluation = engine
        .run(
            &pure_request("entry", pure_iface, []),
            &runtime(),
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap()
        .unwrap();
    let Value::Blob(output) = action_evaluation.value else {
        unreachable!("fixture action returns a blob")
    };

    engine.register_rule(
        pure_rule("length", interface(&[], Type::Int)),
        BlobLenRule { blob: output },
    );
    let bytes_evaluation = engine
        .run(
            &pure_request("length", interface(&[], Type::Int), []),
            &runtime(),
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap()
        .unwrap();

    assert_eq!(output, double_result(21));
    assert_eq!(bytes_evaluation.value, Value::Int(2));
}

#[test]
fn missing_action_input_is_rejected_before_executor_call() {
    let mut engine = Engine::new();
    // The declared input is deliberately not stored; the engine must reject the
    // action before the executor runs. The executable is a host path (0030) and
    // needs no store entry.
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
    let executor = NeverExecutor {
        executions: executions.clone(),
    };

    let diagnostics = engine
        .run(
            &pure_request("entry", root_interface, []),
            &runtime(),
            &AllowAllActions,
            &executor,
        )
        .unwrap()
        .unwrap_err();
    let diagnostic = diagnostics.iter().next().unwrap();

    assert_eq!(
        diagnostic.code,
        StableCode::from(EngineCode::ContentUnavailable)
    );
    assert_eq!(executions.load(Ordering::Relaxed), 0);
    assert_no_pending_attempts(&engine);

    let query = engine.query();
    let (action_computation, action_node) = query
        .computations()
        .find(|(_, node)| matches!(node.kind, ComputationKind::Action(_)))
        .unwrap();
    assert!(matches!(action_node.state, AttemptState::Failed { .. }));
    let action_record = action_node.action.as_ref().unwrap();
    assert!(action_record.executor_report.is_none());
    assert!(action_record.imported_report.is_none());
    let parent = query
        .computations()
        .find(|(_, node)| matches!(node.kind, ComputationKind::Pure(_)))
        .map(|(_, node)| node)
        .unwrap();
    assert!(matches!(parent.state, AttemptState::Failed { .. }));
    assert!(parent.dependencies.iter().any(|dependency| matches!(
        dependency,
        pith_engine::DependencyEdge::Action { computation, .. }
            if *computation == action_computation
    )));
}

#[test]
fn planner_failure_creates_no_action_attempt() {
    let mut engine = Engine::new();
    let action_interface = interface(&[], Type::Unit);
    let root_interface = interface(&[], Type::Unit);
    engine.register_action_rule(
        action_rule("failing planner", action_interface.clone()),
        FailingPlanner,
    );
    engine.register_rule(
        pure_rule("entry", root_interface.clone()),
        ActionDepRule {
            dependency: action_request("failing planner", action_interface, []),
        },
    );

    let diagnostics = engine
        .run(
            &pure_request("entry", root_interface, []),
            &runtime(),
            &AllowAllActions,
            &NeverExecutor {
                executions: Arc::new(AtomicUsize::new(0)),
            },
        )
        .unwrap()
        .unwrap_err();

    assert_eq!(
        diagnostics
            .iter()
            .next()
            .map(|diagnostic| diagnostic.message.0.as_ref()),
        Some("action planning failed")
    );
    assert_no_pending_attempts(&engine);
    assert!(
        engine
            .query()
            .computations()
            .all(|(_, node)| !matches!(node.kind, ComputationKind::Action(_)))
    );
}

#[test]
fn executor_failure_finalizes_action_and_parent_without_a_report() {
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

    let diagnostics = engine
        .run(
            &pure_request("entry", root_interface, []),
            &runtime(),
            &AllowAllActions,
            &FailingExecutor,
        )
        .unwrap()
        .unwrap_err();

    assert_eq!(
        diagnostics
            .iter()
            .next()
            .map(|diagnostic| diagnostic.message.0.as_ref()),
        Some("executor failed")
    );
    assert_no_pending_attempts(&engine);
    let action = engine
        .query()
        .computations()
        .find(|(_, node)| matches!(node.kind, ComputationKind::Action(_)))
        .map(|(_, node)| node)
        .unwrap();
    assert!(matches!(action.state, AttemptState::Failed { .. }));
    let action_record = action.action.as_ref().unwrap();
    assert!(action_record.executor_report.is_none());
    assert!(action_record.imported_report.is_none());
}
