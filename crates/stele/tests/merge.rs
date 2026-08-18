//! The merge operator's own claims, the three decision 0052 named as owed by
//! the round that lands it: a conflict names the field and both owners, a
//! replacement whose ownership changed fails, and a permutation of the
//! contributions merges to the same canonical result.

use pith_core::{RecordField, Value};
use stele::merge::{self, Contribution, Keyed};
use stele::types::{self, Behavior};

fn record_of(value: Value) -> Value {
    match value {
        Value::Nominal { representation, .. } => *representation,
        other => other,
    }
}

fn unit_from(owner: &str, exec: &str, after: &[&str]) -> Contribution {
    Contribution {
        owner: owner.into(),
        value: record_of(types::unit_value(
            "example.service",
            "an example",
            exec,
            after,
            &[],
        )),
    }
}

fn first_message(error: &pith_diag::DiagnosticSink) -> &str {
    match error.iter().next() {
        Some(diagnostic) => diagnostic.message.0.as_ref(),
        None => unreachable!("a refused merge carries a diagnostic"),
    }
}

#[test]
fn a_disagreement_names_the_field_both_values_and_both_owners() {
    let error = match merge::merge_records(
        "unit",
        &[],
        &[
            unit_from("base", "/bin/serve", &[]),
            unit_from("machine", "/bin/other", &[]),
        ],
    ) {
        Ok(merged) => unreachable!("a disagreement was merged into {merged:?}"),
        Err(error) => error,
    };
    let message = first_message(&error);
    for expected in ["exec", "base", "machine", "/bin/serve", "/bin/other"] {
        assert!(
            message.contains(expected),
            "the refusal `{message}` should name `{expected}`"
        );
    }
}

#[test]
fn a_replaced_field_whose_ownership_changed_fails() {
    let error = match merge::replace_field(
        "unit",
        &[
            unit_from("machine", "/bin/other", &[]),
            unit_from("site", "/bin/serve", &[]),
        ],
        "exec",
        "base",
        Value::Text("/bin/replacement".into()),
    ) {
        Ok(replaced) => unreachable!("a stale replacement was applied: {replaced:?}"),
        Err(error) => error,
    };
    let message = first_message(&error);
    assert!(
        message.contains("machine") && message.contains("site"),
        "a stale replacement names who declares the field now: `{message}`"
    );
}

#[test]
fn a_replacement_resolves_a_contested_field_and_names_the_winner() {
    let applied = match merge::replace_field(
        "unit",
        &[
            unit_from("base", "/bin/serve", &[]),
            unit_from("machine", "/bin/other", &[]),
        ],
        "exec",
        "base",
        Value::Text("/bin/replacement".into()),
    ) {
        Ok(applied) => applied,
        Err(error) => unreachable!("a replacement of a declared field failed: {error:?}"),
    };
    let merged = match merge::merge_records("unit", &[], &applied) {
        Ok(merged) => merged,
        Err(error) => unreachable!("the replaced unit did not merge: {error:?}"),
    };
    let Value::Record(fields) = &merged else {
        unreachable!("a merged unit is a record");
    };
    let exec = fields
        .iter()
        .find(|field| field.name.as_ref() == "exec")
        .map(|field| &field.payload);
    assert_eq!(exec, Some(&Value::Text("/bin/replacement".into())));
}

#[test]
fn a_replacement_of_an_undeclared_field_names_its_absence() {
    let carrier = Contribution {
        owner: "base".into(),
        value: Value::record([RecordField {
            name: "exec".into(),
            payload: Value::Text("/bin/serve".into()),
        }])
        .unwrap_or_else(|error| unreachable!("the record is well-formed: {error}")),
    };
    let replaced = match merge::replace_field(
        "unit",
        &[carrier],
        "after",
        "base",
        Value::Text("network.target".into()),
    ) {
        Ok(replaced) => unreachable!("a replacement of nothing was applied: {replaced:?}"),
        Err(error) => error,
    };
    assert!(first_message(&replaced).contains("no contribution"));
}

#[test]
fn permutations_of_contributions_merge_to_one_result() {
    let contributions = [
        unit_from("base", "/bin/serve", &["network.target"]),
        unit_from("machine", "/bin/serve", &["time.target"]),
        Contribution {
            owner: "site".into(),
            value: record_of(types::unit_value(
                "example.service",
                "an example",
                "/bin/serve",
                &[],
                &["network.target"],
            )),
        },
    ];
    let policy = vec![
        (Box::from("after"), Behavior::Concat),
        (Box::from("wants"), Behavior::Concat),
    ];
    let first = match merge::merge_records("unit", &policy, &contributions) {
        Ok(merged) => merged,
        Err(error) => unreachable!("the unit merged under its policy: {error:?}"),
    };
    let mut permuted = contributions.clone().to_vec();
    permuted.reverse();
    let second = match merge::merge_records("unit", &policy, &permuted) {
        Ok(merged) => merged,
        Err(error) => unreachable!("the permuted unit merged: {error:?}"),
    };
    assert_eq!(
        first, second,
        "the merge is a function of the set, not the order"
    );

    let Value::Record(fields) = &first else {
        unreachable!("a merged unit is a record");
    };
    let after = fields
        .iter()
        .find(|field| field.name.as_ref() == "after")
        .map(|field| &field.payload);
    let expected = types::text_list(&["time.target", "network.target"]);
    assert_eq!(
        after,
        Some(&expected),
        "a concat field accumulates and canonicalizes"
    );
}

#[test]
fn an_unlisted_field_must_agree() {
    let error = match merge::merge_records(
        "unit",
        &[(Box::from("after"), Behavior::Concat)],
        &[
            unit_from("base", "/bin/serve", &[]),
            unit_from("machine", "/bin/other", &[]),
        ],
    ) {
        Ok(merged) => unreachable!("an unlisted disagreement was merged into {merged:?}"),
        Err(error) => error,
    };
    assert!(
        first_message(&error).contains("exec"),
        "a field the policy does not name still has to agree"
    );
}

#[test]
fn a_field_the_policy_names_for_concat_but_carries_a_scalar_is_refused() {
    let error = match merge::merge_records(
        "unit",
        &[(Box::from("exec"), Behavior::Concat)],
        &[unit_from("base", "/bin/serve", &[])],
    ) {
        Ok(merged) => unreachable!("a scalar was concatenated into {merged:?}"),
        Err(error) => error,
    };
    assert!(
        first_message(&error).contains("list"),
        "a concat field refuses a non-list contribution"
    );
}

#[test]
fn one_path_naming_two_bodies_is_refused_with_both_owners() {
    let hosts = types::FileBody::File {
        content: pith_ids::ContentId::of_blob(b"127.0.0.1 localhost\n"),
        executable: false,
    };
    let other_hosts = types::FileBody::File {
        content: pith_ids::ContentId::of_blob(b"0.0.0.0 elsewhere\n"),
        executable: false,
    };
    let error = match merge::merge_keyed(
        "file path",
        &[
            Keyed {
                owner: "base".into(),
                key: "etc/hosts".into(),
                value: types::file_body_value(&hosts),
            },
            Keyed {
                owner: "site".into(),
                key: "etc/hosts".into(),
                value: types::file_body_value(&other_hosts),
            },
        ],
    ) {
        Ok(merged) => unreachable!("one path naming two bodies merged into {merged:?}"),
        Err(error) => error,
    };
    let message = first_message(&error);
    assert!(
        message.contains("etc/hosts") && message.contains("base") && message.contains("site"),
        "an overlay refusal names the path and both owners: `{message}`"
    );
}

#[test]
fn agreeing_duplicates_collapse_and_permute_to_one_set() {
    let hosts = types::FileBody::File {
        content: pith_ids::ContentId::of_blob(b"127.0.0.1 localhost\n"),
        executable: false,
    };
    let localtime = types::FileBody::Symlink {
        target: "../pool/hosts".into(),
    };
    let entries = [
        Keyed {
            owner: "base".into(),
            key: "etc/hosts".into(),
            value: types::file_body_value(&hosts),
        },
        Keyed {
            owner: "site".into(),
            key: "etc/localtime".into(),
            value: types::file_body_value(&localtime),
        },
        Keyed {
            owner: "site".into(),
            key: "etc/hosts".into(),
            value: types::file_body_value(&hosts),
        },
    ];
    let merged = match merge::merge_keyed("file path", &entries) {
        Ok(merged) => merged,
        Err(error) => unreachable!("agreeing entries collapse: {error:?}"),
    };
    let mut permuted = entries.clone().to_vec();
    permuted.reverse();
    let permuted = match merge::merge_keyed("file path", &permuted) {
        Ok(merged) => merged,
        Err(error) => unreachable!("the permuted entries merged: {error:?}"),
    };
    assert_eq!(merged, permuted);
    assert_eq!(merged.len(), 2, "agreeing duplicates collapse");

    let keys: Vec<&str> = merged.iter().map(|(key, _)| key.as_ref()).collect();
    assert_eq!(
        keys,
        ["etc/hosts", "etc/localtime"],
        "the result is key-sorted"
    );
}

#[test]
fn a_user_disagreement_names_both_owners() {
    let base = Value::record([
        RecordField {
            name: "name".into(),
            payload: Value::Text("root".into()),
        },
        RecordField {
            name: "uid".into(),
            payload: Value::int(0),
        },
    ])
    .unwrap_or_else(|error| unreachable!("the record is well-formed: {error}"));
    let site = Value::record([
        RecordField {
            name: "name".into(),
            payload: Value::Text("root".into()),
        },
        RecordField {
            name: "uid".into(),
            payload: Value::int(100),
        },
    ])
    .unwrap_or_else(|error| unreachable!("the record is well-formed: {error}"));
    let error = match merge::merge_keyed(
        "user account",
        &[
            Keyed {
                owner: "base".into(),
                key: "root".into(),
                value: base,
            },
            Keyed {
                owner: "site".into(),
                key: "root".into(),
                value: site,
            },
        ],
    ) {
        Ok(merged) => unreachable!("a uid disagreement merged into {merged:?}"),
        Err(error) => error,
    };
    let message = first_message(&error);
    assert!(
        message.contains("root") && message.contains("base") && message.contains("site"),
        "a user refusal names the account and both owners: `{message}`"
    );
}
