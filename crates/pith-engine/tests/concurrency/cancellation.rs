use super::*;

#[test]
fn a_cancelled_run_reports_the_cancellation() {
    let mut engine = fixture_engine();
    let roots = register_action_leaves(&mut engine, 2);
    allow_actions(&mut engine, 2);
    let cancel = Arc::new(AtomicBool::new(false));
    let executor = CancellingExecutor {
        cancel: Arc::clone(&cancel),
        barrier: Arc::new(Barrier::new(2)),
    };

    let diagnostics = engine
        .run_many_cancellable(
            &roots,
            &runtime(),
            &AllowAllActions,
            &executor,
            cancel.as_ref(),
        )
        .expect("the runtime drives the run")
        .expect_err("a cancelled run does not produce results");

    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        codes.contains(&pith_diag::StableCode::from(EngineCode::RunCancelled)),
        "expected a run-cancelled diagnostic, got {codes:?}"
    );
}

#[test]
fn cancelled_work_is_recorded_as_cancelled_rather_than_failed() {
    let mut engine = fixture_engine();
    let roots = register_action_leaves(&mut engine, 2);
    allow_actions(&mut engine, 2);
    let cancel = Arc::new(AtomicBool::new(false));
    let executor = CancellingExecutor {
        cancel: Arc::clone(&cancel),
        barrier: Arc::new(Barrier::new(2)),
    };

    let _ = engine
        .run_many_cancellable(
            &roots,
            &runtime(),
            &AllowAllActions,
            &executor,
            cancel.as_ref(),
        )
        .expect("the runtime drives the run");

    let states = attempt_states(&engine);
    assert!(
        !states.is_empty(),
        "the run should have allocated computations"
    );
    assert_no_pending_attempts(&engine);
    // Nothing was wrong with any of this work; it was stopped. Recording it as
    // failed would tell a later reader not to bother re-running it.
    assert!(
        states
            .iter()
            .all(|state| !matches!(state, AttemptState::Failed { .. })),
        "a cancelled run recorded a failure: {states:?}"
    );
    assert!(
        states
            .iter()
            .any(|state| matches!(state, AttemptState::Cancelled { .. })),
        "a cancelled run recorded nothing as cancelled: {states:?}"
    );
}

#[test]
fn cancelled_attempts_are_published_as_cancelled() {
    let mut engine = fixture_engine();
    let roots = register_action_leaves(&mut engine, 2);
    allow_actions(&mut engine, 2);
    let cancel = Arc::new(AtomicBool::new(false));
    let executor = CancellingExecutor {
        cancel: Arc::clone(&cancel),
        barrier: Arc::new(Barrier::new(2)),
    };

    let _ = engine
        .run_many_cancellable(
            &roots,
            &runtime(),
            &AllowAllActions,
            &executor,
            cancel.as_ref(),
        )
        .expect("the runtime drives the run");

    // The arena state and the durable record must agree: a reader that only has
    // the database has to see the same thing the graph does.
    let mut cancelled = 0_usize;
    for (computation, node) in engine.query().computations() {
        let Some(attempt) = engine.durable_attempt_for(computation) else {
            continue;
        };
        let record = engine
            .state_store()
            .attempt(attempt)
            .expect("the memory adapter reads back")
            .expect("a published attempt exists");
        match node.state {
            AttemptState::Cancelled { .. } => {
                cancelled += 1;
                assert!(
                    matches!(record.state, DurableAttemptState::Cancelled(_)),
                    "arena says cancelled, durable record says {:?}",
                    record.state
                );
            }
            _ => assert!(
                !matches!(record.state, DurableAttemptState::Cancelled(_)),
                "durable record says cancelled, arena says {:?}",
                node.state
            ),
        }
    }
    assert!(cancelled > 0, "nothing was cancelled");
}

#[test]
fn an_uncancelled_signal_lets_the_run_finish() {
    let mut engine = fixture_engine();
    let roots = register_action_leaves(&mut engine, 2);
    let cancel = AtomicBool::new(false);
    let executor = barrier_across(&mut engine, 2);

    let evaluations = engine
        .run_many_cancellable(&roots, &runtime(), &AllowAllActions, &executor, &cancel)
        .expect("the runtime drives the run")
        .expect("an uncancelled run produces results");

    assert_eq!(values(&evaluations), [Value::Int(0), Value::Int(2)]);
}

// ---------------------------------------------------------------------------
// Edges a caller can reach
