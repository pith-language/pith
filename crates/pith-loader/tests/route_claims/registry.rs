use pith_engine::{Engine, MemoryEngineStateStore};
use pith_loader::{FrontendCode, RegisterFrontend};
use pith_store::MemoryContentStore;

use super::fixture::{Request, Scenario};

#[test]
fn registry_aliases_elaborate_and_project_through_the_same_frontend() {
    for (left, right) in [("primary", "alias"), ("alias", "primary")] {
        let outcome = Scenario::diamond(
            &format!("from registry {left}"),
            &format!("from registry {right}"),
        )
        .bind(Request::Registry("registry".into()), "v1")
        .run();
        outcome.assert_read_once("v1");
        let workspace = outcome.result.expect("aliases agree");
        assert!(workspace.elaborate().is_ok());
        let mut engine = Engine::with_state_store(
            MemoryContentStore::default(),
            MemoryEngineStateStore::default(),
        );
        engine.register_frontend();
        let projection = workspace
            .project_onto_frontend(&mut engine)
            .expect("the acquired closure projects");
        assert_eq!(projection.modules().count(), workspace.len().get());
        assert_eq!(
            workspace
                .modules()
                .filter(|module| module.subject().to_string() == "example/dep")
                .count(),
            1
        );
    }
}

#[test]
fn different_trust_roots_at_one_location_do_not_become_aliases() {
    for (left, right) in [("primary", "rotated"), ("rotated", "primary")] {
        let outcome = Scenario::diamond(
            &format!("from registry {left}"),
            &format!("from registry {right}"),
        )
        .bind(Request::Registry("registry".into()), "v1")
        .run();
        outcome.assert_conflict();
    }
}

#[test]
fn distinct_registries_with_one_root_key_remain_distinct() {
    let outcome = Scenario::diamond("from registry primary", "from registry other")
        .bind(Request::Registry("registry".into()), "v1")
        .bind(Request::Registry("other".into()), "v2")
        .run();
    outcome.assert_conflict();
}

#[test]
fn alias_reuse_checks_the_later_consumers_version_range() {
    let outcome = Scenario::diamond(">= 1 from registry primary", ">= 2 from registry alias")
        .bind(Request::Registry("registry".into()), "v1")
        .run();
    let diagnostics = outcome
        .result
        .as_ref()
        .err()
        .expect("the second consumer excludes version 1");
    assert!(diagnostics.iter().any(|diag| {
        diag.code == FrontendCode::UnexpectedSubject.stable()
            && diag
                .source
                .as_ref()
                .is_some_and(|source| source.label.as_ref() == "right/module.pi")
    }));
}
