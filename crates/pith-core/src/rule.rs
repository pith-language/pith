//! Rules, typed interfaces, requests, and deterministic selection.

use pith_arena::define_arena;
use pith_diag::{Diag, Severity, Span, StableCode};
use smallvec::SmallVec;

use crate::Type;

define_arena!(RuleId, RuleArena, RuleBrand);

/// The typed signature a rule provides and a request requires.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Interface {
    pub inputs: Box<[Type]>,
    pub output: Type,
}

impl std::fmt::Display for Interface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("(")?;
        let mut separator = "";
        for input in &self.inputs {
            f.write_str(separator)?;
            write!(f, "{input}")?;
            separator = ", ";
        }
        write!(f, ") -> {}", self.output)
    }
}

/// A request for a typed result. `label` exists only for diagnostics and
/// provenance; selection uses `interface` exclusively (decision 0015).
#[derive(Clone, Debug)]
pub struct Request {
    pub label: Box<str>,
    pub interface: Interface,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub label: Box<str>,
    pub interface: Interface,
    pub span: Span,
}

/// Zero, one, or multiple matching providers. Ambiguity is never ranked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectOutcome {
    NoMatch,
    One(RuleId),
    Ambiguous(SmallVec<[RuleId; 2]>),
}

/// Select rules by exact typed-interface match, independent of registration
/// order. Candidate order is canonical so diagnostics are deterministic.
#[must_use]
pub fn select_rule(request: &Request, rules: &RuleArena<Rule>) -> SelectOutcome {
    let mut candidates: Vec<(Interface, Box<str>, RuleId)> = rules
        .iter()
        .filter(|(_, rule)| rule.interface == request.interface)
        .map(|(id, rule)| (rule.interface.clone(), rule.label.clone(), id))
        .collect();
    candidates.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

    let candidates: SmallVec<[RuleId; 2]> = candidates.into_iter().map(|(_, _, id)| id).collect();
    match candidates.as_slice() {
        [] => SelectOutcome::NoMatch,
        [only] => SelectOutcome::One(*only),
        _ => SelectOutcome::Ambiguous(candidates),
    }
}

impl SelectOutcome {
    /// # Errors
    /// `E-1101` when no rule matched; `E-1102` naming every candidate when more
    /// than one matched.
    pub fn into_result(self, request: &Request, rules: &RuleArena<Rule>) -> Result<RuleId, Diag> {
        match self {
            Self::One(id) => Ok(id),
            Self::NoMatch => Err(Diag::new(
                Severity::Error,
                StableCode::engine(101),
                request.span,
                format!(
                    "no rule satisfies `{}` ({})",
                    request.label, request.interface
                ),
            )),
            Self::Ambiguous(candidates) => {
                let mut diag = Diag::new(
                    Severity::Error,
                    StableCode::engine(102),
                    request.span,
                    format!(
                        "ambiguous rule for `{}` ({})",
                        request.label, request.interface
                    ),
                );
                for id in candidates {
                    if let Some(rule) = rules.get(id) {
                        diag = diag.with_note(
                            rule.span,
                            format!("candidate `{}`: {}", rule.label, rule.interface),
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

    fn interface(inputs: impl Into<Box<[Type]>>, output: Type) -> Interface {
        Interface {
            inputs: inputs.into(),
            output,
        }
    }

    fn rule(label: &str, interface: Interface) -> Rule {
        Rule {
            label: label.into(),
            interface,
            span: Span::none(),
        }
    }

    fn request(label: &str, interface: Interface) -> Request {
        Request {
            label: label.into(),
            interface,
            span: Span::none(),
        }
    }

    #[test]
    fn no_match_produces_error_with_typed_signature() {
        let arena: RuleArena<Rule> = Arena::new();
        let request = request("answer", interface([], Type::Int));
        let err = select_rule(&request, &arena)
            .into_result(&request, &arena)
            .unwrap_err();
        assert_eq!(err.code, StableCode::engine(101));
        assert_eq!(err.severity, Severity::Error);
        assert!(err.message.0.contains("() -> Int"));
    }

    #[test]
    fn labels_do_not_participate_in_selection() {
        let mut arena: RuleArena<Rule> = Arena::new();
        let bool_rule = arena.push(rule("same label", interface([], Type::Bool)));
        let int_rule = arena.push(rule("same label", interface([], Type::Int)));
        let request = request("different label", interface([], Type::Int));

        let selected = select_rule(&request, &arena)
            .into_result(&request, &arena)
            .unwrap();

        assert_ne!(selected, bool_rule);
        assert_eq!(selected, int_rule);
    }

    #[test]
    fn ambiguity_names_every_candidate_and_interface() {
        let mut arena: RuleArena<Rule> = Arena::new();
        let signature = interface([Type::Text], Type::Int);
        arena.push(rule("a", signature.clone()));
        arena.push(rule("b", signature.clone()));
        let request = request("thing", signature);

        let err = select_rule(&request, &arena)
            .into_result(&request, &arena)
            .unwrap_err();

        assert_eq!(err.code, StableCode::engine(102));
        assert_eq!(err.notes.len(), 2);
        assert!(
            err.notes
                .iter()
                .all(|note| note.message.0.contains("(Text) -> Int"))
        );
    }

    #[test]
    fn ambiguity_diagnostics_ignore_registration_order() {
        fn notes(reversed: bool) -> Vec<Box<str>> {
            let mut arena: RuleArena<Rule> = Arena::new();
            let signature = interface([], Type::Int);
            let labels = if reversed {
                ["beta", "alpha"]
            } else {
                ["alpha", "beta"]
            };
            for label in labels {
                arena.push(rule(label, signature.clone()));
            }
            let request = request("answer", signature);
            select_rule(&request, &arena)
                .into_result(&request, &arena)
                .unwrap_err()
                .notes
                .iter()
                .map(|note| note.message.0.clone())
                .collect()
        }

        assert_eq!(notes(false), notes(true));
    }
}
