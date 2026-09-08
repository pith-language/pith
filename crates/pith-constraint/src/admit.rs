//! Admission as a conjunction of claims: the first clause that fails is
//! the refusal. Each domain declares its own clause vocabulary, so a
//! checker written against one vocabulary cannot raise another's clause.

/// The refusal of an admission: the one clause that failed. The clause
/// carries both sides, so a diagnostic renders it without reconstruction.
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

/// Runs `claims` in order and returns the first refusal. Each item is an
/// already-decided claim: `Ok(())` when it holds, the clause when it does
/// not.
///
/// # Errors
/// [`Refusal`] wrapping the first failing claim.
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
