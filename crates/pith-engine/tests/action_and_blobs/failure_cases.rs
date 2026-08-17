use super::*;

#[test]
fn output_import_failure_retains_the_executor_report() {
    let mut engine = import_failing_engine();
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
            &FixtureExecutor,
        )
        .unwrap()
        .unwrap_err();

    assert_eq!(
        diagnostics.iter().next().map(|diagnostic| diagnostic.code),
        Some(StableCode::from(EngineCode::StoreError))
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
    assert_eq!(
        action_record
            .executor_report
            .as_ref()
            .map(|report| report.executor.as_ref()),
        Some("fixture")
    );
    assert!(action_record.imported_report.is_none());
}

#[test]
fn action_result_type_checked_against_interface() {
    let mut engine = fixture_engine();
    let action_iface = interface(&[], Type::Bool);
    let pure_iface = interface(&[], Type::Int);
    engine.register_action_rule(action_rule("liar", action_iface), WrongTypeAction);
    engine.register_rule(
        pure_rule("entry", pure_iface.clone()),
        ActionDepRule {
            dependency: action_request("liar", interface(&[], Type::Bool), []),
        },
    );

    let result = engine
        .run(
            &pure_request("entry", pure_iface, []),
            &runtime(),
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap();

    let err = result.unwrap_err();
    let diag = err.iter().next().unwrap();
    assert_eq!(diag.code, StableCode::from(EngineCode::ResultTypeMismatch));
    assert_no_pending_attempts(&engine);
    let action = engine
        .query()
        .computations()
        .find(|(_, node)| matches!(node.kind, ComputationKind::Action(_)))
        .map(|(_, node)| node)
        .unwrap();
    assert!(matches!(action.state, AttemptState::Failed { .. }));
    let action_record = action.action.as_ref().unwrap();
    assert!(action_record.executor_report.is_some());
    assert!(action_record.imported_report.is_some());
}

#[test]
fn completion_failure_retains_executor_and_imported_reports() {
    let mut engine = fixture_engine();
    let action_interface = interface(&[Type::Int], Type::Blob);
    let root_interface = interface(&[], Type::Blob);
    engine.register_action_rule(
        action_rule("failing completion", action_interface.clone()),
        FailingCompletionAction,
    );
    engine.register_rule(
        pure_rule("entry", root_interface.clone()),
        ActionDepRule {
            dependency: action_request("failing completion", action_interface, [Value::Int(21)]),
        },
    );

    let diagnostics = engine
        .run(
            &pure_request("entry", root_interface, []),
            &runtime(),
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap()
        .unwrap_err();

    assert_eq!(
        diagnostics
            .iter()
            .next()
            .map(|diagnostic| diagnostic.message.0.as_ref()),
        Some("action completion failed")
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
    assert!(action_record.executor_report.is_some());
    assert!(action_record.imported_report.is_some());
}

#[test]
fn undeclared_capability_use_is_rejected() {
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

    let result = engine
        .run(
            &pure_request("entry", pure_iface, []),
            &runtime(),
            &AllowAllActions,
            &UndeclaredCapabilityExecutor,
        )
        .unwrap();

    let err = result.unwrap_err();
    let diag = err.iter().next().unwrap();
    assert_eq!(
        diag.code,
        StableCode::from(EngineCode::UndeclaredCapabilityUse)
    );

    let query = engine.query();
    let Some((computation, node)) = query.computations().find(|(_, node)| node.action.is_some())
    else {
        unreachable!("rejected action has no computation");
    };
    let Some(uses) = query.capability_uses_of(computation) else {
        unreachable!("rejected action computation is missing");
    };
    let uses: Vec<_> = uses.cloned().collect();
    let reported = node
        .action
        .as_ref()
        .and_then(|action| action.imported_report.as_ref())
        .map(|report| report.capabilities_used.as_ref());
    let executor_reported = node
        .action
        .as_ref()
        .and_then(|action| action.executor_report.as_ref())
        .map(|report| report.capabilities_used.as_ref());

    assert_no_pending_attempts(&engine);
    assert!(matches!(node.state, AttemptState::Failed { .. }));
    assert_eq!(uses.len(), 1);
    assert_eq!(
        uses.first().map(|use_| use_.name.as_ref()),
        Some("fixture.clock")
    );
    assert_eq!(reported, Some(uses.as_slice()));
    assert_eq!(executor_reported, Some(uses.as_slice()));
}

#[test]
fn policy_denial_is_recorded_before_execution() {
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

    let runtime_result = engine
        .run(
            &pure_request("entry", root_interface, []),
            &runtime(),
            &DenyDoubleCapability,
            &executor,
        )
        .unwrap();

    let diagnostics = runtime_result.unwrap_err();
    let diagnostic = diagnostics.iter().next().unwrap();
    assert_eq!(diagnostic.code, StableCode::from(EngineCode::PolicyDenied));
    assert_eq!(executions.load(Ordering::Relaxed), 0);
    assert_no_pending_attempts(&engine);

    let query = engine.query();
    let (denied_computation, denied_node, denied_action) = query
        .computations()
        .find_map(|(computation, node)| {
            node.action
                .as_ref()
                .map(|action| (computation, node, action))
        })
        .unwrap();
    assert_eq!(
        denied_action.authorization,
        ActionAuthorization::Denied {
            policy: "deny-double-capability".into(),
            reason: "fixture compute access is disabled".into(),
        }
    );
    assert!(denied_action.executor_report.is_none());
    assert!(denied_action.imported_report.is_none());
    assert!(matches!(
        denied_node.state,
        AttemptState::Failed { ref diagnostics }
            if diagnostics.first().map(|diagnostic| diagnostic.code)
                == Some(StableCode::from(EngineCode::PolicyDenied))
    ));
    let denied_parent = query
        .computations()
        .find(|(_, node)| matches!(node.kind, ComputationKind::Pure(_)))
        .map(|(_, node)| node)
        .unwrap();
    assert!(denied_parent.dependencies.iter().any(|dependency| {
        matches!(
            dependency,
            pith_engine::DependencyEdge::Action { computation, .. }
                if *computation == denied_computation
        )
    }));
}

#[test]
fn executor_must_report_the_planned_platform() {
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

    let runtime_result = engine
        .run(
            &pure_request("entry", root_interface, []),
            &runtime(),
            &AllowAllActions,
            &WrongPlatformExecutor,
        )
        .unwrap();

    let diagnostics = runtime_result.unwrap_err();
    let diagnostic = diagnostics.iter().next().unwrap();
    assert_eq!(
        diagnostic.code,
        StableCode::from(EngineCode::PlatformMismatch)
    );
    let query = engine.query();
    let (failed_computation, failed_action) = query
        .computations()
        .find_map(|(computation, node)| node.action.as_ref().map(|action| (computation, action)))
        .unwrap();
    assert_eq!(
        failed_action
            .imported_report
            .as_ref()
            .map(|report| report.platform.operating_system.as_ref()),
        Some("other")
    );
    assert_eq!(
        failed_action
            .executor_report
            .as_ref()
            .map(|report| report.platform.operating_system.as_ref()),
        Some("other")
    );
    let failed_parent = query
        .computations()
        .find(|(_, node)| matches!(node.kind, ComputationKind::Pure(_)))
        .map(|(_, node)| node)
        .unwrap();
    assert!(failed_parent.dependencies.iter().any(|dependency| {
        matches!(
            dependency,
            pith_engine::DependencyEdge::Action { computation, .. }
                if *computation == failed_computation
        )
    }));
}
