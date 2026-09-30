//! A release line: what one version of one subject is, which publisher
//! key asserts it, and where the signature over that claim sits.

use pith_hir::ModuleVersion;
use pith_ids::ContentDigest;

use super::super::keys::{Detached, Public, Signing};
use super::super::range;
use super::{ADMITTED, BY, DIGEST_PREFIX, Pin, RELEASE, REQUIRES, REVISION, SIG, SUBPATH, TREE};
use crate::ModuleRequirement;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub(super) version: ModuleVersion,
    pub(super) requires: Box<[ModuleRequirement]>,
    pub(super) pin: Pin,
    pub(super) tree: ContentDigest,
    pub(super) signer: Public,
    pub(super) signature: Detached,
    pub(super) admitted: u64,
}

impl Release {
    #[must_use]
    pub fn version(&self) -> &ModuleVersion {
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

    /// The key the line names as its signer.
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
        unsigned(
            &self.version,
            &self.requires,
            &self.pin,
            self.tree,
            &self.signer,
        )
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
        version: ModuleVersion,
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

    /// The release those claims describe, signed: the signature covers
    /// everything but itself and the admission time, which arrive after it.
    #[must_use]
    pub fn signed(
        version: ModuleVersion,
        requires: Box<[ModuleRequirement]>,
        pin: Pin,
        tree: ContentDigest,
        signer: &Signing,
        admitted: u64,
    ) -> Self {
        let unsigned = unsigned(&version, &requires, &pin, tree, &signer.public());
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

fn unsigned(
    version: &ModuleVersion,
    requires: &[ModuleRequirement],
    pin: &Pin,
    tree: ContentDigest,
    signer: &Public,
) -> String {
    let mut line = format!("{RELEASE} {}", version.canonical_spelling());
    for requirement in requires {
        line.push_str(&format!(
            " {REQUIRES} {} {}",
            requirement.subject,
            range::render(&requirement.range)
        ));
    }
    line.push_str(&format!(" {REVISION} {}", pin.revision));
    if let Some(subpath) = &pin.subpath {
        line.push_str(&format!(
            " {SUBPATH} {}",
            pith_diag::text::token(subpath.as_ref())
        ));
    }
    line.push_str(&format!(
        " {TREE} {DIGEST_PREFIX}{} {BY} {}",
        tree,
        signer.spelling()
    ));
    line
}

#[cfg(test)]
mod tests {
    use super::super::Line;
    use super::*;

    fn signed_release() -> Release {
        let publisher = Signing::from_seed([3; ed25519_dalek::SECRET_KEY_LENGTH]);
        Release::signed(
            ModuleVersion::parse("1.2.0").unwrap(),
            Box::new([ModuleRequirement {
                subject: pith_hir::ModuleSubject::parse("example/dep").unwrap(),
                range: pith_hir::VersionRange::Between {
                    lower: pith_hir::VersionBound {
                        version: ModuleVersion::parse("1.2").unwrap(),
                        inclusive: true,
                    },
                    upper: pith_hir::VersionBound {
                        version: ModuleVersion::parse("2.0").unwrap(),
                        inclusive: false,
                    },
                },
            }]),
            Pin::at_subpath("abc123", "nested/module"),
            ContentDigest::of_bytes(b"tree"),
            &publisher,
            1_757_000_000,
        )
    }

    #[test]
    fn release_lines_round_trip() {
        let release = signed_release();
        let rendered = release.render();
        let parsed =
            match Line::parse(&rendered, pith_diag::ByteOffset(0)).expect("the line parses") {
                Line::Release(parsed) => parsed,
                other => unreachable!("a release line parses as one: {other:?}"),
            };
        assert_eq!(parsed, release);
        assert_eq!(parsed.render(), rendered);
    }

    #[test]
    fn a_re_spelled_line_with_equal_values_is_equal() {
        let release = signed_release();
        let spaced = release.render().replace(" sig ", "  sig  ");
        let parsed = match Line::parse(&spaced, pith_diag::ByteOffset(0)).expect("the line parses")
        {
            Line::Release(parsed) => parsed,
            _ => unreachable!("a release line parses as one"),
        };
        assert_eq!(parsed, release);
    }

    #[test]
    fn a_signature_covers_the_claims_not_itself() {
        let release = signed_release();
        let publisher = Signing::from_seed([3; ed25519_dalek::SECRET_KEY_LENGTH]);
        assert!(
            publisher
                .public()
                .verify(release.render_unsigned().as_bytes(), release.signature()),
            "the publisher's key verifies the unsigned rendering"
        );
    }
}
