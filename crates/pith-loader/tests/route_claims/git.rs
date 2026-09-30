use super::fixture::{Read, Request, Scenario};
use pith_loader::PROJECT_NAME;

#[test]
fn identical_symbolic_references_to_distinct_repositories_refuse_in_either_order() {
    for (left, right, unread) in [("first", "second", "v2"), ("second", "first", "v1")] {
        let outcome = Scenario::diamond(
            &format!("git \"{left}\" at \"main\""),
            &format!("git \"{right}\" at \"main\""),
        )
        .bind(Request::git("first", "main", None), "v1")
        .bind(Request::git("second", "main", None), "v2")
        .run();
        outcome.assert_conflict();
        outcome.assert_located(Request::git(right, "main", None));
        outcome.assert_not_read(unread);
    }
}

#[test]
fn mirrors_and_distinct_reference_spellings_can_resolve_to_one_source() {
    for (left, right) in [("full", "full"), ("abbreviation", "full")] {
        let outcome = Scenario::diamond(
            &format!("git \"first\" at \"{left}\""),
            &format!("git \"mirror\" at \"{right}\""),
        )
        .bind(Request::git("first", left, None), "v1")
        .bind(Request::git("mirror", right, None), "v1")
        .run();
        assert!(outcome.result.is_ok(), "{:?}", outcome.result.err());
        outcome.assert_read_once("v1");
        outcome.assert_located(Request::git("mirror", right, None));
    }
}

#[test]
fn subpaths_and_revisions_at_one_repository_select_distinct_sources() {
    for (reference, subpath) in [("other", "one"), ("same", "two")] {
        let outcome = Scenario::diamond(
            "git \"repo\" at \"same\" subpath \"one\"",
            &format!("git \"repo\" at \"{reference}\" subpath \"{subpath}\""),
        )
        .bind(Request::git("repo", "same", Some("one")), "v1")
        .bind(Request::git("repo", reference, Some(subpath)), "v2")
        .run();
        outcome.assert_conflict();
        outcome.assert_not_read("v2");
    }
}

#[test]
fn an_unvalidated_reference_reaches_the_adapter_instead_of_becoming_an_identity() {
    for reference in ["", "main", "abc"] {
        let outcome = Scenario::diamond(
            &format!("git \"first\" at \"{reference}\""),
            &format!("git \"refused\" at \"{reference}\""),
        )
        .bind(Request::git("first", reference, None), "v1")
        .run();
        outcome.assert_located(Request::git("refused", reference, None));
        let diagnostics = outcome
            .result
            .as_ref()
            .err()
            .expect("adapter refusal must survive reuse");
        assert!(diagnostics.iter().any(|diag| {
            diag.message.0.contains("adapter refuses")
                && diag.source.as_ref().is_some_and(|source| {
                    source.label.as_ref() == format!("right/{PROJECT_NAME}").as_str()
                })
        }));
        assert!(!outcome.reads.contains(&Read::Project("refused".into())));
    }
}
