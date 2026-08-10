//! Property tests for the canonical tree manifest (in-crate so they can call
//! `Tree::manifest`, which is `pub(crate)`).
//!
//! Round-trip: a tree built from valid entries re-derives its manifest and
//! decodes back to the same identity and entries. Permutation invariance: the
//! manifest sorts entries by name, so building from the same entries in any
//! order yields the same content identity.

use super::*;
use proptest::prelude::*;

// Single-component, NUL-free names: valid by `TreeEntry::new`'s rules.
const NAMES: &[&str] = &["a", "b", "c", "file", "dir", "link"];

type Decision = (u8, u8, bool, u8, Vec<u8>);

fn tree_decisions() -> impl Strategy<Value = Vec<Decision>> {
    proptest::collection::vec(
        (
            0u8..(NAMES.len() as u8),
            0u8..3,
            any::<bool>(),
            0u8..5,
            // Non-empty, NUL-free symlink target (bytes 1..=0x7f).
            proptest::collection::vec(1u8..=0x7f, 1..8),
        ),
        0..8,
    )
}

/// Keep the first decision per name index so reversing the list cannot change
/// which content is bound to a name.
fn dedup_first(decisions: Vec<Decision>) -> Vec<Decision> {
    let mut seen: Vec<u8> = Vec::new();
    let mut out = Vec::new();
    for decision in decisions {
        if !seen.contains(&decision.0) {
            seen.push(decision.0);
            out.push(decision);
        }
    }
    out
}

fn build_tree(decisions: Vec<Decision>) -> Tree {
    let mut seen: Vec<u8> = Vec::new();
    let mut entries: Vec<TreeEntry<FileContent, ContentId>> = Vec::new();
    for (name_idx, kind, executable, seed, target) in decisions {
        if seen.contains(&name_idx) {
            continue;
        }
        seen.push(name_idx);
        let name = match NAMES.get(name_idx as usize).copied() {
            Some(name) => name,
            None => unreachable!("name index is generated in range"),
        };
        let content = match kind {
            0 => TreeEntryContent::File(FileContent {
                content: ContentId::of_blob(&[seed]),
                executable,
            }),
            1 => TreeEntryContent::Tree(ContentId::of_tree(&[seed])),
            _ => TreeEntryContent::Symlink {
                target: target.into_boxed_slice(),
            },
        };
        let entry = match TreeEntry::new(name, content) {
            Ok(entry) => entry,
            Err(_) => unreachable!("generated tree entry is valid by construction"),
        };
        entries.push(entry);
    }
    match Tree::new(entries) {
        Ok(tree) => tree,
        Err(_) => unreachable!("generated tree has distinct, valid names"),
    }
}

proptest! {
    #[test]
    fn tree_round_trips_through_its_manifest(decisions in tree_decisions()) {
        let tree = build_tree(decisions);
        let manifest = match tree.manifest() {
            Ok(manifest) => manifest,
            Err(_) => unreachable!("a valid tree always encodes a manifest"),
        };
        let decoded = match Tree::from_manifest(&manifest) {
            Ok(decoded) => decoded,
            Err(_) => unreachable!("a canonical manifest round-trips"),
        };
        prop_assert_eq!(decoded.id(), tree.id());
        prop_assert_eq!(decoded.entries(), tree.entries());
    }

    #[test]
    fn tree_identity_is_invariant_under_entry_permutation(decisions in tree_decisions()) {
        let canonical = dedup_first(decisions);
        let mut reversed = canonical.clone();
        reversed.reverse();

        let first = build_tree(canonical);
        let second = build_tree(reversed);

        // `Tree::new` sorts entries before hashing, so order does not affect
        // content identity.
        prop_assert_eq!(first.id(), second.id());
    }

    #[test]
    fn decoding_arbitrary_bytes_as_a_tree_manifest_never_panics(
        bytes in proptest::collection::vec(any::<u8>(), 0..512)
    ) {
        // `Tree::from_manifest` reads untrusted stored bytes; it must return
        // an error rather than index, allocate without bound, or panic. The
        // assertion is a tautology — the real check is that the call returns.
        let result = Tree::from_manifest(&bytes);
        prop_assert!(result.is_ok() || result.is_err());
    }
}
