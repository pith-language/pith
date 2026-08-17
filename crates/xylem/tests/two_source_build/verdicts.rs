use super::*;

#[test]
fn a_passing_test_reports_a_passing_verdict() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let root = temp_root();
    let (mut engine, toolchain_value) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        build_engine(root.path(), &toolchain, universe)
    };
    let executable = built_executable(&mut engine, SOURCE_TEST_PASSING, "passing.c");

    // The program under test is the action's program, staged from the store and
    // run confined (decisions 0036, 0028).
    let verdict = run_build(
        &mut engine,
        &types::test_request(toolchain_value, executable),
    );

    assert_eq!(verdict.value, types::test_report(true));
}

#[test]
fn a_failing_test_is_a_verdict_rather_than_a_failed_build() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let root = temp_root();
    let (mut engine, toolchain_value) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        build_engine(root.path(), &toolchain, universe)
    };
    let executable = built_executable(&mut engine, SOURCE_TEST_FAILING, "failing.c");

    // `run_build` fails the test on a failed run, so reaching an assertion at
    // all is half the claim: the nonzero exit did not fail the action.
    let verdict = run_build(
        &mut engine,
        &types::test_request(toolchain_value, executable),
    );

    assert_eq!(verdict.value, types::test_report(false));
}

#[test]
fn an_unchanged_failing_test_is_served_rather_than_re_run() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let root = temp_root();
    let (mut engine, toolchain_value) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        build_engine(root.path(), &toolchain, universe)
    };
    let executable = built_executable(&mut engine, SOURCE_TEST_FAILING, "failing.c");
    let request = types::test_request(toolchain_value, executable);

    let first = run_build(&mut engine, &request);
    let after_first = action_computations(&engine);
    let second = run_build(&mut engine, &request);

    // A failed computation is not in the reusable index, so this is the case a
    // failed action could not have served.
    assert_eq!(first.source, EvaluationSource::Computed);
    assert_eq!(second.source, EvaluationSource::Reused);
    assert_eq!(first.value, second.value);
    assert_eq!(second.value, types::test_report(false));
    assert_eq!(
        action_computations(&engine),
        after_first,
        "a reused verdict plans no action"
    );
}
