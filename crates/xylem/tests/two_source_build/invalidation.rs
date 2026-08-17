use super::*;

/// Fine-grained invalidation (U-5): touching `a.c` recompiles `a.o` and does
/// not re-run `b.o`'s discovery or compile. Both of those are served from the
/// reusable action index, so the second build adds three action computations —
/// `a`'s discovery, `a`'s compile, and the link — rather than five.
#[test]
fn touching_one_source_recompiles_only_its_object() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let root = temp_root();
    let (mut engine, _) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, true);
        build_engine(root.path(), &toolchain, universe)
    };
    let source_a = store_blob(&mut engine, SOURCE_A, "a.c");
    let source_b = store_blob(&mut engine, SOURCE_B, "b.c");

    let _first = run_build(&mut engine, &build_request(&[source_a, source_b]));
    let after_first = action_computations(&engine);

    let source_a_touched = store_blob(&mut engine, SOURCE_A_TOUCHED, "the touched a.c");
    let _second = run_build(&mut engine, &build_request(&[source_a_touched, source_b]));
    let after_second = action_computations(&engine);

    let new_actions = after_second
        .checked_sub(after_first)
        .expect("the action count went down");
    assert_eq!(
        new_actions, 3,
        "touching a.c should re-run its discovery, recompile a.o, and re-link (3 actions), \
         not re-run b's discovery or compile (which would be 5); got {new_actions}"
    );
}

/// A rebuild under an edited header recompiles both objects (decision 0034).
/// `a.c` is touched so the root re-runs, and the header's new content is
/// offered through a changed universe. `b`'s compile entry has an unchanged
/// pure key, so the measurement is on the walk 0033 built: serving it from the
/// index re-plans its recorded action requests against the universe this run
/// registered, the planned contracts stage the header's new content identity,
/// and both compiles — not just `a`'s — execute again. The rebuild is two
/// discoveries, two compiles, and a link, and the executable that comes out
/// answers the touched header.
#[test]
fn a_rebuild_under_an_edited_header_recompiles_both_objects() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let root = temp_root();
    let source_a;
    let source_b;
    {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
        };
        let universe = header_universe(&mut engine, true);
        let (mut engine, _) = build_engine(root.path(), &toolchain, universe);
        source_a = store_blob(&mut engine, SOURCE_A, "a.c");
        source_b = store_blob(&mut engine, SOURCE_B, "b.c");
        let _first = run_build(&mut engine, &build_request(&[source_a, source_b]));
        assert_eq!(
            action_computations(&engine),
            5,
            "the cold build ran five actions"
        );
    }

    // The header edit: same universe shape, new content identity, delivered by
    // a fresh engine over the same durable state. The first engine is dropped
    // before the second opens, so the rebuild crosses the process boundary the
    // reusable index lives across.
    let (mut engine, _) = {
        let mut engine = match FilesystemContentStore::open(root.path()) {
            Ok(store) => Engine::with_content_store(store),
            Err(error) => unreachable!("the store failed to reopen: {error:?}"),
        };
        let header = match engine.put_blob(HEADER_TOUCHED) {
            Ok(identity) => identity,
            Err(error) => unreachable!("the store failed to hold the touched header: {error:?}"),
        };
        let unused = match engine.put_blob(UNUSED) {
            Ok(identity) => identity,
            Err(error) => unreachable!("the store failed to hold unused.h: {error:?}"),
        };
        let universe = HeaderUniverse::new(
            vec![(HEADER_PATH.into(), header), (UNUSED_PATH.into(), unused)].into_boxed_slice(),
        );
        build_engine(root.path(), &toolchain, universe)
    };

    // `a.c` is touched alongside the header, so the root re-runs. `b`'s entry
    // is served from the index only after its action edges re-plan.
    let source_a_touched = store_blob(&mut engine, SOURCE_A_TOUCHED, "the touched a.c");
    let evaluation = run_build(&mut engine, &build_request(&[source_a_touched, source_b]));
    assert_eq!(
        action_computations(&engine),
        5,
        "an edited header must re-run both discoveries, both compiles, and the link; \
         a smaller count means a stale object was served"
    );

    // The rebuilt executable answers the touched header, which is the proof
    // the recompiles actually read the new content: a() is ANSWER+2 = 44,
    // b() is ANSWER+1 = 43.
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
        code, 87,
        "main computes (42+2) + (42+1) against the touched header"
    );
}

/// A header the universe does not offer is a loud failure inside the sandbox,
/// not a compile against the host filesystem (decisions 0030, 0034). Landlock
/// confines the discovery pass to the staged universe, so the preprocessor's
/// `#include` resolves nowhere and the tool reports it; nothing outside the
/// declared set can be read instead.
#[test]
fn an_undeclared_header_fails_rather_than_reading_the_host() {
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
    let source = store_blob(&mut engine, SOURCE_UNDECLARED, "undeclared.c");

    let diagnostics = run_build_expecting_failure(
        &mut engine,
        &types::compile_request(toolchain.value(), source, no_headers()),
    );
    let text: Vec<String> = diagnostics
        .iter()
        .map(|d| d.message.0.as_ref().to_owned())
        .collect();
    assert!(
        text.iter().any(|message| message.contains("unused.h")),
        "the diagnostics should name the undeclared header; got: {text:?}"
    );
}
