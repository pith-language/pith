//! The index's documents, canonically rendered: a release line, a
//! withdrawal line, and a domain's key-set document. A signature covers
//! the canonical rendering of a document's values (everything before the
//! `sig` field); the registry's admission time follows it, asserted by
//! the host and covered by neither signature. Parsers live in [`super::parse`].

mod key_set;
mod release;
mod withdrawal;

pub use key_set::KeySet;
pub use release::Release;
pub use withdrawal::Withdrawal;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_prefix_is_the_hashers_name() {
        assert_eq!(DIGEST_PREFIX, format!("{}:", pith_ids::DIGEST_ALGORITHM));
    }
}
