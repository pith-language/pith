use super::*;

#[test]
fn a_second_build_of_unchanged_sources_reuses_its_root() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let root = temp_root();
    let (mut engine, _) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        build_engine(root.path(), &toolchain, universe)
    };
    let source_a = store_blob(&mut engine, SOURCE_A, "a.c");
    let source_b = store_blob(&mut engine, SOURCE_B, "b.c");
    let request = build_request(&[source_a, source_b]);

    let first = run_build(&mut engine, &request);
    let after_first = action_computations(&engine);
    let second = run_build(&mut engine, &request);

    assert_eq!(first.source, EvaluationSource::Computed);
    assert_eq!(second.source, EvaluationSource::Reused);
    assert_eq!(first.computation, second.computation);
    assert_eq!(first.value, second.value);
    assert_eq!(
        action_computations(&engine),
        after_first,
        "a reused root plans no action"
    );
}

/// The same build in a fresh engine over the same sqlite state and filesystem
/// content store: the root hydrates rather than recomputing. The first engine is
/// dropped before the second opens, so the result crosses a process boundary in
/// everything but name.
#[test]
fn a_fresh_engine_over_the_same_state_hydrates_the_build() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let root = temp_root();
    let (source_a, source_b, computed) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        let (mut engine, _) = build_engine(root.path(), &toolchain, universe);
        let source_a = store_blob(&mut engine, SOURCE_A, "a.c");
        let source_b = store_blob(&mut engine, SOURCE_B, "b.c");
        let computed = run_build(&mut engine, &build_request(&[source_a, source_b]));
        assert_eq!(computed.source, EvaluationSource::Computed);
        (source_a, source_b, computed.value)
    };

    let (mut engine, _) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the store failed to reopen: {error:?}"),
        };
        let universe = header_universe(&mut engine, false);
        build_engine(root.path(), &toolchain, universe)
    };
    let hydrated = run_build(&mut engine, &build_request(&[source_a, source_b]));

    assert_eq!(hydrated.source, EvaluationSource::Hydrated);
    assert_eq!(hydrated.value, computed);
    assert_eq!(
        action_computations(&engine),
        0,
        "a hydrated root allocates no computation beneath it"
    );
}

/// Determinism (0014): two cold compiles of the same source over the same
/// header universe produce byte-identical objects. Caching is switched off so
/// both compiles actually run.
#[test]
fn two_cold_compiles_of_the_same_source_are_identical() {
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
    engine.set_action_caching(false);
    let source = store_blob(&mut engine, SOURCE_A, "a.c");

    let compile = types::compile_request(toolchain_value, source, no_headers());
    let first = blob_of(&run_build(&mut engine, &compile).value);
    let second = blob_of(&run_build(&mut engine, &compile).value);
    assert_eq!(
        first, second,
        "two cold compiles of the same source produced different objects"
    );
}
