use super::*;

#[test]
fn blob_dependency_resumes_with_bytes_and_records_edge() {
    let mut engine = fixture_engine();
    let blob_id = engine.put_blob(b"hello").unwrap();
    engine.register_rule(
        pure_rule("length", interface(&[], Type::Int)),
        BlobLenRule { blob: blob_id },
    );

    let evaluation = engine
        .run(
            &pure_request("length", interface(&[], Type::Int), []),
            &runtime(),
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap()
        .unwrap();

    assert_eq!(evaluation.value, Value::Int(5));
    let deps = engine
        .query()
        .dependencies_of(evaluation.computation)
        .unwrap();
    assert!(matches!(
        deps.first(),
        Some(pith_engine::DependencyEdge::Blob { id }) if *id == blob_id
    ));
}

#[test]
fn missing_blob_reports_clean_diagnostic() {
    let mut engine = fixture_engine();
    let absent = ContentId::of_blob(b"not stored");
    engine.register_rule(
        pure_rule("length", interface(&[], Type::Int)),
        BlobLenRule { blob: absent },
    );

    let result = engine
        .run(
            &pure_request("length", interface(&[], Type::Int), []),
            &runtime(),
            &AllowAllActions,
            &FixtureExecutor,
        )
        .unwrap();

    let err = result.unwrap_err();
    let diag = err.iter().next().unwrap();
    assert_eq!(diag.code, StableCode::from(EngineCode::ContentUnavailable));
    assert_no_pending_attempts(&engine);
}
