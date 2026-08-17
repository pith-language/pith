use super::*;

#[test]
fn running_no_requests_produces_no_evaluations() {
    let mut engine = fixture_engine();

    let evaluations = engine
        .run_many(
            &[],
            &runtime(),
            &AllowAllActions,
            &BarrierExecutor::expecting(0),
        )
        .expect("the runtime drives the run")
        .expect("an empty run succeeds");

    assert!(evaluations.is_empty());
}

#[test]
fn roots_that_share_a_dependency_still_evaluate() {
    // `run_many` documents its roots as independent, but nothing stops a caller
    // passing two that reach the same computation. Neither sees the other's
    // result — the shared computation has not completed when the second root is
    // prepared — so it is evaluated twice. Wasteful, and it must still be
    // correct: two chains publishing the same computation key concurrently is
    // exactly the case that would corrupt a reusable index that assumed one.
    let mut engine = fixture_engine();
    engine.register_rule(
        pure_rule("shared", int_interface(0)),
        ConstantRule(Value::int(7)),
    );
    for arity in [1_usize, 2] {
        engine.register_rule(
            pure_rule(&format!("consumer-{arity}"), int_interface(arity)),
            SumAllRule {
                dependencies: [pure_request("shared", int_interface(0), int_inputs(0))].into(),
            },
        );
    }
    let roots = [
        pure_request("consumer-1", int_interface(1), int_inputs(1)),
        pure_request("consumer-2", int_interface(2), int_inputs(2)),
    ];

    let evaluations = engine
        .run_many(
            &roots,
            &runtime(),
            &AllowAllActions,
            &BarrierExecutor::expecting(0),
        )
        .expect("the runtime drives the run")
        .expect("both roots evaluate");

    assert_eq!(values(&evaluations), [Value::int(7), Value::int(7)]);
    assert_no_pending_attempts(&engine);
}

#[test]
fn the_same_root_requested_twice_evaluates_twice() {
    let mut engine = fixture_engine();
    engine.register_rule(
        pure_rule("leaf", int_interface(0)),
        ConstantRule(Value::int(5)),
    );
    let request = pure_request("leaf", int_interface(0), int_inputs(0));

    let evaluations = engine
        .run_many(
            &[request.clone(), request],
            &runtime(),
            &AllowAllActions,
            &BarrierExecutor::expecting(0),
        )
        .expect("the runtime drives the run")
        .expect("both roots evaluate");

    assert_eq!(values(&evaluations), [Value::int(5), Value::int(5)]);
    assert_no_pending_attempts(&engine);
}
