//! Package identities and domain-specific version ordering.
//!
//! A package identity consists of a declared domain and package name.
//! Version schemes compare version strings without changing package identity.

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

/// A package name within a declared domain.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageIdentity {
    domain: DomainIdentity,
    name: Box<str>,
}

impl PackageIdentity {
    /// Creates a package identity from its domain and name.
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
}

/// A package identity and version string.
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

    /// Compares two versions of the same package with `scheme`.
    ///
    /// # Errors
    /// Returns a diagnostic when the package identities differ.
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

/// A domain-specific total ordering over version strings.
pub trait VersionScheme {
    /// Compares two version strings.
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

/// Creates a version-scheme value naming `scheme`.
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

/// Dot-separated numeric comparison with bytewise non-numeric segments.
#[derive(Clone, Copy, Debug, Default)]
pub struct NumericSegments;

impl VersionScheme for NumericSegments {
    fn compare(&self, left: &str, right: &str) -> Ordering {
        let mut left = left.split('.');
        let mut right = right.split('.');
        loop {
            let ordering = match (left.next(), right.next()) {
                (None, None) => return Ordering::Equal,
                // Missing segments compare as zero.
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

/// Returns the implicit segment used after one version runs out.
fn zero() -> Segment<'static> {
    Segment::Number(0, "0")
}

/// A numeric or bytewise version segment.
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

/// Debian-style comparison of `[epoch:]upstream[-revision]` versions.
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

/// Returns the next Debian character rank and its byte width.
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
