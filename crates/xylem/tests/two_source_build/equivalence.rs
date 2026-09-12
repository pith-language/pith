use super::*;

/// The equivalence property as a differential check: incremental and cached
/// evaluation produces a result equivalent to evaluation from an empty cache
/// under the same declared inputs. The from-empty side is the round that moves
/// every computation key, where a reuse regression would hide.
#[test]
fn an_incremental_build_matches_the_same_state_built_from_empty() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };

    let incremental_root = temp_root();
    let incremental = {
        let (mut engine, _) = {
            let mut engine = match FilesystemContentStore::open(incremental_root.path()) {
                Ok(store) => Engine::with_content_store(store),
                Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
            };
            let universe = header_universe(&mut engine, false);
            build_engine(incremental_root.path(), &toolchain, universe)
        };
        let source_a = store_blob(&mut engine, SOURCE_A, "a.c");
        let source_b = store_blob(&mut engine, SOURCE_B, "b.c");
        let first = run_build(&mut engine, &build_request(&[source_a, source_b]));
        let before_edit = action_computations(&engine);

        let edited_a = store_blob(&mut engine, SOURCE_A_TOUCHED, "the edited a.c");
        let second = run_build(&mut engine, &build_request(&[edited_a, source_b]));

        // The incremental build has to actually be incremental, or the
        // comparison below is two cold builds agreeing.
        let new_actions = action_computations(&engine)
            .checked_sub(before_edit)
            .expect("the action count went down");
        assert!(
            new_actions < 5,
            "the rebuild served nothing, so this compares two cold builds: {new_actions} actions"
        );
        assert_ne!(
            blob_of(&first.value),
            blob_of(&second.value),
            "the edit did not change the executable, so there is no invalidation to check"
        );
        blob_of(&second.value)
    };

    let empty_root = temp_root();
    let from_empty = {
        let (mut engine, _) = {
            let mut engine = match FilesystemContentStore::open(empty_root.path()) {
                Ok(store) => Engine::with_content_store(store),
                Err(error) => unreachable!("the filesystem store failed to open: {error:?}"),
            };
            let universe = header_universe(&mut engine, false);
            build_engine(empty_root.path(), &toolchain, universe)
        };
        let edited_a = store_blob(&mut engine, SOURCE_A_TOUCHED, "the edited a.c");
        let source_b = store_blob(&mut engine, SOURCE_B, "b.c");
        let only = run_build(&mut engine, &build_request(&[edited_a, source_b]));
        blob_of(&only.value)
    };

    assert_eq!(
        incremental, from_empty,
        "the incrementally rebuilt executable differs from the same declared inputs built \
         from an empty store"
    );
}
