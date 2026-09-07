//! The index's documents: a release line, a withdrawal line, and a
//! domain's key-set document, canonically rendered.
//!
//! A signature covers the canonical rendering of a document's values —
//! everything before the `sig` field — so equal values verify equal and
//! any edited value does not. The registry's admission time follows the
//! signature, asserted by the host rather than the publisher and never
//! covered by either.

use std::collections::BTreeSet;

use pith_hir::ManifestVersion;
use pith_ids::ContentDigest;

use super::keys::{Detached, Public, Signing};
use super::token::quoted;
use crate::ModuleRequirement;

pub(crate) const RELEASE: &str = "release";
pub(crate) const WITHDRAW: &str = "withdraw";
pub(crate) const REQUIRES: &str = "requires";
pub(crate) const REVISION: &str = "revision";
pub(crate) const SUBPATH: &str = "subpath";
pub(crate) const TREE: &str = "tree";
pub(crate) const BY: &str = "by";
pub(crate) const SIG: &str = "sig";
pub(crate) const ADMITTED: &str = "admitted";
pub(crate) const REASON: &str = "reason";
pub(crate) const GENERATION: &str = "generation";
pub(crate) const THRESHOLD: &str = "threshold";
pub(crate) const KEY: &str = "key";

/// The digest prefix, bound to the hasher's own name by test so the
/// written spelling and the algorithm cannot drift apart.
pub(crate) const DIGEST_PREFIX: &str = "blake3:";

/// One line of a subject's index file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Release(Release),
    Withdrawal(Withdrawal),
}

impl Line {
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::Release(release) => release.render(),
            Self::Withdrawal(withdrawal) => withdrawal.render(),
        }
    }
}

/// A revision pin: the immutable revision a tree was measured from, as
/// the serving host spells it, and the subpath holding the module root
/// when the revision holds more than the module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    revision: Box<str>,
    subpath: Option<Box<str>>,
}

impl Pin {
    #[must_use]
    pub fn new(revision: impl Into<Box<str>>) -> Self {
        Self {
            revision: revision.into(),
            subpath: None,
        }
    }

    #[must_use]
    pub fn at_subpath(revision: impl Into<Box<str>>, subpath: impl Into<Box<str>>) -> Self {
        Self {
            revision: revision.into(),
            subpath: Some(subpath.into()),
        }
    }

    #[must_use]
    pub fn revision(&self) -> &str {
        &self.revision
    }

    #[must_use]
    pub fn subpath(&self) -> Option<&str> {
        self.subpath.as_deref()
    }
}

/// A release line: what one version of one subject is, which publisher
/// key asserts it, and where the signature over that claim sits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    version: ManifestVersion,
    requires: Box<[ModuleRequirement]>,
    pin: Pin,
    tree: ContentDigest,
    signer: Public,
    signature: Detached,
    admitted: u64,
}

impl Release {
    #[must_use]
    pub fn version(&self) -> &ManifestVersion {
        &self.version
    }

    #[must_use]
    pub fn requires(&self) -> &[ModuleRequirement] {
        &self.requires
    }

    #[must_use]
    pub const fn pin(&self) -> &Pin {
        &self.pin
    }

    #[must_use]
    pub const fn tree(&self) -> ContentDigest {
        self.tree
    }

    /// The key the line names as its signer, so a refusal can name the
    /// key and the set it is absent from.
    #[must_use]
    pub const fn signer(&self) -> &Public {
        &self.signer
    }

    #[must_use]
    pub const fn signature(&self) -> &Detached {
        &self.signature
    }

    /// When the registry admitted the entry, in seconds since the unix
    /// epoch. Asserted by the host, covered by neither signature.
    #[must_use]
    pub const fn admitted(&self) -> u64 {
        self.admitted
    }

    /// The bytes a publisher's signature covers: the canonical rendering
    /// of every claim the publisher makes.
    #[must_use]
    pub fn render_unsigned(&self) -> String {
        let mut line = format!("{RELEASE} {}", self.version.canonical_spelling());
        for requirement in &self.requires {
            line.push_str(&format!(
                " {REQUIRES} {} {}",
                requirement.subject,
                super::range::render(&requirement.range)
            ));
        }
        line.push_str(&format!(" {REVISION} {}", self.pin.revision));
        if let Some(subpath) = &self.pin.subpath {
            line.push_str(&format!(" {SUBPATH} {}", quoted(subpath)));
        }
        line.push_str(&format!(
            " {TREE} {DIGEST_PREFIX}{} {BY} {}",
            self.tree,
            self.signer.spelling()
        ));
        line
    }

    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "{} {SIG} {} {ADMITTED} {}",
            self.render_unsigned(),
            self.signature.spelling(),
            self.admitted
        )
    }

    /// Assembles a release from derived claims, a signature over
    /// [`Self::render_unsigned`], and the host's admission time.
    #[must_use]
    pub fn from_claims(
        version: ManifestVersion,
        requires: Box<[ModuleRequirement]>,
        pin: Pin,
        tree: ContentDigest,
        signer: Public,
        signature: Detached,
        admitted: u64,
    ) -> Self {
        Self {
            version,
            requires,
            pin,
            tree,
            signer,
            signature,
            admitted,
        }
    }

    /// The release those claims describe, signed: the signature covers the
    /// canonical rendering of everything but itself and the admission
    /// time, which arrive after it.
    #[must_use]
    pub fn signed(
        version: ManifestVersion,
        requires: Box<[ModuleRequirement]>,
        pin: Pin,
        tree: ContentDigest,
        signer: &Signing,
        admitted: u64,
    ) -> Self {
        let unsigned = Self::from_claims(
            version.clone(),
            requires.clone(),
            pin.clone(),
            tree,
            signer.public(),
            signer.sign(&[]),
            admitted,
        )
        .render_unsigned();
        Self::from_claims(
            version,
            requires,
            pin,
            tree,
            signer.public(),
            signer.sign(unsigned.as_bytes()),
            admitted,
        )
    }
}

/// A withdrawal line: one version marked unfit, with a reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Withdrawal {
    version: ManifestVersion,
    reason: Box<str>,
    signer: Public,
    signature: Detached,
}

impl Withdrawal {
    #[must_use]
    pub fn version(&self) -> &ManifestVersion {
        &self.version
    }

    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    #[must_use]
    pub const fn signer(&self) -> &Public {
        &self.signer
    }

    #[must_use]
    pub const fn signature(&self) -> &Detached {
        &self.signature
    }

    /// The bytes the issuer's signature covers.
    #[must_use]
    pub fn render_unsigned(&self) -> String {
        format!(
            "{WITHDRAW} {} {REASON} {} {BY} {}",
            self.version.canonical_spelling(),
            quoted(&self.reason),
            self.signer.spelling()
        )
    }

    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "{} {SIG} {}",
            self.render_unsigned(),
            self.signature.spelling()
        )
    }

    #[must_use]
    pub fn from_claims(
        version: ManifestVersion,
        reason: Box<str>,
        signer: Public,
        signature: Detached,
    ) -> Self {
        Self {
            version,
            reason,
            signer,
            signature,
        }
    }

    /// The withdrawal those claims describe, signed by `issuer`.
    #[must_use]
    pub fn signed(version: ManifestVersion, reason: Box<str>, issuer: &Signing) -> Self {
        let unsigned = Self::from_claims(
            version.clone(),
            reason.clone(),
            issuer.public(),
            issuer.sign(&[]),
        )
        .render_unsigned();
        Self::from_claims(
            version,
            reason,
            issuer.public(),
            issuer.sign(unsigned.as_bytes()),
        )
    }
}

/// A domain's key-set document: the keys whose signatures speak for the
/// domain, how many must sign, and the generation that advances when the
/// set changes. Signed by the registry's root key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeySet {
    generation: u64,
    threshold: u32,
    keys: BTreeSet<Public>,
    signature: Detached,
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
        let mut document = format!(
            "{GENERATION} {}\n{THRESHOLD} {}",
            self.generation, self.threshold
        );
        for key in &self.keys {
            document.push_str(&format!("\n{KEY} {}", key.spelling()));
        }
        document
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
        let unsigned = Self::from_claims(generation, threshold, keys.clone(), root.sign(&[]))
            .render_unsigned();
        Self::from_claims(generation, threshold, keys, root.sign(unsigned.as_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_prefix_is_the_hashers_name() {
        assert_eq!(DIGEST_PREFIX, format!("{}:", pith_ids::DIGEST_ALGORITHM));
    }
}
