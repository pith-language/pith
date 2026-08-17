use super::*;

/// Discover a toolchain, skipping only on genuine absence. A driver that is
/// present but undiscoverable fails the test rather than skipping it green:
/// a skip and a pass must not be the same color, because nearly every claim
/// M-3 makes rests on these tests actually running. A skip prints, which is
/// visible under an uncaptured run (`cargo nextest run --no-capture`) — nextest
/// captures the output of passing tests, so `a_c_toolchain_is_available` is
/// what makes a compiler-less host fail rather than look green.
/// A host with no C compiler at all cannot run any test in this file, and a
/// green run over seventeen skips would read as a verified M-3. Fail instead:
/// installing a compiler is the fix, not reading the run as evidence.
#[test]
fn a_c_toolchain_is_available() {
    assert_c_toolchain_available("the M-3 fixture cannot run and its other tests all skipped");
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

/// The linked executable runs and exits with the value its sources compute.
/// `main` returns `a() + b()` = `ANSWER + (ANSWER + 1)` = `81`, which becomes
/// the process exit code. This is the truest end-to-end check: the toolchain
/// produced a program that works, not just bytes with the right magic.
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

/// Variadic linking (decision 0035): a build of four sources links all four
/// objects in one driver invocation over a `List<Object>`, and the executable
/// that comes out computes over all of them — `a() + b() + c()` = `123`. The
/// cold build runs nine actions: four discoveries, four compiles, one link.
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
        // A universe whose header declares all three functions, since this
        // build's main calls `c`.
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
