//! Lock entries: coordinates bound to content, origin as evidence (decision 0039).
//!
//! A lock entry is a pair plus evidence: the package version (what was
//! chosen), the content identity of the source it resolved to (what the
//! choice means), and the origin it was resolved from (where that
//! happened). The binding is the entry; the origin is not part of either
//! identity. The content side is intrinsic, in Software Heritage's sense —
//! computed from the bytes read, valid wherever those bytes are found
//! again — so content that no longer matches the binding is drift to
//! report, never an identity transition. What makes a binding trustworthy
//! is the artifacts-and-trust question 0039 leaves open; the entry here
//! records the binding and checks it, and witnesses nothing.

use pith_diag::PithResult;
use pith_ids::ContentId;

use crate::diag;
use crate::identity::PackageVersion;

/// Where a resolution happened: the remote or local source the package was
/// resolved from. Evidence for provenance, not part of the binding — the
/// same content reached through two origins is one resolution recorded
/// twice, not two resolutions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    Registry(Box<str>),
    Forge(Box<str>),
    LocalPath(Box<str>),
}

impl std::fmt::Display for Origin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registry(url) => write!(formatter, "registry {url}"),
            Self::Forge(url) => write!(formatter, "forge {url}"),
            Self::LocalPath(path) => write!(formatter, "local path {path}"),
        }
    }
}

/// What a lock entry pins: one package version, one content identity for the
/// source it resolved to. Equality on the binding is the statement "these
/// coordinates resolve to this content," with no origin in it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Binding {
    pub package: PackageVersion,
    pub source: ContentId,
}

/// One lock entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockEntry {
    pub package: PackageVersion,
    pub source: ContentId,
    pub origin: Origin,
}

impl LockEntry {
    #[must_use]
    pub fn new(package: PackageVersion, source: ContentId, origin: Origin) -> Self {
        Self {
            package,
            source,
            origin,
        }
    }

    /// The binding this entry records, without the origin evidence.
    #[must_use]
    pub fn binding(&self) -> Binding {
        Binding {
            package: self.package.clone(),
            source: self.source,
        }
    }

    /// Check freshly resolved content against the binding. Matching content
    /// confirms the entry; different content under the same coordinates is
    /// drift — the failure a lock binding exists to catch when a domain does
    /// not honor immutability — or a new resolution to record, and in both
    /// cases it is reported rather than absorbed (0039).
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming the coordinates and both
    /// content identities when `resolved` differs from the bound source.
    pub fn verify_resolution(&self, resolved: ContentId) -> PithResult<()> {
        if resolved == self.source {
            return Ok(());
        }
        Err(diag(format!(
            "the lock binds `{}` in `{}` version {} to content `{}`, but resolution \
             from {} produced content `{}`: the domain's entry changed underneath \
             the coordinates, which is drift to report or a new resolution to \
             record, never the same resolution",
            self.package.identity().name(),
            self.package.identity().domain().as_str(),
            self.package.version(),
            self.source.digest(),
            self.origin,
            resolved.digest(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{DomainIdentity, PackageIdentity};

    fn package(version: &str) -> PackageVersion {
        PackageVersion::new(
            PackageIdentity::declare(DomainIdentity::new("pithpkgs"), "zlib"),
            version,
        )
    }

    fn source() -> ContentId {
        ContentId::of_blob(b"zlib-1.3.tar")
    }

    #[test]
    fn the_origin_is_evidence_not_identity() {
        // The same coordinates resolving to the same content through two
        // origins is one binding recorded twice. An origin that entered the
        // binding would make "what was chosen and what it resolves to"
        // depend on where it was downloaded from.
        let from_registry = LockEntry::new(
            package("1.3"),
            source(),
            Origin::Registry("pkgs.pith-lang.org".into()),
        );
        let from_mirror = LockEntry::new(
            package("1.3"),
            source(),
            Origin::Registry("mirror.example".into()),
        );
        assert_ne!(from_registry, from_mirror);
        assert_eq!(from_registry.binding(), from_mirror.binding());
    }

    #[test]
    fn a_version_bump_is_a_different_binding_over_one_package() {
        let before = LockEntry::new(
            package("1.3"),
            source(),
            Origin::Registry("pkgs.pith-lang.org".into()),
        );
        let after = LockEntry::new(
            package("1.3.1"),
            source(),
            Origin::Registry("pkgs.pith-lang.org".into()),
        );
        assert_ne!(before.binding(), after.binding());
        assert_eq!(
            before.binding().package.identity(),
            after.binding().package.identity()
        );
    }

    #[test]
    fn matching_content_verifies_and_drifting_content_is_reported() {
        let entry = LockEntry::new(
            package("1.3"),
            source(),
            Origin::Registry("pkgs.pith-lang.org".into()),
        );
        assert!(entry.verify_resolution(source()).is_ok());

        let drifted = ContentId::of_blob(b"zlib-1.3-republished");
        let error = entry
            .verify_resolution(drifted)
            .expect_err("different content under the same coordinates is drift");
        let message = error
            .iter()
            .next()
            .map(|diagnostic| diagnostic.message.0.to_string())
            .unwrap_or_default();
        assert!(
            message.contains("zlib") && message.contains("drift"),
            "the diagnostic should name the package and the drift: {message}"
        );
        assert!(
            message.contains(&source().digest().to_string())
                && message.contains(&drifted.digest().to_string()),
            "the diagnostic should carry both content identities: {message}"
        );
    }
}
