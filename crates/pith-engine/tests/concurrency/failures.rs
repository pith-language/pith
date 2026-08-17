use super::*;

#[test]
fn a_fan_out_that_requests_its_requester_is_a_cycle() {
    let mut engine = fixture_engine();
    let root_interface = interface(&[], Type::Int);
    engine.register_rule(
        pure_rule("root", root_interface.clone()),
        SumAllRule {
            dependencies: [pure_request("root", root_interface.clone(), [])].into(),
        },
    );

    let diagnostics = engine
        .evaluate_pure(&pure_request("root", root_interface, []))
        .expect_err("a self-request is a cycle");

    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        codes.contains(&pith_diag::StableCode::from(EngineCode::DependencyCycle)),
        "expected a dependency-cycle diagnostic, got {codes:?}"
    );
    assert_no_pending_attempts(&engine);
}

#[test]
fn a_cycle_is_detected_through_the_frame_that_opened_the_fan_out() {
    let mut engine = fixture_engine();
    // root fans out to `middle`, which requests `root` again. The repeat is in
    // an ancestor chain, not in `middle`'s own stack, so only a cycle check that
    // walks past the fan-out boundary catches it.
    engine.register_rule(
        pure_rule("root", int_interface(0)),
        SumAllRule {
            dependencies: [pure_request("middle", int_interface(1), int_inputs(1))].into(),
        },
    );
    engine.register_rule(
        pure_rule("middle", int_interface(1)),
        SumAllRule {
            dependencies: [pure_request("root", int_interface(0), int_inputs(0))].into(),
        },
    );

    let diagnostics = engine
        .evaluate_pure(&pure_request("root", int_interface(0), int_inputs(0)))
        .expect_err("the cycle crosses the fan-out boundary");

    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        codes.contains(&pith_diag::StableCode::from(EngineCode::DependencyCycle)),
        "expected a dependency-cycle diagnostic, got {codes:?}"
    );
    assert_no_pending_attempts(&engine);
}

#[test]
fn a_failing_action_leaves_no_concurrent_sibling_pending() {
    let mut engine = fixture_engine();
    let roots = register_action_leaves(&mut engine, 2);
    allow_actions(&mut engine, 2);
    // Leaf 1's action plans with the argument "1"; the executor fails that one
    // and yields on the other, so the sibling is in flight when the run aborts.
    let executor = FailOneExecutor { failing: 1 };

    let result = engine
        .run_many(&roots, &runtime(), &AllowAllActions, &executor)
        .expect("the runtime drives the run");

    assert!(result.is_err(), "the failing action should abort the run");
    // The sibling was in flight when the run aborted. Its arena node must not
    // be left waiting on an executor result nobody will read.
    assert_no_pending_attempts(&engine);

    let states = attempt_states(&engine);
    // The action that failed is a failure; the one that was still running was
    // stopped. Recording both as failures would blame work that never got a
    // verdict.
    assert!(
        states
            .iter()
            .any(|state| matches!(state, AttemptState::Failed { .. })),
        "the failing action should be recorded as failed: {states:?}"
    );
    assert!(
        states
            .iter()
            .any(|state| matches!(state, AttemptState::Cancelled { .. })),
        "the in-flight sibling should be recorded as cancelled: {states:?}"
    );
}
