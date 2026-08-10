//! Additional identity and digest tests.

use pith_ids::{
    ActionSpecDigest, ContentDigest, ContentId, DIGEST_LEN, PureComputationDigest, RuleIdentity,
    RuleRevision,
};

#[test]
fn empty_input_has_a_stable_digest() {
    let d = ContentDigest::of_bytes(b"");
    assert_eq!(d, ContentDigest::of_bytes(b""));
    assert_ne!(d, ContentDigest::of_bytes(b"x"));
    assert_eq!(d.as_bytes().len(), DIGEST_LEN);
}

#[test]
fn all_zero_digest_displays_as_sixty_four_hex_zeros() {
    let d = ContentDigest::from_bytes([0u8; DIGEST_LEN]);
    assert_eq!(d.to_string(), "0".repeat(64));
    assert_eq!(
        format!("{d:?}"),
        "ContentDigest(".to_string() + &"0".repeat(64) + ")"
    );
}

#[test]
fn all_bytes_digest_round_trips_through_debug_and_display() {
    let bytes: [u8; DIGEST_LEN] = core::array::from_fn(|i| (i % 256) as u8);
    let d = ContentDigest::from_bytes(bytes);
    let display = d.to_string();
    let debug = format!("{d:?}");
    assert_eq!(display.len(), 64);
    assert!(debug.starts_with("ContentDigest(") && debug.ends_with(')'));
    assert_eq!(&debug["ContentDigest(".len()..debug.len() - 1], display);
}

#[test]
fn of_blob_and_of_tree_of_empty_input_are_distinct() {
    let blob = ContentId::of_blob(b"");
    let tree = ContentId::of_tree(b"");
    assert_eq!(blob, ContentId::of_blob(b""));
    assert_eq!(tree, ContentId::of_tree(b""));
    assert_ne!(blob, tree);
    assert_ne!(blob.digest(), ContentDigest::of_bytes(b""));
}

#[test]
fn action_spec_and_pure_computation_digests_restore_from_a_known_digest() {
    let action = ActionSpecDigest::of_manifest(b"manifest");
    let computation = PureComputationDigest::of_manifest(b"manifest");

    assert_eq!(ActionSpecDigest::from_digest(action.digest()), action);
    assert_eq!(
        PureComputationDigest::from_digest(computation.digest()),
        computation
    );
    assert_ne!(action.digest(), computation.digest());
}

#[test]
fn rule_identity_restores_from_a_known_digest() {
    let identity = RuleIdentity::of_module_declaration("m", "d");
    assert_eq!(RuleIdentity::from_digest(identity.digest()), identity);
}

#[test]
fn rule_identity_distinguishes_empty_from_nonempty_components() {
    assert_ne!(
        RuleIdentity::of_module_declaration("", "ab"),
        RuleIdentity::of_module_declaration("ab", "")
    );
    assert_ne!(
        RuleIdentity::of_module_declaration("a", "b"),
        RuleIdentity::of_module_declaration("", "ab")
    );
}

#[test]
fn rule_revision_from_parts_preserves_both_halves() {
    let identity = RuleIdentity::of_module_declaration("module", "rule");
    let revision = RuleRevision::of_manifest(identity, b"provider-v1");

    let restored = RuleRevision::from_parts(identity, revision.digest());
    assert_eq!(restored.digest(), revision.digest());
    assert_eq!(restored.rule_identity(), identity);
    assert_eq!(restored, revision);
}
