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

use pith_core::{Type, Value};
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
/// than baking one in. Version-range semantics over these comparisons are
/// declared values over the ordering (0040).
pub trait VersionScheme {
    /// How `left` and `right` order. Total: two spellings a domain accepts
    /// as versions have an ordering, whatever the domain's rule is.
    fn compare(&self, left: &str, right: &str) -> Ordering;
}

/// A boxed scheme is a scheme, so a registry can hold its orderings behind
/// one pointer type.
impl<T: VersionScheme + ?Sized> VersionScheme for Box<T> {
    fn compare(&self, left: &str, right: &str) -> Ordering {
        (**self).compare(left, right)
    }
}

/// The nominal type a request carries to name the declared ordering it runs
/// under: `phloem.VersionScheme` over the scheme's declared name.
pub const VERSION_SCHEME: &str = "phloem.VersionScheme";

/// The declared name of [`NumericSegments`].
pub const NUMERIC_SEGMENTS: &str = "numeric-segments";

/// The declared name of [`Debian`].
pub const DEBIAN: &str = "debian";

/// The version-scheme type: nominal over the scheme's declared name.
#[must_use]
pub fn version_scheme_type() -> Type {
    Type::Nominal {
        name: VERSION_SCHEME.into(),
    }
}

/// A version-scheme value naming `scheme`. The name, not the comparison, is
/// what travels: the name is the identity a computation key covers, and the
/// ordering it resolves to is looked up in the registered schemes the way a
/// toolchain value names a driver whose closure the rule holds. A name whose
/// ordering changes is a declaration that changed its meaning, on the terms
/// a package name changing its meaning is domain policy to enforce.
#[must_use]
pub fn version_scheme_value(scheme: &str) -> Value {
    Value::Nominal {
        name: VERSION_SCHEME.into(),
        representation: Box::new(Value::Text(scheme.into())),
    }
}

/// The declared name a version-scheme value carries.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming what was found when the value is
/// not a version-scheme value.
pub fn version_scheme_name(value: &Value) -> PithResult<&str> {
    match value {
        Value::Nominal {
            name,
            representation,
        } if name.as_ref() == VERSION_SCHEME => match representation.as_ref() {
            Value::Text(text) => Ok(text),
            other => Err(diag(format!(
                "a {VERSION_SCHEME} value carried {other:?} rather than a text"
            ))),
        },
        other => Err(diag(format!(
            "expected a {VERSION_SCHEME} value, found {}",
            other.describe()
        ))),
    }
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

/// Debian version comparison: `[epoch:]upstream[-revision]`, the scheme a
/// deb domain declares (0039 names it as the other canonical example beside
/// semver). The epoch compares numerically and dominates everything; the
/// upstream part compares with the Debian rule, where `~` sorts before
/// anything including the empty string (so `1.0~rc1` precedes `1.0`); the
/// revision compares numerically. The point of the scheme here is not
/// fidelity to dpkg but a second declared ordering with genuinely different
/// spellings, over which the range algebra in `constraint` must behave
/// identically (0040).
#[derive(Clone, Copy, Debug, Default)]
pub struct Debian;

impl VersionScheme for Debian {
    fn compare(&self, left: &str, right: &str) -> Ordering {
        let ((left_epoch, left_rest), (right_epoch, right_rest)) =
            (split_epoch(left), split_epoch(right));
        left_epoch
            .cmp(&right_epoch)
            .then_with(|| compare_upstream(left_rest, right_rest))
            .then_with(|| compare_revision(left_rest, right_rest))
    }
}

/// Split `[epoch:]rest`, defaulting the epoch to zero.
fn split_epoch(version: &str) -> (u64, &str) {
    match version.split_once(':') {
        Some((epoch, rest)) => (epoch.parse().unwrap_or(0), rest),
        None => (0, version),
    }
}

/// Split off a `-debian_revision` suffix, if any.
fn split_revision(rest: &str) -> (&str, Option<&str>) {
    match rest.rsplit_once('-') {
        Some((upstream, revision)) => (upstream, Some(revision)),
        None => (rest, None),
    }
}

fn compare_revision(left: &str, right: &str) -> Ordering {
    let (left, right) = (split_revision(left).1, split_revision(right).1);
    NumericSegments.compare(left.unwrap_or("0"), right.unwrap_or("0"))
}

/// The Debian upstream comparison: digit runs compare numerically, other
/// characters by rank, with `~` ranked below the end of the string and the
/// end of the string below every other character. Both walk their strings
/// in lockstep until one decides.
fn compare_upstream(left: &str, right: &str) -> Ordering {
    let (mut left, mut right) = (left, right);
    loop {
        let (left_digit, right_digit) = (
            left.starts_with(|c: char| c.is_ascii_digit()),
            right.starts_with(|c: char| c.is_ascii_digit()),
        );
        if left_digit && right_digit {
            let (left_run, left_rest) = digit_run(left);
            let (right_run, right_rest) = digit_run(right);
            let ordering = numeric_run(left_run).cmp(&numeric_run(right_run));
            if ordering != Ordering::Equal {
                return ordering;
            }
            left = left_rest;
            right = right_rest;
            continue;
        }
        let (left_rank, left_step) = next_rank(left);
        let (right_rank, right_step) = next_rank(right);
        if left_rank != right_rank {
            return left_rank.cmp(&right_rank);
        }
        if left_step == 0 {
            return Ordering::Equal;
        }
        left = &left[left_step..];
        right = &right[right_step..];
    }
}

/// A maximal run of leading digits and the rest after it.
fn digit_run(text: &str) -> (&str, &str) {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    text.split_at(end)
}

/// A digit run by value: leading zeros dropped, then longer-is-larger, so
/// `01` equals `1` and `10` beats `9`.
fn numeric_run(run: &str) -> (usize, Vec<u8>) {
    let trimmed = run.trim_start_matches('0');
    (trimmed.len(), trimmed.as_bytes().to_vec())
}

/// The rank of the next character, with `~` lowest and the end of the
/// string just above it — the two facts that make `1.0~rc1 < 1.0 < 1.0+a`.
/// Returns the rank and how many bytes to consume (zero at the end).
fn next_rank(text: &str) -> (i32, usize) {
    match text.as_bytes().first() {
        None => (0, 0),
        Some(b'~') => (-1, 1),
        Some(byte) => (
            i32::from(*byte),
            text.chars().next().map_or(1, char::len_utf8),
        ),
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

    #[test]
    fn debian_tilde_sorts_before_the_untilded_same_version() {
        let scheme = Debian;
        assert_eq!(scheme.compare("1.0~rc1", "1.0"), Ordering::Less);
        assert_eq!(scheme.compare("1.0", "1.0+dfsg"), Ordering::Less);
        assert_eq!(scheme.compare("1.0~rc1", "1.0~rc2"), Ordering::Less);
    }

    #[test]
    fn debian_epoch_dominates_and_revision_compares_numerically() {
        let scheme = Debian;
        assert_eq!(scheme.compare("1:0.1", "0.9"), Ordering::Greater);
        assert_eq!(scheme.compare("1.0-2", "1.0-10"), Ordering::Less);
        assert_eq!(scheme.compare("1.0-2", "1.0"), Ordering::Greater);
    }

    #[test]
    fn debian_digit_runs_compare_numerically_not_lexicographically() {
        let scheme = Debian;
        assert_eq!(scheme.compare("0.9", "0.10"), Ordering::Less);
        assert_eq!(scheme.compare("1.01", "1.1"), Ordering::Equal);
    }
}
