use super::*;

#[test]
fn independent_roots_run_their_actions_concurrently() {
    let mut engine = fixture_engine();
    let roots = register_action_leaves(&mut engine, 2);
    let executor = barrier_across(&mut engine, 2);

    let evaluations = engine
        .run_many(&roots, &runtime(), &AllowAllActions, &executor)
        .expect("the runtime drives the run")
        .expect("both roots evaluate");

    assert_eq!(values(&evaluations), [Value::Int(0), Value::Int(2)]);
    assert_eq!(
        executor.peak(),
        2,
        "the two roots' actions should have been in flight together"
    );
    assert_no_pending_attempts(&engine);
}

#[test]
fn a_fan_out_runs_its_actions_concurrently() {
    let mut engine = fixture_engine();
    let leaves = register_action_leaves(&mut engine, 3);
    // Arity 3 keeps the root distinct from the arity-0/1/2 leaves.
    let root_interface = int_interface(3);
    engine.register_rule(
        pure_rule("root", root_interface.clone()),
        SumAllRule {
            dependencies: leaves.into_boxed_slice(),
        },
    );
    let executor = barrier_across(&mut engine, 3);

    let evaluation = engine
        .run(
            &pure_request("root", root_interface, int_inputs(3)),
            &runtime(),
            &AllowAllActions,
            &executor,
        )
        .expect("the runtime drives the run")
        .expect("the root evaluates");

    // 2*0 + 2*1 + 2*2
    assert_eq!(evaluation.value, Value::Int(6));
    assert_eq!(
        executor.peak(),
        3,
        "all three fan-out actions should have been in flight together"
    );
    assert_no_pending_attempts(&engine);
}

/// The width of the fan-out the bounded tests build, and the bound they hold it
/// to. Six over two is three batches: enough that a driver which started a batch
/// and then stopped refilling would not finish.
const BOUNDED_FAN_OUT: usize = 6;
const ACTION_BOUND: usize = 2;

/// Build a root that fans out over `BOUNDED_FAN_OUT` action-backed leaves.
fn wide_fan_out_root(engine: &mut Engine) -> Request<Pure> {
    let leaves = register_action_leaves(engine, BOUNDED_FAN_OUT);
    let root_interface = int_interface(BOUNDED_FAN_OUT);
    engine.register_rule(
        pure_rule("root", root_interface.clone()),
        SumAllRule {
            dependencies: leaves.into_boxed_slice(),
        },
    );
    pure_request("root", root_interface, int_inputs(BOUNDED_FAN_OUT))
}

fn action_computations(engine: &Engine) -> usize {
    engine
        .query()
        .computations()
        .filter(|(_, node)| matches!(node.kind, ComputationKind::Action(_)))
        .count()
}

#[test]
fn a_fan_out_wider_than_the_bound_runs_its_actions_in_batches() {
    let mut engine = fixture_engine();
    let root = wide_fan_out_root(&mut engine);
    // The barrier is as wide as the bound, so it releases once the driver has
    // filled the pipe and not before. A driver that started the whole fan-out
    // would show a peak of six; one that started a batch and never refilled
    // would never reach the third generation and would time out.
    let executor = barrier_across(&mut engine, ACTION_BOUND);

    let evaluation = engine
        .run(&root, &runtime(), &AllowAllActions, &executor)
        .expect("the runtime drives the run")
        .expect("the root evaluates");

    // 2 * (0 + 1 + 2 + 3 + 4 + 5)
    assert_eq!(evaluation.value, Value::Int(30));
    assert_eq!(
        executor.peak(),
        ACTION_BOUND,
        "the bound should have held the fan-out to {ACTION_BOUND} actions at a time"
    );
    assert_no_pending_attempts(&engine);
}

#[test]
fn actions_waiting_for_a_slot_are_not_planned() {
    let mut engine = fixture_engine();
    let root = wide_fan_out_root(&mut engine);
    allow_actions(&mut engine, ACTION_BOUND);
    // Leaf 0's action plans with the argument "0", so the failure lands in the
    // first batch and the run aborts while the rest are still queued.
    let executor = FailOneExecutor { failing: 0 };

    let result = engine
        .run(&root, &runtime(), &AllowAllActions, &executor)
        .expect("the runtime drives the run");

    assert!(result.is_err(), "the failing action should abort the run");
    // The cost the bound exists to control is the materialized invocation, not
    // the chain. A queued action has no computation node: it was never planned,
    // so its inputs were never read out of the content store.
    assert_eq!(
        action_computations(&engine),
        ACTION_BOUND,
        "only the actions that had a slot should have been planned"
    );
    assert_no_pending_attempts(&engine);
}

#[test]
fn a_fan_out_resumes_in_request_order() {
    let mut engine = fixture_engine();
    // Register the leaves in the reverse of the order they are requested, so a
    // scheduler that resumed in registration or completion order would show it.
    for (arity, value) in [(1_usize, 20_i64), (0, 10)] {
        engine.register_rule(
            pure_rule(&format!("leaf-{arity}"), int_interface(arity)),
            ConstantRule(Value::Int(value)),
        );
    }
    let root_interface = interface(&[], Type::Text);
    engine.register_rule(
        pure_rule("root", root_interface.clone()),
        OrderedAllRule {
            dependencies: [
                pure_request("leaf-0", int_interface(0), int_inputs(0)),
                pure_request("leaf-1", int_interface(1), int_inputs(1)),
            ]
            .into(),
        },
    );

    let evaluation = engine
        .evaluate_pure(&pure_request("root", root_interface, []))
        .expect("the root evaluates");

    assert_eq!(evaluation.value, Value::Text("10,20".into()));
}

#[test]
fn an_empty_fan_out_resumes_with_no_values() {
    let mut engine = fixture_engine();
    let root_interface = interface(&[], Type::Int);
    engine.register_rule(
        pure_rule("root", root_interface.clone()),
        SumAllRule {
            dependencies: Box::new([]),
        },
    );

    let evaluation = engine
        .evaluate_pure(&pure_request("root", root_interface, []))
        .expect("the root evaluates");

    assert_eq!(evaluation.value, Value::Int(0));
}
