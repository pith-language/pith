//! Admission as a conjunction of claims: the first clause that fails is
//! the refusal, and every clause names both of its sides.
//!
//! The machinery is one run and one envelope. A domain declares its clause
//! vocabulary — one variant per claim an admission can refuse with — and
//! the vocabulary is the whole separation: a checker written against one
//! vocabulary has no way to raise another's clause, because no other
//! clause type fits the refusal it returns.

/// The refusal of an admission: the one clause that failed.
///
/// `Clause` is the domain's vocabulary, one variant per claim, so this is
/// one refusal type with a clause per claim rather than a boolean any
/// check could have set. A refusal never summarizes: the clause carries
/// both the side that was bound or requested and the side that was
/// offered, so a diagnostic renders it without reconstructing either.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal<Clause> {
    pub clause: Clause,
}

impl<Clause: std::fmt::Display> std::fmt::Display for Refusal<Clause> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.clause.fmt(formatter)
    }
}

impl<Clause: std::fmt::Display + std::fmt::Debug> std::error::Error for Refusal<Clause> {}

/// Runs `claims` in order and returns the first refusal.
///
/// Each item is one claim already decided — `Ok(())` when the offered side
/// satisfied the bound side, the claim's clause when it did not — so the
/// order of the items is the order of the clauses, and the first failure is
/// the answer rather than the most recent one.
///
/// # Errors
/// Returns the first failing claim, wrapped in the refusal that names it.
pub fn admit<Clause>(
    claims: impl IntoIterator<Item = Result<(), Clause>>,
) -> Result<(), Refusal<Clause>> {
    for claim in claims {
        claim.map_err(|clause| Refusal { clause })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Clause {
        First,
        Second,
    }

    impl std::fmt::Display for Clause {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(match self {
                Self::First => "the first claim failed",
                Self::Second => "the second claim failed",
            })
        }
    }

    #[test]
    fn every_claim_holding_admits() {
        let claims: [Result<(), Clause>; 2] = [Ok(()), Ok(())];
        assert_eq!(admit(claims), Ok(()));
    }

    #[test]
    fn the_first_failure_is_the_refusal() {
        let claims: [Result<(), Clause>; 3] = [Ok(()), Err(Clause::First), Err(Clause::Second)];
        let refused = admit(claims).expect_err("the second claim failed");
        assert_eq!(refused.clause, Clause::First);
    }

    #[test]
    fn a_refusal_renders_its_clause() {
        let refused = Refusal {
            clause: Clause::Second,
        };
        assert_eq!(refused.to_string(), "the second claim failed");
    }
}
