//! Property tests for the identity and digest primitives in `pith-ids`.
//!
//! These exercise the contracts the docs rely on (decisions 0005, 0013, 0023):
//! digests are deterministic, domain-separated, and length-prefixed so moving
//! bytes across a field boundary changes identity; every restorable digest
//! round-trips through its `from_digest`/`from_parts` constructor.

use pith_ids::{
    ActionSpecDigest, ContentDigest, ContentId, PureComputationDigest, RuleIdentity, RuleRevision,
};
use proptest::prelude::*;

/// ASCII byte strings (NUL included, multi-byte excluded) so every byte offset
/// is a valid UTF-8 boundary and `String::from_utf8` is infallible.
#[allow(
    clippy::unwrap_used,
    reason = "bytes are 0x00-0x7f, always valid utf-8"
)]
fn ascii_string(min: usize, max: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(0u8..=0x7f, min..=max).prop_map(|b| String::from_utf8(b).unwrap())
}

fn arbitrary_bytes(max: usize) -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), 0..max)
}

proptest! {
    #[test]
    fn of_bytes_is_deterministic(bytes in arbitrary_bytes(128)) {
        prop_assert_eq!(ContentDigest::of_bytes(&bytes), ContentDigest::of_bytes(&bytes));
    }

    #[test]
    fn blob_identity_restores_from_its_digest(bytes in arbitrary_bytes(128)) {
        let id = ContentId::of_blob(&bytes);
        prop_assert_eq!(ContentId::from_digest(id.digest()), id);
    }

    #[test]
    fn blob_tree_and_raw_digests_are_domain_separated(bytes in arbitrary_bytes(128)) {
        // Distinct domain-separation prefixes must never collide across kinds.
        let blob = ContentId::of_blob(&bytes).digest();
        let tree = ContentId::of_tree(&bytes).digest();
        let raw = ContentDigest::of_bytes(&bytes);
        prop_assert_ne!(blob, tree);
        prop_assert_ne!(blob, raw);
        prop_assert_ne!(tree, raw);
    }

    #[test]
    fn rule_identity_restores_from_its_digest(
        module in ascii_string(0, 32),
        declaration in ascii_string(0, 32),
    ) {
        let id = RuleIdentity::of_module_declaration(&module, &declaration);
        prop_assert_eq!(RuleIdentity::from_digest(id.digest()), id);
    }

    #[test]
    fn moving_bytes_across_the_declaration_boundary_changes_identity(
        prefix in ascii_string(0, 16),
        mid in ascii_string(1, 16),
        suffix in ascii_string(0, 16),
    ) {
        // Same total bytes, two different split points. A naive concatenation
        // hash would make these equal; the length-prefixed manifest must not.
        // `mid` is non-empty so the split point actually moves.
        let left = RuleIdentity::of_module_declaration(&prefix, &format!("{mid}{suffix}"));
        let right = RuleIdentity::of_module_declaration(&format!("{prefix}{mid}"), &suffix);
        prop_assert_ne!(left, right);
    }

    #[test]
    fn rule_revision_restores_from_parts(
        module in ascii_string(0, 16),
        declaration in ascii_string(0, 16),
        manifest in arbitrary_bytes(64),
    ) {
        let identity = RuleIdentity::of_module_declaration(&module, &declaration);
        let revision = RuleRevision::of_manifest(identity, &manifest);
        let restored = RuleRevision::from_parts(identity, revision.digest());
        prop_assert_eq!(restored, revision);
        prop_assert_eq!(restored.digest(), revision.digest());
        prop_assert_eq!(restored.rule_identity(), identity);
    }

    #[test]
    fn rule_revision_distinguishes_manifests(
        module in ascii_string(0, 16),
        declaration in ascii_string(0, 16),
        manifest in arbitrary_bytes(64),
        other_manifest in arbitrary_bytes(64),
    ) {
        prop_assume!(manifest != other_manifest);
        let identity = RuleIdentity::of_module_declaration(&module, &declaration);
        let a = RuleRevision::of_manifest(identity, &manifest);
        let b = RuleRevision::of_manifest(identity, &other_manifest);
        prop_assert_ne!(a, b);
        prop_assert_eq!(a.rule_identity(), b.rule_identity());
    }

    #[test]
    fn rule_revision_distinguishes_rule_identities(
        module in ascii_string(0, 16),
        declaration in ascii_string(0, 16),
        manifest in arbitrary_bytes(64),
    ) {
        prop_assume!(module != declaration);
        let a_id = RuleIdentity::of_module_declaration(&module, &declaration);
        let b_id = RuleIdentity::of_module_declaration(&declaration, &module);
        let a = RuleRevision::of_manifest(a_id, &manifest);
        let b = RuleRevision::of_manifest(b_id, &manifest);
        prop_assert_ne!(a, b);
    }

    #[test]
    fn action_spec_and_pure_computation_digests_restore(manifest in arbitrary_bytes(64)) {
        let action = ActionSpecDigest::of_manifest(&manifest);
        prop_assert_eq!(ActionSpecDigest::from_digest(action.digest()), action);

        let pure = PureComputationDigest::of_manifest(&manifest);
        prop_assert_eq!(PureComputationDigest::from_digest(pure.digest()), pure);

        // Different domain-separation prefixes; must not collide.
        prop_assert_ne!(action.digest(), pure.digest());
    }
}
