use super::*;

/// Discover a toolchain, skipping only on genuine absence. A driver that is
/// present but undiscoverable fails the test rather than skipping it green:
/// these claims mean something only if the tests run. nextest captures the
/// output of passing tests, so this test is what makes a compiler-less host
/// fail rather than read green over a file of skips.
#[test]
fn a_c_toolchain_is_available() {
    assert_c_toolchain_available("the builds in this file cannot run and their tests all skipped");
}

#[test]
fn a_two_source_build_produces_an_executable() {
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

    let evaluation = run_build(&mut engine, &build_request(&[source_a, source_b]));
    let executable = blob_of(&evaluation.value);

    let store = match FilesystemContentStore::open(root.path()) {
        Ok(store) => store,
        Err(error) => unreachable!("the store failed to reopen: {error:?}"),
    };
    let bytes = match store.get_blob(executable) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => unreachable!("the executable was not in the store"),
        Err(error) => unreachable!("the store failed to read: {error:?}"),
    };
    assert!(
        bytes.as_bytes().starts_with(ELF_MAGIC),
        "the build output is not an ELF executable"
    );
}

/// The linked executable runs and exits with the value its sources compute:
/// `main` returns `a() + b()` = `ANSWER + (ANSWER + 1)` = `81`, which becomes
/// the process exit code.
#[test]
fn the_built_executable_runs_and_exits_with_the_expected_code() {
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

    let evaluation = run_build(&mut engine, &build_request(&[source_a, source_b]));
    let executable = blob_of(&evaluation.value);

    let store = match FilesystemContentStore::open(root.path()) {
        Ok(store) => store,
        Err(error) => unreachable!("the store failed to reopen: {error:?}"),
    };
    let bytes = match store.get_blob(executable) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => unreachable!("the executable was not in the store"),
        Err(error) => unreachable!("the store failed to read: {error:?}"),
    };
    let program = materialize_executable(root.path(), bytes.as_bytes());

    let status = match Command::new(&program).status() {
        Ok(status) => status,
        Err(error) => unreachable!("could not run the program: {error:?}"),
    };
    let code = status
        .code()
        .unwrap_or_else(|| unreachable!("the program was terminated by a signal: {status}"));
    assert_eq!(
        code, EXPECTED_EXIT_CODE,
        "the program exited with {code}; the sources compute {EXPECTED_EXIT_CODE}"
    );
}

/// Variadic linking: every object in a `List<Object>` links in one driver
/// invocation, and the executable computes over all of them.
#[test]
fn a_three_source_build_links_a_list_of_objects() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let root = temp_root();
    let (mut engine, _) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        // This build's `main` calls `c`, so the header declares it.
        let header = match engine
            .put_blob(b"#define ANSWER 40\nint a(void);\nint b(void);\nint c(void);\n")
        {
            Ok(identity) => identity,
            Err(error) => unreachable!("the store failed to hold the header: {error:?}"),
        };
        let universe = HeaderUniverse::new(vec![(HEADER_PATH.into(), header)].into_boxed_slice());
        build_engine(root.path(), &toolchain, universe)
    };
    let source_a = store_blob(&mut engine, SOURCE_A, "a.c");
    let source_b = store_blob(&mut engine, SOURCE_B_NO_MAIN, "b.c");
    let source_c = store_blob(&mut engine, SOURCE_C, "c.c");
    let source_main = store_blob(&mut engine, SOURCE_MAIN_THREE, "main3.c");

    let evaluation = run_build(
        &mut engine,
        &build_request(&[source_a, source_b, source_c, source_main]),
    );
    assert_eq!(
        action_computations(&engine),
        9,
        "the cold four-source build runs nine actions: \
         four discoveries, four compiles, one link"
    );

    let executable = blob_of(&evaluation.value);
    let store = match FilesystemContentStore::open(root.path()) {
        Ok(store) => store,
        Err(error) => unreachable!("the store failed to reopen: {error:?}"),
    };
    let bytes = match store.get_blob(executable) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => unreachable!("the executable was not in the store"),
        Err(error) => unreachable!("the store failed to read: {error:?}"),
    };
    let program = materialize_executable(root.path(), bytes.as_bytes());

    let status = match Command::new(&program).status() {
        Ok(status) => status,
        Err(error) => unreachable!("could not run the program: {error:?}"),
    };
    let code = status
        .code()
        .unwrap_or_else(|| unreachable!("the program was terminated by a signal: {status}"));
    assert_eq!(
        code, 123,
        "main computes 40 + 41 + 42 over the four linked objects; got {code}"
    );
}
