use super::*;

#[test]
fn two_toolchains_compile_the_same_source_without_sharing_a_cache_entry() {
    let Some(gcc) = toolchain_or_skip("gcc").unwrap() else {
        return;
    };
    let Some(clang) = toolchain_or_skip("clang").unwrap() else {
        return;
    };
    // Without this the test could be one compiler twice and still pass.
    assert_ne!(
        gcc.driver, clang.driver,
        "the two drivers must be different programs"
    );

    let root = temp_root();
    let mut engine = {
        let mut store_engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut store_engine, false);
        two_toolchain_engine(root.path(), &gcc, &clang, universe)
    };
    let source = store_blob(&mut engine, SOURCE_A, "a.c");

    let under_gcc = run_build(
        &mut engine,
        &types::compile_request(gcc.value(), source, no_headers()),
    );
    let after_gcc = action_computations(&engine);
    let under_clang = run_build(
        &mut engine,
        &types::compile_request(clang.value(), source, no_headers()),
    );
    let after_clang = action_computations(&engine);
    let gcc_again = run_build(
        &mut engine,
        &types::compile_request(gcc.value(), source, no_headers()),
    );

    // The toolchain is a request input, so clang's compile is a different
    // request that plans a different contract and cannot be answered from gcc's
    // entry.
    assert_eq!(under_gcc.source, EvaluationSource::Computed);
    assert_eq!(under_clang.source, EvaluationSource::Computed);
    assert!(
        after_clang > after_gcc,
        "clang's compile must plan its own actions, not reuse gcc's"
    );
    assert_ne!(
        blob_of(&under_gcc.value),
        blob_of(&under_clang.value),
        "two compilers must not produce byte-identical objects here"
    );
    assert_eq!(
        gcc_again.source,
        EvaluationSource::Reused,
        "the first toolchain's entry survives the second toolchain's build"
    );
}

#[test]
fn a_toolchain_the_build_was_not_registered_with_is_refused() {
    let Some(gcc) = toolchain_or_skip("gcc").unwrap() else {
        return;
    };
    let root = temp_root();
    let (mut engine, _) = {
        let mut store_engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut store_engine, false);
        build_engine(root.path(), &gcc, universe)
    };
    let source = store_blob(&mut engine, SOURCE_A, "a.c");

    // A driver nothing registered has no closure to confine, and planning a
    // contract against a guessed one would be the ambient discovery 0007 forbids.
    let diagnostics = run_build_expecting_failure(
        &mut engine,
        &types::compile_request(types::toolchain("/nowhere/cc"), source, no_headers()),
    );
    let message = diagnostics
        .iter()
        .next()
        .map(|diagnostic| diagnostic.message.0.to_string())
        .unwrap_or_default();
    assert!(
        message.contains("was not registered with"),
        "the error should name the unregistered toolchain, got: {message}"
    );
}
