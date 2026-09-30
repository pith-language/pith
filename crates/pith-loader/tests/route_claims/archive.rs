use super::fixture::{Request, Scenario};

#[test]
fn matching_raw_digest_text_does_not_override_adapter_identity() {
    for (left, right) in [("first", "second"), ("second", "first")] {
        let outcome = Scenario::diamond(
            &format!("archive \"{left}\" digest \"claimed\""),
            &format!("archive \"{right}\" digest \"claimed\""),
        )
        .bind(Request::archive("first", "claimed"), "v1")
        .bind(Request::archive("second", "claimed"), "v2")
        .run();
        outcome.assert_conflict();
    }
}

#[test]
fn mirror_agreement_reuses_bytes_after_both_routes_are_located() {
    let outcome = Scenario::diamond(
        "archive \"first\" digest \"same\"",
        "archive \"mirror\" digest \"same\"",
    )
    .bind(Request::archive("first", "same"), "v1")
    .bind(Request::archive("mirror", "same"), "v1")
    .run();
    assert!(outcome.result.is_ok(), "{:?}", outcome.result.err());
    outcome.assert_located(Request::archive("mirror", "same"));
    outcome.assert_read_once("v1");
}

#[test]
fn digest_changes_at_one_locator_reach_distinct_canonical_sources() {
    let outcome = Scenario::diamond(
        "archive \"archive\" digest \"one\"",
        "archive \"archive\" digest \"two\"",
    )
    .bind(Request::archive("archive", "one"), "v1")
    .bind(Request::archive("archive", "two"), "v2")
    .run();
    outcome.assert_conflict();
    outcome.assert_not_read("v2");
}

#[test]
fn malformed_digest_requests_cannot_hide_a_second_adapter_refusal() {
    for digest in ["", "no-algorithm", "blake3:invalid"] {
        let outcome = Scenario::diamond(
            &format!("archive \"first\" digest \"{digest}\""),
            &format!("archive \"refused\" digest \"{digest}\""),
        )
        .bind(Request::archive("first", digest), "v1")
        .run();
        outcome.assert_located(Request::archive("refused", digest));
        assert!(
            outcome
                .result
                .as_ref()
                .err()
                .expect("adapter refusal")
                .iter()
                .any(|diag| diag.message.0.contains("adapter refuses"))
        );
    }
}
