//! A domain's key-set document: the keys whose signatures speak for the
//! domain, how many must sign, and the generation that advances when the
//! set changes. Signed by the registry's root key.

use std::collections::BTreeSet;

use super::super::keys::{Detached, Public, Signing};
use super::{GENERATION, KEY, SIG, THRESHOLD};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeySet {
    pub(super) generation: u64,
    pub(super) threshold: u32,
    pub(super) keys: BTreeSet<Public>,
    pub(super) signature: Detached,
}

impl KeySet {
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn threshold(&self) -> u32 {
        self.threshold
    }

    #[must_use]
    pub fn keys(&self) -> &BTreeSet<Public> {
        &self.keys
    }

    #[must_use]
    pub const fn signature(&self) -> &Detached {
        &self.signature
    }

    /// The bytes the root key's signature covers.
    #[must_use]
    pub fn render_unsigned(&self) -> String {
        unsigned(self.generation, self.threshold, &self.keys)
    }

    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "{}\n{SIG} {}\n",
            self.render_unsigned(),
            self.signature.spelling()
        )
    }

    #[must_use]
    pub fn from_claims(
        generation: u64,
        threshold: u32,
        keys: BTreeSet<Public>,
        signature: Detached,
    ) -> Self {
        Self {
            generation,
            threshold,
            keys,
            signature,
        }
    }

    /// The key set those claims describe, signed by `root`.
    #[must_use]
    pub fn signed(generation: u64, threshold: u32, keys: BTreeSet<Public>, root: &Signing) -> Self {
        let unsigned = unsigned(generation, threshold, &keys);
        Self::from_claims(generation, threshold, keys, root.sign(unsigned.as_bytes()))
    }
}

fn unsigned(generation: u64, threshold: u32, keys: &BTreeSet<Public>) -> String {
    let mut document = format!("{GENERATION} {generation}\n{THRESHOLD} {threshold}");
    for key in keys {
        document.push_str(&format!("\n{KEY} {}", key.spelling()));
    }
    document
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_sets_round_trip_and_refuse_trailing_lines() {
        let root = Signing::from_seed([1; ed25519_dalek::SECRET_KEY_LENGTH]);
        let publisher = Signing::from_seed([3; ed25519_dalek::SECRET_KEY_LENGTH]);
        let signed = KeySet::signed(2, 1, BTreeSet::from([publisher.public()]), &root);
        let rendered = signed.render();
        assert_eq!(
            KeySet::parse(&rendered, pith_diag::ByteOffset(0)),
            Ok(signed)
        );
        let trailing = format!("{rendered}extra\n");
        assert!(KeySet::parse(&trailing, pith_diag::ByteOffset(0)).is_err());
    }
}
