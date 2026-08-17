use super::*;

#[test]
fn action_dependency_publishes_a_reusable_action_and_a_reusable_parent() {
    let mut engine = engine_with_fixtures();
    let action_interface = interface(&[], Type::Blob);
    let root_interface = interface(&[], Type::Blob);
    engine.register_action_rule(
        action_rule("produce", action_interface.clone()),
        BlobProducingAction,
    );
    engine.register_rule(
        pure_rule("entry", root_interface.clone()),
        ActionDepRule {
            dependency: action_request("produce", action_interface, []),
        },
    );

    let evaluation = engine
        .run(
            &pure_request("entry", root_interface, []),
            &runtime(),
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap()
        .unwrap();

    let store = engine.state_store();
    let action_computation = sole_action_computation(&engine).0;
    let action_id = durable_id(&engine, action_computation);
    let action_completion = completed_record(store, action_id);

    // An action's only edges are capability use, which never blocks reuse.
    assert_eq!(action_completion.reuse, DurableReuseDecision::Reusable);
    // A completed action retains the imported report (decision 0024).
    match &action_completion.provenance {
        DurableProvenance::Action(DurableActionProvenance::Imported { imported_report }) => {
            assert_eq!(imported_report.executor.as_ref(), "fixture");
            assert_eq!(imported_report.access, AccessVerification::Prevented);
        }
        provenance => unreachable!("action provenance was {provenance:?}, expected Imported"),
    }
    // Capability-use edges equal the canonicalized reported capabilities.
    assert_eq!(
        action_completion.dependencies.as_ref(),
        [DurableDependency::CapabilityUse {
            capability: action_capability(),
        }]
    );

    // The parent enters the index too (decision 0033), and the gap its key
    // leaves is closed when it is read back.
    let parent_record = completed_record(store, durable_id(&engine, evaluation.computation));
    assert_eq!(parent_record.reuse, DurableReuseDecision::Reusable);
    assert_eq!(
        parent_record.dependencies.as_ref(),
        [DurableDependency::Action { attempt: action_id }]
    );
    assert_eq!(parent_record.provenance, DurableProvenance::Pure);
}

#[test]
fn denied_action_publishes_failed_attempt_with_not_executed_provenance() {
    let mut engine = engine_with_fixtures();
    let action_interface = interface(&[], Type::Blob);
    let root_interface = interface(&[], Type::Blob);
    engine.register_action_rule(
        action_rule("produce", action_interface.clone()),
        BlobProducingAction,
    );
    engine.register_rule(
        pure_rule("entry", root_interface.clone()),
        ActionDepRule {
            dependency: action_request("produce", action_interface, []),
        },
    );

    let diagnostics = engine
        .run(
            &pure_request("entry", root_interface, []),
            &runtime(),
            &DenyCapability,
            &FixtureExecutor,
        )
        .unwrap()
        .err()
        .unwrap();

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == pith_diag::EngineCode::PolicyDenied.into())
    );
    let store = engine.state_store();
    let action_computation = sole_action_computation(&engine).0;
    let failure = failed_record(store, durable_id(&engine, action_computation));
    // A denied action never reached execution: no executor report.
    assert_eq!(
        failure.provenance,
        DurableProvenance::Action(DurableActionProvenance::NotExecuted)
    );
    assert!(failure.dependencies.is_empty());
}
