//! Transparency-log witnesses for lock binding lines.
//!
//! The log is an append-only Merkle tree with distinct leaf and node hash
//! domains. Checkpoints contain an origin, tree size, and root digest.
//! Inclusion proofs recompute that root from one binding line and its sibling
//! path. Checkpoint signature verification is outside this module.

use pith_diag::PithResult;
use pith_ids::ContentDigest;

use crate::codec::digest_from_hex;
use crate::diag;

/// The digest domain for one leaf: a binding line. NUL-terminated so it is
/// self-delimiting against the record that follows, on the pattern
/// `pith-ids` applies to every digest kind it owns.
const LEAF_DOMAIN: &[u8] = b"phloem.log-leaf-v1\0";

/// The digest domain for one interior node over exactly two children.
const NODE_DOMAIN: &[u8] = b"phloem.log-node-v1\0";

/// A log's signed-tree-head shape: which log this is, how many records it
/// holds, and the root hash committing to them. The name follows Go's
/// spelling because the shape does. unlike Go's, it carries no signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    /// The log's declared identity, the thing a person's configuration
    /// pins when it pins a log.
    pub origin: Box<str>,
    /// The number of records the tree commits to.
    pub size: u64,
    /// The Merkle root over those records.
    pub root: ContentDigest,
}

impl Checkpoint {
    /// The checkpoint's written form, one datum per line: the origin, the
    /// size, and the root as hexadecimal.
    #[must_use]
    pub fn render(&self) -> String {
        format!("{}\n{}\n{}\n", self.origin, self.size, self.root)
    }

    /// Read a checkpoint from its written form.
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming what was found when the text
    /// is not an origin, a size, and a hexadecimal root.
    pub fn parse(text: &str) -> PithResult<Self> {
        let mut lines = text.lines();
        let (Some(origin), Some(size), Some(root), None) =
            (lines.next(), lines.next(), lines.next(), lines.next())
        else {
            return Err(diag(
                "a checkpoint is three lines: the log's origin, its size, and its root hash",
            ));
        };
        let size = size.parse::<u64>().map_err(|_| {
            diag(format!(
                "the checkpoint's size `{size}` is not a non-negative integer"
            ))
        })?;
        let root = digest_of(root)
            .ok_or_else(|| diag(format!("the checkpoint's root `{root}` is not a digest")))?;
        Ok(Self {
            origin: origin.into(),
            size,
            root,
        })
    }
}

/// An inclusion proof: which record the leaf is, and the sibling hashes
/// from that leaf to the root of a tree of a particular size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inclusion {
    pub index: u64,
    pub path: Box<[ContentDigest]>,
}

/// One leaf's hash: the record's bytes under the leaf domain.
#[must_use]
pub fn leaf_hash(record: &[u8]) -> ContentDigest {
    let mut leaf = LEAF_DOMAIN.to_vec();
    leaf.extend_from_slice(record);
    ContentDigest::of_bytes(&leaf)
}

fn node_hash(left: &ContentDigest, right: &ContentDigest) -> ContentDigest {
    let mut node = NODE_DOMAIN.to_vec();
    node.extend_from_slice(left.as_bytes());
    node.extend_from_slice(right.as_bytes());
    ContentDigest::of_bytes(&node)
}

fn digest_of(text: &str) -> Option<ContentDigest> {
    digest_from_hex(text)
}

/// The largest power of two strictly below `size`, for a size of at least
/// two, which is the split point the tree of `size` leaves folds at.
fn split_below(size: u64) -> u64 {
    // `size - 1` is nonzero, so the shift remains within the word.
    let shift = u64::BITS
        .saturating_sub(1)
        .saturating_sub(size.saturating_sub(1).leading_zeros());
    u64::checked_shl(1, shift).unwrap_or(0)
}

/// The Merkle root over `hashes`, folding left-to-right at powers of two
/// the transparent-log arrangement fixes. The empty tree has no root, and
/// [`MerkleTree::new`] refuses one, so every tree and checkpoint has
/// `size >= 1`.
fn root_of(hashes: &[ContentDigest]) -> ContentDigest {
    let Some((&leaf, rest)) = hashes.split_first() else {
        unreachable!("a log commits to at least one record")
    };
    if rest.is_empty() {
        return leaf;
    }
    let split = split_below(hashes.len() as u64) as usize;
    let (left, right) = hashes.split_at(split);
    node_hash(&root_of(left), &root_of(right))
}

/// The inclusion path for `index` in `hashes`, sibling hashes ordered from
/// the leaf's subtree outward.
fn path_of(index: u64, hashes: &[ContentDigest]) -> Vec<ContentDigest> {
    if hashes.len() == 1 {
        return Vec::new();
    }
    let split = split_below(hashes.len() as u64) as usize;
    let (left, right) = hashes.split_at(split);
    if (index as usize) < split {
        let mut path = path_of(index, left);
        path.push(root_of(right));
        path
    } else {
        let Some(right_index) = index.checked_sub(split as u64) else {
            unreachable!("the index is at least the split it was compared against")
        };
        let mut path = path_of(right_index, right);
        path.push(root_of(left));
        path
    }
}

/// The log's operator half: a tree over records that serves the root a
/// checkpoint names and the proofs a client verifies. A pith-operated log
/// and the prototype's fixture share this spelling. a third-party log
/// needs only the checkpoint and proof format above.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerkleTree {
    hashes: Box<[ContentDigest]>,
}

impl MerkleTree {
    /// Builds a Merkle tree over `records`.
    ///
    /// # Errors
    /// Returns a diagnostic when `records` is empty.
    pub fn new<'a>(records: impl IntoIterator<Item = &'a [u8]>) -> PithResult<Self> {
        let hashes: Box<[ContentDigest]> = records.into_iter().map(leaf_hash).collect();
        if hashes.is_empty() {
            return Err(diag(
                "a log commits to at least one record; this one was given none",
            ));
        }
        Ok(Self { hashes })
    }

    /// The root of the tree a checkpoint over these records names.
    #[must_use]
    pub fn root(&self) -> ContentDigest {
        root_of(&self.hashes)
    }

    /// The number of records the tree holds.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.hashes.len() as u64
    }

    /// The inclusion proof for the record at `index`.
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming the index and the size when
    /// the index is outside the tree.
    pub fn inclusion(&self, index: u64) -> PithResult<Inclusion> {
        if index >= self.size() {
            return Err(diag(format!(
                "the log holds {} records and was asked to prove record {index}",
                self.size()
            )));
        }
        Ok(Inclusion {
            index,
            path: path_of(index, &self.hashes).into(),
        })
    }
}

/// Verifies an inclusion proof against a checkpoint.
///
/// # Errors
/// Returns a diagnostic for an invalid index, path length, or root digest.
pub fn verify_inclusion(
    record: &[u8],
    proof: &Inclusion,
    checkpoint: &Checkpoint,
) -> PithResult<()> {
    if proof.index >= checkpoint.size {
        return Err(diag(format!(
            "the proof claims record {} of a log of {} records, and there is no such record",
            proof.index, checkpoint.size
        )));
    }
    let mut path = proof.path.iter();
    let computed = fold(proof.index, checkpoint.size, leaf_hash(record), &mut path)?;
    if let Some(extra) = path.next() {
        return Err(diag(format!(
            "the proof carries more hashes than a log of {} records accounts for; the first \
             extra names {}",
            checkpoint.size, extra
        )));
    }
    if computed != checkpoint.root {
        return Err(diag(format!(
            "the proof for record {} computes root `{computed}`, and the checkpoint for `{}` \
             names `{}`: the log's tree does not contain the record as claimed",
            proof.index, checkpoint.origin, checkpoint.root
        )));
    }
    Ok(())
}

/// Recomputes a Merkle root from a leaf and sibling path.
///
/// # Errors
/// Returns a diagnostic when the path ends before reaching the root.
fn fold(
    index: u64,
    size: u64,
    leaf: ContentDigest,
    path: &mut std::slice::Iter<'_, ContentDigest>,
) -> PithResult<ContentDigest> {
    if size == 1 {
        return Ok(leaf);
    }
    let split = split_below(size);
    if index < split {
        let left = fold(index, split, leaf, path)?;
        let Some(right) = path.next() else {
            return short(size);
        };
        Ok(node_hash(&left, right))
    } else {
        let Some(right_index) = index.checked_sub(split) else {
            unreachable!("the index is at least the split it was compared against")
        };
        let Some(right_size) = size.checked_sub(split) else {
            unreachable!("the split is below the size it was derived from")
        };
        let right = fold(right_index, right_size, leaf, path)?;
        let Some(left) = path.next() else {
            return short(size);
        };
        Ok(node_hash(left, &right))
    }
}

fn short(size: u64) -> PithResult<ContentDigest> {
    Err(diag(format!(
        "the proof carries fewer hashes than a log of {size} records accounts for"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn records() -> Vec<Vec<u8>> {
        (0..13)
            .map(|n| format!("bind pithpkgs lib{n} 1.0 [] blake3:{n:064x}").into_bytes())
            .collect()
    }

    fn tree_and_checkpoint(origin: &str) -> (MerkleTree, Checkpoint) {
        let tree = MerkleTree::new(records().iter().map(Vec::as_slice)).unwrap();
        let checkpoint = Checkpoint {
            origin: origin.into(),
            size: tree.size(),
            root: tree.root(),
        };
        (tree, checkpoint)
    }

    #[test]
    fn every_record_in_the_tree_verifies_against_its_checkpoint() {
        let (tree, checkpoint) = tree_and_checkpoint("logs.pith-lang.org");
        for index in 0..tree.size() {
            let proof = tree.inclusion(index).unwrap();
            let records = records();
            let record = records.get(index as usize).map(Vec::as_slice).unwrap();
            verify_inclusion(record, &proof, &checkpoint)
                .unwrap_or_else(|error| unreachable!("record {index} verifies: {error:?}"));
        }
    }

    #[test]
    fn a_proof_for_other_records_fails_naming_both_roots() {
        let (tree, checkpoint) = tree_and_checkpoint("logs.pith-lang.org");
        let proof = tree.inclusion(4).unwrap();
        let error = verify_inclusion(
            records().get(9).map(Vec::as_slice).unwrap(),
            &proof,
            &checkpoint,
        )
        .unwrap_err();
        let message = error
            .iter()
            .next()
            .map(|diagnostic| diagnostic.message.0.to_string())
            .unwrap_or_default();
        assert!(
            message.contains("computes root") && message.contains("logs.pith-lang.org"),
            "the diagnostic names the computed root and the checkpoint's log: {message}"
        );
    }

    #[test]
    fn a_tampered_leaf_line_does_not_reach_the_root_it_claims() {
        let (tree, checkpoint) = tree_and_checkpoint("logs.pith-lang.org");
        let proof = tree.inclusion(4).unwrap();
        let original = String::from_utf8(records().get(4).unwrap().clone()).unwrap();
        let tampered = original.replacen("1.0", "1.1", 1);
        assert_ne!(tampered.as_bytes(), original.as_bytes());
        let error = verify_inclusion(tampered.as_bytes(), &proof, &checkpoint).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.0.contains("computes root")),
            "the tampered line fails the proof: {error:?}"
        );
    }

    #[test]
    fn a_truncated_path_and_an_index_outside_the_tree_are_refused() {
        let (tree, checkpoint) = tree_and_checkpoint("logs.pith-lang.org");
        let proof = tree.inclusion(4).unwrap();
        let truncated = Inclusion {
            index: proof.index,
            path: proof.path.get(..1).unwrap().to_vec().into(),
        };
        let error = verify_inclusion(
            records().get(4).map(Vec::as_slice).unwrap(),
            &truncated,
            &checkpoint,
        )
        .unwrap_err();
        assert!(
            error.iter().any(|d| d.message.0.contains("fewer hashes")),
            "the diagnostic names the short path: {error:?}"
        );

        let outside = Inclusion {
            index: checkpoint.size,
            path: proof.path.clone(),
        };
        let error = verify_inclusion(
            records().get(4).map(Vec::as_slice).unwrap(),
            &outside,
            &checkpoint,
        )
        .unwrap_err();
        assert!(
            error.iter().any(|d| d.message.0.contains("no such record")),
            "the diagnostic names the size and the index: {error:?}"
        );
    }

    #[test]
    fn a_checkpoint_round_trips_through_its_written_form() {
        let (_, checkpoint) = tree_and_checkpoint("logs.pith-lang.org");
        assert_eq!(Checkpoint::parse(&checkpoint.render()).unwrap(), checkpoint);
        assert!(Checkpoint::parse("one line only").is_err());
        let bad_root = checkpoint
            .render()
            .replace(&checkpoint.root.to_string(), "zz");
        assert!(Checkpoint::parse(&bad_root).is_err());
    }

    #[test]
    fn the_root_moves_when_any_record_moves_and_a_log_never_starts_empty() {
        let base = MerkleTree::new(records().iter().map(Vec::as_slice)).unwrap();
        let mut moved = records();
        let sixth = format!("bind pithpkgs lib6 1.0 [] blake3:{:064x}", 0xdead).into_bytes();
        if let Some(slot) = moved.get_mut(6) {
            *slot = sixth;
        }
        let moved = MerkleTree::new(moved.iter().map(Vec::as_slice)).unwrap();
        assert_ne!(base.root(), moved.root());
        assert_eq!(
            base.root(),
            MerkleTree::new(records().iter().map(Vec::as_slice))
                .unwrap()
                .root()
        );
        assert!(MerkleTree::new(Vec::<&[u8]>::new().iter().copied()).is_err());
    }
}
