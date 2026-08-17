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

#[test]
fn hydration_is_refused_when_a_dependency_rule_was_revised() {
    // Decision 0049. A revised rule mints a *new* computation key, which leaves
    // the old key's attempt undisturbed and still the latest reusable one under
    // it. Asking only "is this key's latest attempt still the recorded one"
    // therefore answers yes, and the consumer hydrates a result derived from a
    // rule body this engine no longer has.
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
    assert_eq!(first_root.value, Value::Int(1));

    // The root's own rule is unchanged, so its key and its recorded attempt are
    // untouched. Only the leaf's revision moved, and its body now answers 2.
    let mut second = engine_with_state(state.clone());
    second.register_rule(
        revised_pure_rule("leaf", leaf.clone(), b"leaf-v2"),
        ConstantRule(Value::Int(2)),
    );
    second.register_rule(
        pure_rule("root", root.clone()),
        ForwardRule {
            dependency: pure_request("leaf", leaf, []),
        },
    );
    let second_root = second
        .evaluate_pure(&pure_request("root", root, [Value::Bool(false)]))
        .unwrap();

    assert_eq!(
        second_root.source,
        EvaluationSource::Computed,
        "a revised dependency rule must make its consumer recompute, not hydrate"
    );
    assert_eq!(
        second_root.value,
        Value::Int(2),
        "the recomputed root must observe the revised leaf rather than the superseded one"
    );
}

#[test]
fn hydration_is_refused_when_a_rule_two_levels_down_was_revised() {
    // Decision 0051. The root's edge to the middle computation revalidates on
    // its own terms and stops: the middle rule is unrevised, so its key is
    // unmoved and its recorded attempt is still the latest reusable one under
    // it. Everything 0049 checks passes, and the leaf underneath it has been
    // revised. Only a walk into the middle attempt's own record sees it.
    let state = SharedEngineStateStore::default();
    let leaf = interface(&[], Type::Int);
    let middle = interface(&[Type::Bool], Type::Int);
    let root = interface(&[Type::Bool, Type::Bool], Type::Int);

    let mut first = engine_with_state(state.clone());
    register_chain(
        &mut first,
        &leaf,
        &middle,
        &root,
        ConstantRule(Value::Int(1)),
    );
    let first_root = first.evaluate_pure(&chain_root_request(&root)).unwrap();
    assert_eq!(first_root.value, Value::Int(1));

    // Only the leaf's revision moved. The middle and root rules are registered
    // exactly as they were, so both of their keys and both of their recorded
    // attempts are untouched.
    let mut second = engine_with_state(state.clone());
    second.register_rule(
        revised_pure_rule("leaf", leaf.clone(), b"leaf-v2"),
        ConstantRule(Value::Int(2)),
    );
    second.register_rule(
        pure_rule("middle", middle.clone()),
        ForwardRule {
            dependency: pure_request("leaf", leaf, []),
        },
    );
    second.register_rule(
        pure_rule("root", root.clone()),
        ForwardRule {
            dependency: pure_request("middle", middle, [Value::Bool(false)]),
        },
    );
    let second_root = second.evaluate_pure(&chain_root_request(&root)).unwrap();

    assert_eq!(
        second_root.source,
        EvaluationSource::Computed,
        "a revision two levels down must reach the root, not stop at the edge above it"
    );
    assert_eq!(
        second_root.value,
        Value::Int(2),
        "the recomputed root must observe the revised leaf through the middle computation"
    );
}

#[test]
fn a_revised_leaf_recomputed_to_an_equal_result_still_hydrates_its_root() {
    // The walk descends through the attempt the edge check accepted, not the
    // superseded one the edge recorded. Here they differ and disagree: the
    // recorded middle attempt names the leaf at its old revision, and the
    // attempt that superseded it names the leaf at the new one. Descending into
    // the recorded attempt would refuse a root that is genuinely current, which
    // is the early cutoff decision 0033 exists to preserve.
    let state = SharedEngineStateStore::default();
    let leaf = interface(&[], Type::Int);
    let middle = interface(&[Type::Bool], Type::Int);
    let root = interface(&[Type::Bool, Type::Bool], Type::Int);

    let mut first = engine_with_state(state.clone());
    register_chain(
        &mut first,
        &leaf,
        &middle,
        &root,
        ConstantRule(Value::Int(1)),
    );
    let first_root = first.evaluate_pure(&chain_root_request(&root)).unwrap();
    let root_attempt = durable_id(&first, first_root.computation);

    // The leaf is revised and answers the same value, so the middle computation
    // recomputes to a canonically equal result under its own unmoved key. The
    // root is never requested here and its record is left as it was.
    let mut second = engine_with_state(state.clone());
    second.register_rule(
        revised_pure_rule("leaf", leaf.clone(), b"leaf-v2"),
        ConstantRule(Value::Int(1)),
    );
    second.register_rule(
        pure_rule("middle", middle.clone()),
        ForwardRule {
            dependency: pure_request("leaf", leaf.clone(), []),
        },
    );
    let recomputed_middle = second
        .evaluate_pure(&pure_request(
            "middle",
            middle.clone(),
            [Value::Bool(false)],
        ))
        .unwrap();
    assert_eq!(recomputed_middle.source, EvaluationSource::Computed);

    let mut third = engine_with_state(state.clone());
    third.register_rule(
        revised_pure_rule("leaf", leaf.clone(), b"leaf-v2"),
        FailingRule,
    );
    third.register_rule(
        pure_rule("middle", middle.clone()),
        ForwardRule {
            dependency: pure_request("leaf", leaf, []),
        },
    );
    third.register_rule(
        pure_rule("root", root.clone()),
        ForwardRule {
            dependency: pure_request("middle", middle, [Value::Bool(false)]),
        },
    );
    let third_root = third.evaluate_pure(&chain_root_request(&root)).unwrap();

    assert_eq!(
        third_root.source,
        EvaluationSource::Hydrated,
        "the root's whole recorded subtree is current, so nothing under it should force a recompute"
    );
    assert_eq!(third_root.value, Value::Int(1));
    assert_eq!(durable_id(&third, third_root.computation), root_attempt);
}

/// A root over a middle computation over a leaf, the shape the transitive
/// checks need and the shallowest one they can be told apart on: an edge that
/// stops at its target cannot see past the middle computation.
fn register_chain(
    engine: &mut Engine,
    leaf: &Interface,
    middle: &Interface,
    root: &Interface,
    leaf_body: ConstantRule,
) {
    engine.register_rule(pure_rule("leaf", leaf.clone()), leaf_body);
    engine.register_rule(
        pure_rule("middle", middle.clone()),
        ForwardRule {
            dependency: pure_request("leaf", leaf.clone(), []),
        },
    );
    engine.register_rule(
        pure_rule("root", root.clone()),
        ForwardRule {
            dependency: pure_request("middle", middle.clone(), [Value::Bool(false)]),
        },
    );
}

fn chain_root_request(root: &Interface) -> Request<Pure> {
    pure_request(
        "root",
        root.clone(),
        [Value::Bool(false), Value::Bool(true)],
    )
}

#[test]
fn hydration_is_refused_when_a_dependency_rule_is_not_registered() {
    // The other half of 0049's check, and the case that decides what an absent
    // rule means for a recorded edge: refusing is not a lost optimization,
    // because a consumer whose dependency has no rule cannot be evaluated at
    // all. Serving it from the index would hand back a value this rule set
    // cannot derive.
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
    first
        .evaluate_pure(&pure_request("root", root.clone(), [Value::Bool(false)]))
        .unwrap();

    // The second engine has the root's rule and not the leaf's.
    let mut second = engine_with_state(state.clone());
    second.register_rule(
        pure_rule("root", root.clone()),
        ForwardRule {
            dependency: pure_request("leaf", leaf, []),
        },
    );
    let diagnostics = second
        .evaluate_pure(&pure_request("root", root, [Value::Bool(false)]))
        .expect_err("the root must not be served from a rule set that cannot derive it");

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == pith_diag::EngineCode::NoRuleForInterface.into()),
        "the refusal names the missing rule rather than reporting a cache miss: {diagnostics:?}"
    );
}
