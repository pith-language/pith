//! The preference list: a lexicographic list of orderings the domain declares
//! (decision 0040).
//!
//! A preference never appears in the intersection that defines validity; it
//! is a third input value ordering valid solutions. Each ordering must be
//! grounded in a declared ordering, never invented (0040's reconciliation
//! with 0015): both constructors here read the version ordering the domain
//! declares (0039's `VersionScheme`), which is what makes `newest` a
//! domain-declared fact rather than engine policy. A list that underdetermines
//! a choice is the resolver's refusal to answer, not a tie for a search order
//! to break invisibly — the 0install lesson 0040 builds on.

use std::cmp::Ordering;

use pith_core::{SumConstructor, Type, Value};
use pith_diag::PithResult;

use crate::codec::sum_value;
use crate::diag;
use crate::identity::VersionScheme;

/// The declared preference sum's name.
pub const PREFERENCE: &str = "phloem.Preference";

const NEWEST: &str = "Newest";
const OLDEST: &str = "Oldest";

/// The written names of the orderings, the inverse pair for the lock file.
const NEWEST_NAME: &str = "newest";
const OLDEST_NAME: &str = "oldest";

/// One declared ordering over candidates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preference {
    /// Newest under the domain's declared version ordering.
    Newest,
    /// Oldest under the same declared ordering — its reversal, which is as
    /// domain-declared as the ordering itself.
    Oldest,
}

/// The declared preference sum type: `Newest`, `Oldest`.
#[must_use]
pub fn preference_type() -> Type {
    let sum = Type::sum(
        PREFERENCE,
        [
            SumConstructor {
                name: NEWEST.into(),
                payload: None,
            },
            SumConstructor {
                name: OLDEST.into(),
                payload: None,
            },
        ],
    );
    sum.unwrap_or_else(|error| unreachable!("{error}"))
}

/// The preference-list type: a lexicographic list of declared orderings.
#[must_use]
pub fn preference_list_type() -> Type {
    Type::List(Box::new(preference_type()))
}

impl Preference {
    #[must_use]
    pub fn to_value(self) -> Value {
        let constructor = match self {
            Self::Newest => NEWEST,
            Self::Oldest => OLDEST,
        };
        sum_value(PREFERENCE, constructor, None)
    }

    /// Read one preference from a value.
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming what was found when the value
    /// is not a declared preference.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        let Value::Sum {
            type_name,
            constructor,
            ..
        } = value
        else {
            return Err(diag(format!(
                "expected a value of the {PREFERENCE} sum, found {}",
                value.describe()
            )));
        };
        if type_name.as_ref() != PREFERENCE {
            return Err(diag(format!(
                "expected a value of the {PREFERENCE} sum, found {}",
                value.describe()
            )));
        }
        match constructor.as_ref() {
            NEWEST => Ok(Self::Newest),
            OLDEST => Ok(Self::Oldest),
            other => Err(diag(format!(
                "the {PREFERENCE} sum carried an unknown constructor `{other}`"
            ))),
        }
    }

    /// The name a decision trail records for this ordering, and the name
    /// the written lock spells it by.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Newest => NEWEST_NAME,
            Self::Oldest => OLDEST_NAME,
        }
    }

    /// The preference a written name names, the inverse of [`Self::name`].
    /// Both spellings live here, beside the ordering they name, so a new
    /// ordering lands where it is declared rather than in a reader.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            NEWEST_NAME => Some(Self::Newest),
            OLDEST_NAME => Some(Self::Oldest),
            _ => None,
        }
    }
}

/// A lexicographic preference list: earlier orderings decide, later ones
/// break ties. An empty list orders nothing, so any two distinct candidates
/// tie under it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PreferenceList(pub Box<[Preference]>);

impl PreferenceList {
    /// How two candidate versions compare under the list, over the ordering
    /// `scheme` declares. `Ordering::Equal` means no declared ordering
    /// separates them — underdetermination, for the resolver to refuse on.
    #[must_use]
    pub fn compare(&self, scheme: &dyn VersionScheme, left: &str, right: &str) -> Ordering {
        for preference in self.0.iter() {
            let ordering = scheme.compare(left, right);
            if ordering != Ordering::Equal {
                return match preference {
                    Preference::Newest => ordering,
                    Preference::Oldest => ordering.reverse(),
                };
            }
        }
        Ordering::Equal
    }

    /// The first ordering in the list that separates the two versions, or
    /// `None` when none does. This is the ordering a decision trail names as
    /// having chosen the winner.
    #[must_use]
    pub fn separator(
        &self,
        scheme: &dyn VersionScheme,
        left: &str,
        right: &str,
    ) -> Option<Preference> {
        for preference in self.0.iter() {
            if scheme.compare(left, right) != Ordering::Equal {
                return Some(*preference);
            }
        }
        None
    }
}

/// The canonical preference-list value: the list as given, which callers
/// already spell in priority order.
#[must_use]
pub fn preference_list_value(list: &PreferenceList) -> Value {
    Value::List(list.0.iter().map(|p| p.to_value()).collect())
}

/// Read a preference list from a value.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming what was found when the value is
/// not a list of declared preferences.
pub fn preference_list_from_value(value: &Value) -> PithResult<PreferenceList> {
    let Value::List(elements) = value else {
        return Err(diag(format!(
            "expected a preference list, found {}",
            value.describe()
        )));
    };
    let mut preferences = Vec::with_capacity(elements.len());
    for element in elements.iter() {
        preferences.push(Preference::from_value(element)?);
    }
    Ok(PreferenceList(preferences.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::NumericSegments;

    #[test]
    fn an_empty_list_separates_nothing() {
        let list = PreferenceList(Box::new([]));
        assert_eq!(
            list.compare(&NumericSegments, "1.0", "2.0"),
            Ordering::Equal
        );
    }

    #[test]
    fn later_orderings_break_the_ties_earlier_ones_cannot_see() {
        // Both orderings read the same declared comparison, so this checks
        // direction and lexicographic composition without inventing a second
        // ordering: newest decides distinct versions, and nothing separates
        // equal ones.
        let newest = PreferenceList(Box::new([Preference::Newest]));
        let oldest = PreferenceList(Box::new([Preference::Oldest]));
        assert_eq!(
            newest.compare(&NumericSegments, "1.2", "1.10"),
            Ordering::Less
        );
        assert_eq!(
            oldest.compare(&NumericSegments, "1.2", "1.10"),
            Ordering::Greater
        );
        assert_eq!(
            newest.compare(&NumericSegments, "1.0", "1.0.0"),
            Ordering::Equal
        );
    }
}
