//! A withdrawal line: one version marked unfit, with a reason.

use pith_hir::ManifestVersion;

use super::super::keys::{Detached, Public, Signing};
use super::{BY, REASON, SIG, WITHDRAW};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Withdrawal {
    pub(super) version: ManifestVersion,
    pub(super) reason: Box<str>,
    pub(super) signer: Public,
    pub(super) signature: Detached,
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
        unsigned(&self.version, &self.reason, &self.signer)
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
        let unsigned = unsigned(&version, &reason, &issuer.public());
        Self::from_claims(
            version,
            reason,
            issuer.public(),
            issuer.sign(unsigned.as_bytes()),
        )
    }
}

fn unsigned(version: &ManifestVersion, reason: &str, signer: &Public) -> String {
    format!(
        "{WITHDRAW} {} {REASON} {} {BY} {}",
        version.canonical_spelling(),
        pith_diag::text::token(reason),
        signer.spelling()
    )
}
