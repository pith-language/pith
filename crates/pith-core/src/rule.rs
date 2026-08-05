//! Rules, interfaces, and rule selection.
//!
//! Decision 0015 fixes the selection contract: a request matches at most one
//! rule; more than one match is an error naming every candidate, never a
//! silent ranking. [`SelectOutcome`] encodes that: it has an `Ambiguous`
//! variant that carries the candidate set, so silent ranking cannot be
//! implemented without changing this type.

use pith_arena::define_arena;
use pith_diag::{Diag, Severity, Span, StableCode};
use smallvec::SmallVec;

define_arena!(RuleId, RuleArena, RuleBrand);

/// A request for a typed result. Matched against [`Interface`] (decision 0015).
#[derive(Clone, Debug)]
pub struct Request {
    pub label: Box<str>,
    pub span: Span,
}

/// The typed interface a rule declares. For M-1 this is a label plus the
/// output kind; the prototype adds required capabilities, input constraints,
/// target platform, and domain-noun identity.
#[derive(Clone, Debug)]
pub struct Interface {
    pub label: Box<str>,
    pub output_kind: Box<str>,
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub interface: Interface,
    pub span: Span,
}

/// The outcome of matching a request against available rules. Three variants,
/// no ranking: zero matches, one match, or an ambiguity carrying every
/// candidate (decision 0015).
#[derive(Clone, Debug)]
pub enum SelectOutcome {
    NoMatch,
    One(RuleId),
    Ambiguous(SmallVec<[RuleId; 2]>),
}

impl SelectOutcome {
    /// # Errors
    /// `E-1101` when no rule matched; `E-1102` naming every candidate when more
    /// than one matched.
    pub fn into_result(self, request: &Request, rules: &RuleArena<Rule>) -> Result<RuleId, Diag> {
        match self {
            SelectOutcome::One(id) => Ok(id),
            SelectOutcome::NoMatch => Err(Diag::new(
                Severity::Error,
                StableCode::engine(101),
                request.span,
                format!("no rule satisfies `{}`", request.label),
            )),
            SelectOutcome::Ambiguous(candidates) => {
                let mut diag = Diag::new(
                    Severity::Error,
                    StableCode::engine(102),
                    request.span,
                    format!("ambiguous rule for `{}`", request.label),
                );
                for id in candidates {
                    if let Some(rule) = rules.get(id) {
                        diag = diag.with_note(
                            rule.span,
                            format!(
                                "candidate: {} -> {}",
                                rule.interface.label, rule.interface.output_kind
                            ),
                        );
                    }
                }
                Err(diag)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pith_arena::Arena;

    fn rule(label: &str) -> Rule {
        Rule {
            interface: Interface {
                label: label.into(),
                output_kind: "Artifact".into(),
            },
            span: Span::none(),
        }
    }

    fn request(label: &str) -> Request {
        Request {
            label: label.into(),
            span: Span::none(),
        }
    }

    #[test]
    fn no_match_produces_error_with_stable_code() {
        let arena: RuleArena<Rule> = Arena::new();
        let err = SelectOutcome::NoMatch
            .into_result(&request("x"), &arena)
            .unwrap_err();
        assert_eq!(err.code, StableCode::engine(101));
        assert_eq!(err.severity, Severity::Error);
    }

    #[test]
    fn ambiguous_names_every_candidate_in_notes() {
        let mut arena: RuleArena<Rule> = Arena::new();
        let r0 = arena.push(rule("a"));
        let r1 = arena.push(rule("b"));
        let err = SelectOutcome::Ambiguous(smallvec::smallvec![r0, r1])
            .into_result(&request("thing"), &arena)
            .unwrap_err();
        assert_eq!(err.code, StableCode::engine(102));
        assert_eq!(err.notes.len(), 2);
        assert!(err.notes.iter().all(|n| n.message.0.contains("candidate")));
    }
}
