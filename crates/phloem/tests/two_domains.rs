//! The range algebra depends only on the ordering, not the spelling
//! (decision 0040's claim about splitting PEP 440's three-in-one grammar).
//!
//! Two domains declare different version schemes — a semver-shaped numeric
//! one and a Debian-shaped one with epochs and tilde ordering — and the same
//! algebra must hold over both. Every expectation below is derived from the
//! position a spelling occupies in the domain's own declared ordering, so
//! the test knows nothing about how either scheme spells or parses a
//! version: if the algebra leaked any spelling knowledge, one of the two
//! instances would fail.

use phloem::constraint::{Bound, Range};
use phloem::identity::{Debian, NumericSegments, VersionScheme};

/// `at least` admits exactly the suffix of the ordering, edge included: the
/// i-th spelling of `ordered` is the i-th version under the scheme's own
/// comparison, and every assertion speaks in those positions.
fn at_least_admits_exactly_the_suffix(scheme: &dyn VersionScheme, ordered: &[&str]) {
    for (lower_index, lower) in ordered.iter().enumerate() {
        let at_least = Range::AtLeast(Bound::new(*lower, true));
        for (candidate_index, candidate) in ordered.iter().enumerate() {
            assert_eq!(
                at_least.satisfies(scheme, candidate),
                candidate_index >= lower_index,
                "at least {lower} misclassified {candidate}",
            );
        }
    }
}

/// `exactly` admits one version, and its negation admits every other.
fn exactly_admits_one_version_and_negates_the_rest(scheme: &dyn VersionScheme, ordered: &[&str]) {
    for (lower_index, lower) in ordered.iter().enumerate() {
        let exactly = Range::Exactly((*lower).into());
        let negated = exactly.negate();
        for (candidate_index, candidate) in ordered.iter().enumerate() {
            assert_eq!(
                exactly.satisfies(scheme, candidate),
                candidate_index == lower_index
            );
            assert_eq!(
                negated.iter().any(|part| part.satisfies(scheme, candidate)),
                candidate_index != lower_index,
                "the negation of exactly {lower} misclassified {candidate}",
            );
        }
    }
}

/// `between` admits exactly the interval, and its negation is the complement:
/// everything the interval excludes, and nothing it includes.
fn between_admits_the_interval_and_negates_the_complement(
    scheme: &dyn VersionScheme,
    ordered: &[&str],
) {
    for (lower_index, lower) in ordered.iter().enumerate() {
        for (upper_index, upper) in ordered.iter().enumerate() {
            // A between with a crossed edge pair is not constructed by
            // honest callers; skip the empty spelling rather than define
            // what a crossed interval means.
            if lower_index > upper_index {
                continue;
            }
            let between = Range::Between {
                lower: Bound::new(*lower, true),
                upper: Bound::new(*upper, true),
            };
            for (candidate_index, candidate) in ordered.iter().enumerate() {
                let inside = candidate_index >= lower_index && candidate_index <= upper_index;
                assert_eq!(
                    between.satisfies(scheme, candidate),
                    inside,
                    "between {lower} and {upper} misclassified {candidate}",
                );
                assert_eq!(
                    between
                        .negate()
                        .iter()
                        .any(|part| part.satisfies(scheme, candidate)),
                    !inside
                );
            }
        }
    }
}

/// Intersection is the shared interval, and disjoint edges are the empty set.
fn intersection_is_the_shared_interval(scheme: &dyn VersionScheme, ordered: &[&str]) {
    for (lower_index, lower) in ordered.iter().enumerate() {
        for (upper_index, upper) in ordered.iter().enumerate() {
            let at_least = Range::AtLeast(Bound::new(*lower, true));
            let at_most = Range::AtMost(Bound::new(*upper, true));
            let shared = at_least.intersect(scheme, &at_most);
            if lower_index > upper_index {
                assert_eq!(shared, None, "edges {lower} and {upper} share nothing");
                continue;
            }
            let shared = shared.unwrap_or_else(|| {
                unreachable!("edges {lower} and {upper} overlap under the declared scheme")
            });
            for (candidate_index, candidate) in ordered.iter().enumerate() {
                assert_eq!(
                    shared.satisfies(scheme, candidate),
                    candidate_index >= lower_index && candidate_index <= upper_index,
                    "the intersection of at least {lower} and at most {upper} \
                     misclassified {candidate}",
                );
            }
        }
    }
}

#[test]
fn the_algebra_holds_over_a_semver_shaped_domain() {
    // Ascending under NumericSegments; the spellings are semver-shaped and
    // the ordering numeric, so "1.10" sits above "1.2".
    let scheme = &NumericSegments as &dyn VersionScheme;
    let ordered = &["0.9", "1.0", "1.2", "1.10", "2.0", "2.0.1"];
    at_least_admits_exactly_the_suffix(scheme, ordered);
    exactly_admits_one_version_and_negates_the_rest(scheme, ordered);
    between_admits_the_interval_and_negates_the_complement(scheme, ordered);
    intersection_is_the_shared_interval(scheme, ordered);
}

#[test]
fn the_algebra_holds_over_a_debian_shaped_domain() {
    // Ascending under Debian: tilde sorts below the untilded same version,
    // `+dfsg` above the bare version, and the epoch dominates everything —
    // the three facts that make this ordering a different animal from the
    // semver-shaped one above.
    let scheme = &Debian as &dyn VersionScheme;
    let ordered = &[
        "0.9~rc1", "0.9", "1.0~rc1", "1.0", "1.0+dfsg", "2.0", "1:0.1", "1:0.2",
    ];
    at_least_admits_exactly_the_suffix(scheme, ordered);
    exactly_admits_one_version_and_negates_the_rest(scheme, ordered);
    between_admits_the_interval_and_negates_the_complement(scheme, ordered);
    intersection_is_the_shared_interval(scheme, ordered);
}

#[test]
fn the_two_schemes_genuinely_disagree_about_spellings() {
    // The guard behind the two instances: if the schemes did not order the
    // same spellings differently, both instances would be the same test and
    // nothing about "depends only on the ordering" would be measured.
    use std::cmp::Ordering;
    let semver_shaped = NumericSegments.compare("1.0+dfsg", "1.0~rc1");
    let deb = Debian.compare("1.0+dfsg", "1.0~rc1");
    assert_ne!(semver_shaped, deb);
    assert_eq!(semver_shaped, Ordering::Less);
    assert_eq!(deb, Ordering::Greater);
}
