//! Version ranges and the interval algebra over them.
//!
//! The five constructors are the range model the resolver protocol settled:
//! `Any`, `Exactly`, `AtLeast`, `AtMost`, `Between`. Membership,
//! intersection, and complement are closed over those constructors — every
//! operation returns ranges a manifest could have written — and every
//! answer is a function of the ordering the caller supplies, never of a
//! spelling.

use std::cmp::Ordering;

/// How a domain orders its versions.
///
/// The algebra never parses a version: it compares the caller's version
/// values under the ordering the caller declares, so one algebra serves
/// scheme-carried spellings and typed segment sequences alike.
pub trait Compare<Version: ?Sized> {
    /// How `left` orders against `right`.
    fn compare(&self, left: &Version, right: &Version) -> Ordering;
}

/// Orders versions by their own [`Ord`].
///
/// The comparator for version types whose ordering is total and derived —
/// canonical segment sequences, notably — where no scheme sits between a
/// range and its members.
#[derive(Clone, Copy, Debug, Default)]
pub struct ByOrdering;

impl<Version: Ord> Compare<Version> for ByOrdering {
    fn compare(&self, left: &Version, right: &Version) -> Ordering {
        left.cmp(right)
    }
}

/// One edge of a range: a version, and whether the edge itself is inside.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge<Version> {
    pub version: Version,
    pub inclusive: bool,
}

/// Which versions a clause admits, under the ordering it is evaluated
/// against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Range<Version> {
    Any,
    Exactly(Version),
    AtLeast(Edge<Version>),
    AtMost(Edge<Version>),
    Between {
        lower: Edge<Version>,
        upper: Edge<Version>,
    },
}

impl<Version: Clone> Range<Version> {
    /// Whether `version` is inside the range under `order`.
    #[must_use]
    pub fn contains<Order>(&self, order: &Order, version: &Version) -> bool
    where
        Order: Compare<Version> + ?Sized,
    {
        let (lower, upper) = self.edges();
        lower
            .as_ref()
            .is_none_or(|edge| above(order, version, edge))
            && upper
                .as_ref()
                .is_none_or(|edge| below(order, version, edge))
    }

    /// The nonempty intersection of two ranges under `order`: `None` when
    /// they share no version.
    #[must_use]
    pub fn intersect<Order>(&self, order: &Order, other: &Self) -> Option<Self>
    where
        Order: Compare<Version> + ?Sized,
    {
        let (lower, upper) = (
            tighter(order, self.edges().0, other.edges().0, Ordering::Greater),
            tighter(order, self.edges().1, other.edges().1, Ordering::Less),
        );
        let (lower, upper) = (lower?, upper?);
        if empty(order, &lower, &upper) {
            return None;
        }
        Some(Range::Between { lower, upper })
    }

    /// The complement, as the union of at most two ranges: everything
    /// below the lower edge plus everything above the upper edge, each
    /// edge flipping its inclusivity. Empty when the range is `Any`.
    #[must_use]
    pub fn negate(&self) -> Box<[Self]> {
        let (lower, upper) = self.edges();
        let mut parts = Vec::with_capacity(2);
        if let Some(lower) = lower {
            parts.push(Range::AtMost(Edge {
                version: lower.version,
                inclusive: !lower.inclusive,
            }));
        }
        if let Some(upper) = upper {
            parts.push(Range::AtLeast(Edge {
                version: upper.version,
                inclusive: !upper.inclusive,
            }));
        }
        parts.into()
    }

    /// The range as interval edges. `Exactly(v)` is the closed interval at
    /// `v`; the constructor is kept elsewhere because it is the spelling a
    /// pin wants.
    fn edges(&self) -> (Option<Edge<Version>>, Option<Edge<Version>>) {
        match self {
            Self::Any => (None, None),
            Self::Exactly(version) => {
                let lower = Edge {
                    version: version.clone(),
                    inclusive: true,
                };
                let upper = Edge {
                    version: version.clone(),
                    inclusive: true,
                };
                (Some(lower), Some(upper))
            }
            Self::AtLeast(edge) => (Some(edge.clone()), None),
            Self::AtMost(edge) => (None, Some(edge.clone())),
            Self::Between { lower, upper } => (Some(lower.clone()), Some(upper.clone())),
        }
    }
}

/// Whether `version` sits at or above `edge` under `order`.
fn above<Version, Order>(order: &Order, version: &Version, edge: &Edge<Version>) -> bool
where
    Order: Compare<Version> + ?Sized,
{
    match order.compare(version, &edge.version) {
        Ordering::Greater => true,
        Ordering::Equal => edge.inclusive,
        Ordering::Less => false,
    }
}

/// Whether `version` sits at or below `edge` under `order`.
fn below<Version, Order>(order: &Order, version: &Version, edge: &Edge<Version>) -> bool
where
    Order: Compare<Version> + ?Sized,
{
    match order.compare(version, &edge.version) {
        Ordering::Less => true,
        Ordering::Equal => edge.inclusive,
        Ordering::Greater => false,
    }
}

/// The tighter of two like edges: the version further along `tighter`, and
/// the exclusive edge when the two versions tie.
fn tighter<Version, Order>(
    order: &Order,
    left: Option<Edge<Version>>,
    right: Option<Edge<Version>>,
    tighter_ordering: Ordering,
) -> Option<Edge<Version>>
where
    Order: Compare<Version> + ?Sized,
{
    match (left, right) {
        (None, other) | (other, None) => other,
        (Some(left), Some(right)) => Some(match order.compare(&left.version, &right.version) {
            ordering if ordering == tighter_ordering => left,
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
/// passes the upper edge, or the two name one version that at least one
/// edge excludes.
fn empty<Version, Order>(order: &Order, lower: &Edge<Version>, upper: &Edge<Version>) -> bool
where
    Order: Compare<Version> + ?Sized,
{
    match order.compare(&lower.version, &upper.version) {
        Ordering::Greater => true,
        Ordering::Equal => !lower.inclusive || !upper.inclusive,
        Ordering::Less => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at_least(major: u64, inclusive: bool) -> Range<u64> {
        Range::AtLeast(Edge {
            version: major,
            inclusive,
        })
    }

    fn at_most(major: u64, inclusive: bool) -> Range<u64> {
        Range::AtMost(Edge {
            version: major,
            inclusive,
        })
    }

    #[test]
    fn membership_reads_the_ordering_not_the_constructor() {
        assert!(at_least(2, true).contains(&ByOrdering, &3));
        assert!(at_least(2, true).contains(&ByOrdering, &2));
        assert!(!at_least(2, false).contains(&ByOrdering, &2));
        assert!(Range::Exactly(4).contains(&ByOrdering, &4));
        assert!(!Range::Exactly(4).contains(&ByOrdering, &5));
        assert!(Range::<u64>::Any.contains(&ByOrdering, &0));
        let between = Range::Between {
            lower: Edge {
                version: 2,
                inclusive: true,
            },
            upper: Edge {
                version: 5,
                inclusive: false,
            },
        };
        assert!(between.contains(&ByOrdering, &2));
        assert!(between.contains(&ByOrdering, &4));
        assert!(!between.contains(&ByOrdering, &5));
    }

    #[test]
    fn intersection_is_the_shared_set_and_empty_is_none() {
        let shared = at_least(2, true)
            .intersect(&ByOrdering, &at_most(5, true))
            .unwrap();
        assert!(shared.contains(&ByOrdering, &3));
        assert!(!shared.contains(&ByOrdering, &1));
        assert!(!shared.contains(&ByOrdering, &6));
        assert_eq!(
            at_most(5, true).intersect(&ByOrdering, &at_least(6, true)),
            None
        );
        assert_eq!(
            at_least(2, true).intersect(&ByOrdering, &at_most(2, false)),
            None
        );
    }

    #[test]
    fn negation_is_the_complement_over_the_ordering() {
        let ranges = [
            Range::<u64>::Any,
            Range::Exactly(2),
            at_least(2, true),
            at_most(4, false),
            Range::Between {
                lower: Edge {
                    version: 2,
                    inclusive: true,
                },
                upper: Edge {
                    version: 4,
                    inclusive: true,
                },
            },
        ];
        for range in &ranges {
            let negated = range.negate();
            for version in 0..6 {
                let admitted = negated
                    .iter()
                    .any(|part| part.contains(&ByOrdering, &version));
                assert_eq!(
                    admitted,
                    !range.contains(&ByOrdering, &version),
                    "the negation of {range:?} misclassified {version}"
                );
            }
        }
    }
}
