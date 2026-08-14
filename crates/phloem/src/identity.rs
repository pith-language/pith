//! Package identity and version coordinates (decision 0039).
//!
//! A package's identity is an author-declared name inside a declared domain:
//! the pair (domain identity, package name). The construction parallels
//! 0023's `RuleIdentity`, a module identity plus a declaration name, because
//! the two questions are the same shape: a stable coordinate of a declaration
//! site that survives changes to what is declared there. The identity is
//! declared, not computed — it fills 0005's semantic-identity slot for this
//! domain — so it survives version bumps, metadata changes, platform and
//! toolchain changes, and source moves the domain's resolution survives, and
//! only a rename breaks it.

use std::cmp::Ordering;

use pith_core::Value;
use pith_diag::PithResult;

use crate::diag;

/// A namespace authority a package is named within: either a first-party
/// library's namespace or a remote source identity such as a registry or a
/// forge. A distinct type from a bare string so a domain name does not stand
/// where a package name is wanted.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DomainIdentity(Box<str>);

impl DomainIdentity {
    /// Declare a domain by its name.
    #[must_use]
    pub fn new(name: impl Into<Box<str>>) -> Self {
        Self(name.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A package's identity: a declared name in a declared domain. Two packages
/// are the same package exactly when both halves agree; nothing else — not a
/// version, not a description digest, not a resolved source — enters the
/// comparison.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageIdentity {
    domain: DomainIdentity,
    name: Box<str>,
}

impl PackageIdentity {
    /// Declare a package's identity. The declaration is the identity: no
    /// content is read and no digest is taken.
    #[must_use]
    pub fn declare(domain: DomainIdentity, name: impl Into<Box<str>>) -> Self {
        Self {
            domain,
            name: name.into(),
        }
    }

    #[must_use]
    pub fn domain(&self) -> &DomainIdentity {
        &self.domain
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// A nominal value carrying this identity's package name. The domain is
    /// the declaration site's fact and is not part of the value a description
    /// records; the identity pair stays in the types that compare packages.
    #[must_use]
    pub fn name_value(&self) -> Value {
        Value::Text(self.name.clone())
    }
}

/// A package version: the identity plus a version, in the format and
/// comparison the domain declares. This is the thing a constraint ranges
/// over and a lock names, and it is deliberately a level below the package:
/// an upgrade is a relation between two versions of one identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageVersion {
    identity: PackageIdentity,
    version: Box<str>,
}

impl PackageVersion {
    #[must_use]
    pub fn new(identity: PackageIdentity, version: impl Into<Box<str>>) -> Self {
        Self {
            identity,
            version: version.into(),
        }
    }

    #[must_use]
    pub fn identity(&self) -> &PackageIdentity {
        &self.identity
    }

    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Compare two versions of one package under `scheme`. Versions of
    /// different packages have no ordering to report — the upgrade relation
    /// is between versions of one identity — so mismatched identities are a
    /// diagnostic naming both.
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming both identities when they
    /// differ.
    pub fn compare(&self, scheme: &dyn VersionScheme, other: &Self) -> PithResult<Ordering> {
        if self.identity != other.identity {
            return Err(diag(format!(
                "cannot compare versions of different packages: `{}` in `{}` versus `{}` in `{}`",
                self.identity.name,
                self.identity.domain.as_str(),
                other.identity.name,
                other.identity.domain.as_str(),
            )));
        }
        Ok(scheme.compare(&self.version, &other.version))
    }
}

/// The version format and comparison a domain declares. 0039 leaves the
/// format to the domain — semver for most, Debian's epoch-and-tilde ordering
/// for a deb domain — so phloem takes the ordering as a declaration rather
/// than baking one in. Version-range semantics over these comparisons are a
/// later record's work.
pub trait VersionScheme {
    /// How `left` and `right` order. Total: two spellings a domain accepts
    /// as versions have an ordering, whatever the domain's rule is.
    fn compare(&self, left: &str, right: &str) -> Ordering;
}

/// A dot-separated numeric scheme, the comparison most domains declare:
/// segments compare numerically, a missing segment is zero. A non-numeric
/// segment compares bytewise, so the ordering stays total without inventing
/// precedence the domain did not declare.
#[derive(Clone, Copy, Debug, Default)]
pub struct NumericSegments;

impl VersionScheme for NumericSegments {
    fn compare(&self, left: &str, right: &str) -> Ordering {
        let mut left = left.split('.');
        let mut right = right.split('.');
        loop {
            let ordering = match (left.next(), right.next()) {
                (None, None) => return Ordering::Equal,
                // A missing segment is zero, so only a nonzero segment on
                // the present side decides; trailing zeros are equal.
                (None, Some(present)) => zero().cmp(&segment(present)),
                (Some(present), None) => segment(present).cmp(&zero()),
                (Some(left_segment), Some(right_segment)) => {
                    segment(left_segment).cmp(&segment(right_segment))
                }
            };
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
    }
}

/// The zero segment, against which a present segment is compared when the
/// other side has run out.
fn zero() -> Segment<'static> {
    Segment::Number(0, "0")
}

/// One segment as comparable data: numeric segments compare by value, any
/// other segment by bytes. The two kinds never cross-compare because
/// `Number` sorts before `Text`, keeping the ordering total when a domain
/// spells a segment the numeric rule does not parse.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Segment<'a> {
    Number(i64, &'a str),
    Text(&'a str),
}

fn segment(raw: &str) -> Segment<'_> {
    match raw.parse::<i64>() {
        Ok(number) => Segment::Number(number, raw),
        Err(_) => Segment::Text(raw),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(name: &str) -> PackageIdentity {
        PackageIdentity::declare(DomainIdentity::new("pithpkgs"), name)
    }

    #[test]
    fn a_package_in_another_domain_is_another_package() {
        let pithpkgs = PackageIdentity::declare(DomainIdentity::new("pithpkgs"), "zlib");
        let deb = PackageIdentity::declare(DomainIdentity::new("deb"), "zlib");
        assert_ne!(pithpkgs, deb);
    }

    #[test]
    fn numeric_segments_compare_numerically_not_lexicographically() {
        let scheme = NumericSegments;
        assert_eq!(scheme.compare("1.2", "1.10"), Ordering::Less);
        assert_eq!(scheme.compare("1.10", "1.2"), Ordering::Greater);
        assert_eq!(scheme.compare("1.0", "1"), Ordering::Equal);
        assert_eq!(scheme.compare("2.0", "1.9"), Ordering::Greater);
    }

    #[test]
    fn versions_of_different_packages_have_no_ordering() {
        let first = PackageVersion::new(identity("openssl"), "1.0");
        let second = PackageVersion::new(identity("zlib"), "1.0");
        let error = first.compare(&NumericSegments, &second);
        assert!(
            error.is_err(),
            "an ordering across packages would read as an upgrade"
        );
    }

    #[test]
    fn versions_of_one_package_order_under_the_declared_scheme() {
        let old = PackageVersion::new(identity("openssl"), "1.1.1");
        let new = PackageVersion::new(identity("openssl"), "1.1.2");
        assert_eq!(
            old.compare(&NumericSegments, &new)
                .unwrap_or(Ordering::Equal),
            Ordering::Less
        );
    }
}
