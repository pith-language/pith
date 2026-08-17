use super::*;

#[test]
fn a_generated_source_is_compiled_and_linked_through_the_graph() {
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
    let generator = built_executable(&mut engine, SOURCE_GENERATOR, "generator.c");

    // The generated source is a value the graph produced, so the compile
    // depends on the generate action.
    let generated = blob_of(
        &run_build(
            &mut engine,
            &types::generate_request(toolchain_value.clone(), generator),
        )
        .value,
    );
    let main = store_blob(&mut engine, SOURCE_USES_GENERATED, "main.c");
    let program = blob_of(&run_build(&mut engine, &build_request(&[generated, main])).value);

    // The generator wrote a function returning 7, so a program that exits
    // nonzero ran code the generate action produced: the generated source
    // compiled, linked, and behaved as it was written.
    let verdict = run_build(&mut engine, &types::test_request(toolchain_value, program));
    assert_eq!(verdict.value, types::test_report(false));
}

#[test]
fn touching_the_generator_regenerates_the_source_and_relinks() {
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
    let main = store_blob(&mut engine, SOURCE_USES_GENERATED, "main.c");

    let first_generator = built_executable(&mut engine, SOURCE_GENERATOR, "generator.c");
    let first_generated = blob_of(
        &run_build(
            &mut engine,
            &types::generate_request(toolchain_value.clone(), first_generator),
        )
        .value,
    );
    let first_program =
        blob_of(&run_build(&mut engine, &build_request(&[first_generated, main])).value);

    // A changed generator is a different program, so the generate action's
    // contract names different content and derives a different key (0031, 0036).
    let second_generator =
        built_executable(&mut engine, SOURCE_GENERATOR_TOUCHED, "generator.c touched");
    let second_generated = blob_of(
        &run_build(
            &mut engine,
            &types::generate_request(toolchain_value, second_generator),
        )
        .value,
    );
    let second_program =
        blob_of(&run_build(&mut engine, &build_request(&[second_generated, main])).value);

    assert_ne!(
        first_generator, second_generator,
        "the two generators must differ for this test to mean anything"
    );
    assert_ne!(
        first_generated, second_generated,
        "a changed generator must produce a changed source"
    );
    assert_ne!(
        first_program, second_program,
        "a changed generated source must produce a changed executable"
    );
}
