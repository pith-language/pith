use super::fixture::{Request, Scenario};

#[test]
fn equivalent_paths_load_once_and_distinct_paths_refuse() {
    let outcome = Scenario::diamond("from path \"dep\"", "from path \"./dep\"")
        .bind(Request::Path("dep".into()), "v1")
        .bind(Request::Path("./dep".into()), "v1")
        .run();
    assert!(outcome.result.is_ok(), "{:?}", outcome.result.err());
    outcome.assert_read_once("v1");
    let outcome = Scenario::diamond("from path \"dep\"", "from path \"other\"")
        .bind(Request::Path("dep".into()), "v1")
        .bind(Request::Path("other".into()), "v2")
        .run();
    outcome.assert_conflict();
    outcome.assert_not_read("v2");
}

#[test]
fn source_kinds_retain_distinct_claims_at_one_location() {
    let outcome = Scenario::diamond("from path \"dep\"", "from git \"repo\" revision \"same\"")
        .bind(Request::Path("dep".into()), "v1")
        .bind(Request::git("repo", "same", None), "v1")
        .run();
    outcome.assert_conflict();
}
