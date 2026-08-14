//! Constraint values: ranges over the domain's ordering, and constraints over
//! package coordinates (decision 0040).
//!
//! A version range is a closed set of constructors — any, exactly, at least,
//! at most, between — defined against the ordering the domain declares
//! (0039's `VersionScheme`), never against a version spelling. PEP 440 and
//! semver bundle spelling, ordering, and range semantics into one grammar;
//! 0039 split the first two out and this module splits the third, which is
//! why the same algebra runs over a semver-shaped domain and a Debian-shaped
//! one without change.
//!
//! Bounds carry an inclusivity bit because negation must stay closed over
//! the five constructors: the complement of `>= 1.0` is `< 1.0`, an open
//! upper edge, and a constructor set of inclusive edges only could not say
//! it. A domain that wants inclusive edges simply constructs them; the bit
//! is what keeps the algebra total.
//!
//! Coordinates carry features beside the version (0040: features are
//! coordinates), so a constraint over a feature is a constraint over
//! coordinates and speaks before any realization exists.

use std::cmp::Ordering;

use pith_core::{SumConstructor, Type, Value};
use pith_diag::PithResult;

use crate::codec::{field_of, record_type, record_value, sum_value, text_list, text_of};
use crate::diag;
use crate::identity::{DomainIdentity, PackageIdentity, VersionScheme};

/// The declared range sum's name.
pub const RANGE: &str = "phloem.Range";

const ANY: &str = "Any";
const EXACTLY: &str = "Exactly";
const AT_LEAST: &str = "AtLeast";
const AT_MOST: &str = "AtMost";
const BETWEEN: &str = "Between";

const VERSION: &str = "version";
const INCLUSIVE: &str = "inclusive";
const LOWER: &str = "lower";
const UPPER: &str = "upper";

pub(crate) const DOMAIN: &str = "domain";
pub(crate) const PACKAGE: &str = "package";
pub(crate) const RANGE_FIELD: &str = "range";
pub(crate) const FEATURES: &str = "features";
pub(crate) const ATTRIBUTION: &str = "attribution";

/// One edge of a range: a version spelling plus whether the edge itself is
/// inside the range. The spelling orders only under the domain's scheme;
/// nothing here parses it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bound {
    pub version: Box<str>,
    pub inclusive: bool,
}

impl Bound {
    #[must_use]
    pub fn new(version: impl Into<Box<str>>, inclusive: bool) -> Self {
        Self {
            version: version.into(),
            inclusive,
        }
    }

    fn to_value(&self) -> Value {
        record_value([
            (VERSION, Value::Text(self.version.clone())),
            (INCLUSIVE, Value::Bool(self.inclusive)),
        ])
    }

    fn from_value(value: &Value) -> PithResult<Self> {
        let Value::Record(fields) = value else {
            return Err(bound_error(value));
        };
        let version = field_of(fields, VERSION)
            .map(|payload| text_of(payload, VERSION))
            .transpose()?;
        let inclusive = match field_of(fields, INCLUSIVE) {
            Some(Value::Bool(flag)) => Some(flag),
            Some(other) => return Err(bound_error(other)),
            None => None,
        };
        match (version, inclusive) {
            (Some(version), Some(inclusive)) => Ok(Self {
                version,
                inclusive: *inclusive,
            }),
            _ => Err(bound_error(value)),
        }
    }
}

fn bound_error(found: &Value) -> pith_diag::DiagnosticSink {
    diag(format!(
        "expected a range bound record {{version, inclusive}}, found {}",
        found.describe()
    ))
}

fn bound_type() -> Type {
    record_type([(VERSION, Type::Text), (INCLUSIVE, Type::Bool)])
}

/// One version range: a closed constructor over the ordering the domain
/// declares (0040). Satisfaction, intersection, and negation are defined
/// against that ordering alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Range {
    Any,
    Exactly(Box<str>),
    AtLeast(Bound),
    AtMost(Bound),
    Between { lower: Bound, upper: Bound },
}

/// The declared range sum type: `Any`, `Exactly(Text)`,
/// `AtLeast({version, inclusive})`, `AtMost({version, inclusive})`,
/// `Between({lower, upper})`.
#[must_use]
pub fn range_type() -> Type {
    let sum = Type::sum(
        RANGE,
        [
            SumConstructor {
                name: ANY.into(),
                payload: None,
            },
            SumConstructor {
                name: EXACTLY.into(),
                payload: Some(Type::Text),
            },
            SumConstructor {
                name: AT_LEAST.into(),
                payload: Some(bound_type()),
            },
            SumConstructor {
                name: AT_MOST.into(),
                payload: Some(bound_type()),
            },
            SumConstructor {
                name: BETWEEN.into(),
                payload: Some(record_type([(LOWER, bound_type()), (UPPER, bound_type())])),
            },
        ],
    );
    sum.unwrap_or_else(|error| unreachable!("{error}"))
}

impl Range {
    #[must_use]
    pub fn to_value(&self) -> Value {
        match self {
            Self::Any => sum_value(RANGE, ANY, None),
            Self::Exactly(version) => sum_value(RANGE, EXACTLY, Some(Value::Text(version.clone()))),
            Self::AtLeast(bound) => sum_value(RANGE, AT_LEAST, Some(bound.to_value())),
            Self::AtMost(bound) => sum_value(RANGE, AT_MOST, Some(bound.to_value())),
            Self::Between { lower, upper } => sum_value(
                RANGE,
                BETWEEN,
                Some(record_value([
                    (LOWER, lower.to_value()),
                    (UPPER, upper.to_value()),
                ])),
            ),
        }
    }

    /// Read a range from a value, checking inhabitation with `is_type` rather
    /// than `value_type`: a sum value cannot recover its sibling constructors
    /// (0026's asymmetry), so the declared sum is the check and the selected
    /// constructor is the read.
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming what was found when the value is
    /// not a range of the declared sum.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        if !value.is_type(&range_type()) {
            return Err(diag(format!(
                "expected a value of the {RANGE} sum, found {}",
                value.describe()
            )));
        }
        let Value::Sum {
            constructor,
            payload,
            ..
        } = value
        else {
            return Err(diag(format!(
                "expected a value of the {RANGE} sum, found {}",
                value.describe()
            )));
        };
        match constructor.as_ref() {
            ANY => Ok(Self::Any),
            EXACTLY => {
                let Some(payload) = payload.as_deref() else {
                    return Err(diag(format!(
                        "the {EXACTLY} constructor carried no version"
                    )));
                };
                Ok(Self::Exactly(text_of(payload, VERSION)?))
            }
            AT_LEAST => Ok(Self::AtLeast(bound_of(payload.as_deref(), AT_LEAST)?)),
            AT_MOST => Ok(Self::AtMost(bound_of(payload.as_deref(), AT_MOST)?)),
            BETWEEN => {
                let Some(payload) = payload.as_deref() else {
                    return Err(diag(format!("the {BETWEEN} constructor carried no edges")));
                };
                let Value::Record(fields) = payload else {
                    return Err(diag(format!(
                        "the {BETWEEN} constructor carried {} rather than an edge record",
                        payload.describe()
                    )));
                };
                let lower = field_of(fields, LOWER).map(Bound::from_value).transpose()?;
                let upper = field_of(fields, UPPER).map(Bound::from_value).transpose()?;
                match (lower, upper) {
                    (Some(lower), Some(upper)) => Ok(Self::Between { lower, upper }),
                    _ => Err(diag(format!(
                        "the {BETWEEN} constructor carried an edge record missing an edge"
                    ))),
                }
            }
            other => Err(diag(format!(
                "the {RANGE} sum carried an unknown constructor `{other}`"
            ))),
        }
    }

    /// Whether `version` is inside the range, under the ordering `scheme`
    /// declares. The spelling participates only as something the scheme
    /// compares.
    #[must_use]
    pub fn satisfies(&self, scheme: &dyn VersionScheme, version: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Exactly(exact) => scheme.compare(version, exact) == Ordering::Equal,
            Self::AtLeast(bound) => above(scheme, version, bound),
            Self::AtMost(bound) => below(scheme, version, bound),
            Self::Between { lower, upper } => {
                above(scheme, version, lower) && below(scheme, version, upper)
            }
        }
    }

    /// The intersection of two ranges: the set of versions both admit, when
    /// that set is nonempty. `None` is the empty set — no constructor names
    /// it, because a range is a set someone chose to speak about and the
    /// empty intersection is the discovery that nobody did.
    #[must_use]
    pub fn intersect(&self, scheme: &dyn VersionScheme, other: &Self) -> Option<Self> {
        let (lower, upper) = (
            tighter_lower(scheme, self.interval().0, other.interval().0),
            tighter_upper(scheme, self.interval().1, other.interval().1),
        );
        let (lower, upper) = (lower?, upper?);
        if interval_is_empty(scheme, &lower, &upper) {
            return None;
        }
        Some(interval_to_range(lower, upper))
    }

    /// The complement, as the union of at most two ranges: the complement of
    /// a bounded interval is everything below its lower edge plus everything
    /// above its upper edge, each edge flipping its inclusivity. Empty when
    /// the range is `Any`.
    #[must_use]
    pub fn negate(&self) -> Box<[Self]> {
        let (lower, upper) = self.interval();
        let mut parts = Vec::with_capacity(2);
        if let Some(lower) = lower {
            parts.push(Self::AtMost(Bound {
                version: lower.version,
                inclusive: !lower.inclusive,
            }));
        }
        if let Some(upper) = upper {
            parts.push(Self::AtLeast(Bound {
                version: upper.version,
                inclusive: !upper.inclusive,
            }));
        }
        parts.into()
    }

    /// The range as interval edges. `Exactly(v)` is the closed interval at
    /// `v`; the constructor is kept because it is the spelling a pin wants.
    fn interval(&self) -> (Option<Bound>, Option<Bound>) {
        match self {
            Self::Any => (None, None),
            Self::Exactly(version) => (
                Some(Bound {
                    version: version.clone(),
                    inclusive: true,
                }),
                Some(Bound {
                    version: version.clone(),
                    inclusive: true,
                }),
            ),
            Self::AtLeast(bound) => (Some(bound.clone()), None),
            Self::AtMost(bound) => (None, Some(bound.clone())),
            Self::Between { lower, upper } => (Some(lower.clone()), Some(upper.clone())),
        }
    }
}

fn bound_of(payload: Option<&Value>, constructor: &str) -> PithResult<Bound> {
    let Some(payload) = payload else {
        return Err(diag(format!(
            "the {constructor} constructor carried no bound"
        )));
    };
    Bound::from_value(payload)
}

/// Whether `version` sits at or above `bound` under the scheme.
fn above(scheme: &dyn VersionScheme, version: &str, bound: &Bound) -> bool {
    match scheme.compare(version, &bound.version) {
        Ordering::Greater => true,
        Ordering::Equal => bound.inclusive,
        Ordering::Less => false,
    }
}

/// Whether `version` sits at or below `bound` under the scheme.
fn below(scheme: &dyn VersionScheme, version: &str, bound: &Bound) -> bool {
    match scheme.compare(version, &bound.version) {
        Ordering::Less => true,
        Ordering::Equal => bound.inclusive,
        Ordering::Greater => false,
    }
}

/// The tighter of two lower edges: the greater version, and the exclusive
/// edge when the versions tie.
fn tighter_lower(
    scheme: &dyn VersionScheme,
    left: Option<Bound>,
    right: Option<Bound>,
) -> Option<Bound> {
    combine_edges(scheme, left, right, Ordering::Greater)
}

/// The tighter of two upper edges: the lesser version, and the exclusive
/// edge when the versions tie.
fn tighter_upper(
    scheme: &dyn VersionScheme,
    left: Option<Bound>,
    right: Option<Bound>,
) -> Option<Bound> {
    combine_edges(scheme, left, right, Ordering::Less)
}

fn combine_edges(
    scheme: &dyn VersionScheme,
    left: Option<Bound>,
    right: Option<Bound>,
    tighter: Ordering,
) -> Option<Bound> {
    match (left, right) {
        (None, other) | (other, None) => other,
        (Some(left), Some(right)) => Some(match scheme.compare(&left.version, &right.version) {
            ordering if ordering == tighter => left,
            Ordering::Equal => {
                if left.inclusive {
                    right
                } else {
                    left
                }
            }
            _ => right,
        }),
    }
}

/// Whether an interval with these edges holds no version: the lower edge
/// passes the upper edge, or the two name one version that at least one edge
/// excludes.
fn interval_is_empty(scheme: &dyn VersionScheme, lower: &Bound, upper: &Bound) -> bool {
    match scheme.compare(&lower.version, &upper.version) {
        Ordering::Greater => true,
        Ordering::Equal => !lower.inclusive || !upper.inclusive,
        Ordering::Less => false,
    }
}

fn interval_to_range(lower: Bound, upper: Bound) -> Range {
    Range::Between { lower, upper }
}

/// One hard constraint: a range and a feature set over one package's
/// coordinates, attributed to whoever declared it. The attribution is what a
/// failure derivation names; an unattributed constraint is unspeakable in an
/// explanation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constraint {
    pub subject: PackageIdentity,
    pub range: Range,
    /// The features the constraint requires. Features are coordinates (0040),
    /// so this is part of what the constraint ranges over, not a side
    /// condition on realization.
    pub features: Box<[Box<str>]>,
    pub attribution: Box<str>,
}

/// The constraint record type: `{domain, package, range, features,
/// attribution}`.
#[must_use]
pub fn constraint_type() -> Type {
    record_type([
        (DOMAIN, Type::Text),
        (PACKAGE, Type::Text),
        (RANGE_FIELD, range_type()),
        (FEATURES, Type::List(Box::new(Type::Text))),
        (ATTRIBUTION, Type::Text),
    ])
}

impl Constraint {
    #[must_use]
    pub fn to_value(&self) -> Value {
        record_value([
            (DOMAIN, Value::Text(self.subject.domain().as_str().into())),
            (PACKAGE, Value::Text(self.subject.name().into())),
            (RANGE_FIELD, self.range.to_value()),
            (
                FEATURES,
                Value::List(
                    self.features
                        .iter()
                        .map(|feature| Value::Text(feature.clone()))
                        .collect(),
                ),
            ),
            (ATTRIBUTION, Value::Text(self.attribution.clone())),
        ])
    }

    /// Read a constraint from a value, on the `is_type` terms every declared
    /// shape here uses (0026's asymmetry).
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming what was found when the value is
    /// not a constraint record.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        if !value.is_type(&constraint_type()) {
            return Err(diag(format!(
                "expected a value of the constraint record type, found {}",
                value.describe()
            )));
        }
        let Value::Record(fields) = value else {
            return Err(diag(format!(
                "expected a value of the constraint record type, found {}",
                value.describe()
            )));
        };
        let mut domain = None;
        let mut package = None;
        let mut range = None;
        let mut features = Vec::new();
        let mut attribution = None;
        for field in fields.iter() {
            match field.name.as_ref() {
                DOMAIN => domain = Some(text_of(&field.payload, DOMAIN)?),
                PACKAGE => package = Some(text_of(&field.payload, PACKAGE)?),
                RANGE_FIELD => range = Some(Range::from_value(&field.payload)?),
                FEATURES => features = text_list(&field.payload, FEATURES)?,
                ATTRIBUTION => attribution = Some(text_of(&field.payload, ATTRIBUTION)?),
                _ => {}
            }
        }
        let (Some(domain), Some(package), Some(range), Some(attribution)) =
            (domain, package, range, attribution)
        else {
            return Err(diag(
                "the constraint record was missing a domain, package, range, or attribution field",
            ));
        };
        Ok(Self {
            subject: PackageIdentity::declare(DomainIdentity::new(domain), package),
            range,
            features: features.into(),
            attribution,
        })
    }

    /// Whether a candidate's coordinates satisfy this constraint: the
    /// version inside the range and every required feature among the
    /// coordinates' features. Both checks are over declared coordinates;
    /// nothing here knows a realization.
    #[must_use]
    pub fn admits(&self, scheme: &dyn VersionScheme, version: &str, features: &[Box<str>]) -> bool {
        self.range.satisfies(scheme, version)
            && self
                .features
                .iter()
                .all(|required| features.iter().any(|feature| feature == required))
    }
}

/// A constraint set as a value: a canonically sorted list, so the same set
/// has one spelling and one computation key (0040's keyed lookups as sorted
/// lists).
#[must_use]
pub fn constraint_set_type() -> Type {
    Type::List(Box::new(constraint_type()))
}

/// Canonicalize a constraint list into value order: sorted, without
/// duplicates. Construction order is caller policy and must not reach the
/// computation key.
#[must_use]
pub fn constraint_set_value(constraints: &[Constraint]) -> Value {
    let mut entries: Vec<(Vec<u8>, Value)> = constraints
        .iter()
        .map(|c| (c.to_value().encode_canonical(), c.to_value()))
        .collect();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries.dedup_by(|front, back| front.0 == back.0);
    Value::List(entries.into_iter().map(|(_, value)| value).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{DomainIdentity, NumericSegments};

    fn identity(name: &str) -> PackageIdentity {
        PackageIdentity::declare(DomainIdentity::new("pithpkgs"), name)
    }

    #[test]
    fn satisfaction_reads_the_ordering_not_the_spelling() {
        let scheme = NumericSegments;
        let at_least = Range::AtLeast(Bound::new("1.2", true));
        assert!(at_least.satisfies(&scheme, "1.10"));
        assert!(at_least.satisfies(&scheme, "1.2"));
        assert!(!at_least.satisfies(&scheme, "1.1"));
        let between = Range::Between {
            lower: Bound::new("1.2", true),
            upper: Bound::new("2.0", false),
        };
        assert!(between.satisfies(&scheme, "1.9"));
        assert!(!between.satisfies(&scheme, "2.0"));
        assert!(Range::Exactly("1.2".into()).satisfies(&scheme, "1.2.0"));
        assert!(Range::Any.satisfies(&scheme, "0.0.1"));
    }

    #[test]
    fn intersection_is_the_shared_set_and_empty_is_none() {
        let scheme = NumericSegments;
        let at_least = Range::AtLeast(Bound::new("1.2", true));
        let at_most = Range::AtMost(Bound::new("2.0", true));
        let shared = at_least.intersect(&scheme, &at_most).unwrap();
        assert!(shared.satisfies(&scheme, "1.5"));
        assert!(!shared.satisfies(&scheme, "1.1"));
        assert!(!shared.satisfies(&scheme, "2.1"));

        let conflicting = Range::AtLeast(Bound::new("2.1", true));
        assert_eq!(at_most.intersect(&scheme, &conflicting), None);

        // An exclusive edge at a tied version excludes the one version both
        // name, so the intersection is empty rather than a point.
        let exclusive = Range::AtMost(Bound::new("1.2", false));
        assert_eq!(at_least.intersect(&scheme, &exclusive), None);
    }

    #[test]
    fn negation_is_the_complement_over_the_ordering() {
        let scheme = NumericSegments;
        let versions = ["1.0", "1.2", "1.5", "2.0", "2.3"];
        let ranges = [
            Range::Any,
            Range::Exactly("1.2".into()),
            Range::AtLeast(Bound::new("1.2", true)),
            Range::AtMost(Bound::new("1.5", false)),
            Range::Between {
                lower: Bound::new("1.2", true),
                upper: Bound::new("2.0", true),
            },
        ];
        for range in &ranges {
            let negated = range.negate();
            for version in versions {
                let admitted: bool = negated.iter().any(|part| part.satisfies(&scheme, version));
                let membership = range.satisfies(&scheme, version);
                assert_eq!(
                    admitted, !membership,
                    "the negation of {range:?} misclassified {version}"
                );
            }
        }
    }

    #[test]
    fn a_constraint_round_trips_through_its_value() {
        let constraint = Constraint {
            subject: identity("zlib"),
            range: Range::AtLeast(Bound::new("1.3", true)),
            features: Box::new(["shared".into()]),
            attribution: "root".into(),
        };
        let value = constraint.to_value();
        assert!(value.is_type(&constraint_type()));
        assert_eq!(Constraint::from_value(&value).unwrap(), constraint);
    }

    #[test]
    fn a_constraint_over_a_feature_selects_among_equal_versions() {
        // Features are coordinates (0040): a constraint requiring `shared`
        // rejects a candidate at the same version without it, which is the
        // feature-selection subject M-4 names.
        let scheme = NumericSegments;
        let constraint = Constraint {
            subject: identity("openssl"),
            range: Range::Exactly("1.1.1".into()),
            features: Box::new(["shared".into()]),
            attribution: "root".into(),
        };
        assert!(constraint.admits(&scheme, "1.1.1", &["shared".into()]));
        assert!(!constraint.admits(&scheme, "1.1.1", &[]));
        assert!(!constraint.admits(&scheme, "1.1.0", &["shared".into()]));
    }

    #[test]
    fn a_constraint_set_has_one_spelling_regardless_of_construction_order() {
        let first = Constraint {
            subject: identity("zlib"),
            range: Range::Any,
            features: Box::new([]),
            attribution: "root".into(),
        };
        let second = Constraint {
            subject: identity("openssl"),
            range: Range::Any,
            features: Box::new([]),
            attribution: "root".into(),
        };
        let one_way = constraint_set_value(&[first.clone(), second.clone()]);
        let other_way = constraint_set_value(&[second, first]);
        assert_eq!(one_way, other_way);
    }
}
