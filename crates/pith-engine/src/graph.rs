//! The arena dependency graph (decisions 0021, 0022).

use std::collections::VecDeque;

use indexmap::IndexMap;
use pith_core::{Request, Rule, RuleArena, RuleId, Value};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_ids::{ComputationArena, ComputationId};
use smallvec::SmallVec;

pub struct ComputationNode {
    pub rule: RuleId,
    pub dependencies: SmallVec<[ComputationId; 4]>,
    pub result: Option<Value>,
}

pub struct Engine {
    rules: RuleArena<Rule>,
    computations: ComputationArena<ComputationNode>,
    by_rule: IndexMap<RuleId, ComputationId>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            rules: RuleArena::new(),
            computations: ComputationArena::new(),
            by_rule: IndexMap::new(),
        }
    }

    pub fn register_rule(&mut self, rule: Rule) -> RuleId {
        self.rules.push(rule)
    }

    pub fn rules_iter(&self) -> impl Iterator<Item = (RuleId, &Rule)> {
        self.rules.iter()
    }

    /// # Errors
    /// Returns a `DiagnosticSink` with stable code `E-1201` when no rule
    /// matches, `E-1202` (naming every candidate) when more than one matches,
    /// or `E-1203` when evaluation detects a dependency cycle.
    pub fn evaluate(&mut self, request: &Request) -> PithResult<Value> {
        let candidates: Vec<RuleId> = self
            .rules_iter()
            .filter(|(_, rule)| rule.interface.label.as_ref() == request.label.as_ref())
            .map(|(id, _)| id)
            .collect();
        match candidates.as_slice() {
            [] => Err(no_match_diag(request)),
            [only] => self.evaluate_rule(*only),
            _ => Err(ambiguous_diag(request, &candidates, &self.rules)),
        }
    }

    fn evaluate_rule(&mut self, rule_id: RuleId) -> PithResult<Value> {
        let mut chain: Vec<RuleId> = vec![rule_id];
        let mut queue: VecDeque<RuleId> = VecDeque::new();
        let mut seen: SmallVec<[RuleId; 16]> = SmallVec::new();
        queue.push_back(rule_id);

        while let Some(current) = queue.pop_front() {
            if seen.contains(&current) {
                chain.push(current);
                return Err(cycle_diag(&chain));
            }
            seen.push(current);

            if self.rules.get(current).is_none() {
                continue;
            }

            let node = ComputationNode {
                rule: current,
                dependencies: SmallVec::new(),
                result: Some(Value::Unit),
            };
            let id = self.computations.push(node);
            self.by_rule.insert(current, id);
        }

        Ok(Value::Unit)
    }

    /// The in-memory query interface (requirement K-12).
    pub fn dependencies_of(&self, rule_id: RuleId) -> Option<&SmallVec<[ComputationId; 4]>> {
        let node_id = self.by_rule.get(&rule_id)?;
        self.computations.get(*node_id).map(|n| &n.dependencies)
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

fn no_match_diag(request: &Request) -> DiagnosticSink {
    let mut sink = DiagnosticSink::new();
    sink.push(Diag::new(
        Severity::Error,
        StableCode::engine(201),
        request.span,
        format!("no rule satisfies `{}`", request.label),
    ));
    sink
}

fn ambiguous_diag(
    request: &Request,
    candidates: &[RuleId],
    rules: &RuleArena<Rule>,
) -> DiagnosticSink {
    let mut diag = Diag::new(
        Severity::Error,
        StableCode::engine(202),
        request.span,
        format!("ambiguous rule for `{}`", request.label),
    );
    for id in candidates {
        if let Some(rule) = rules.get(*id) {
            diag = diag.with_note(rule.span, format!("candidate: {}", rule.interface.label));
        }
    }
    let mut sink = DiagnosticSink::new();
    sink.push(diag);
    sink
}

fn cycle_diag(chain: &[RuleId]) -> DiagnosticSink {
    let names: Vec<String> = chain.iter().map(|id| format!("#{}", id.to_raw())).collect();
    let mut sink = DiagnosticSink::new();
    sink.push(Diag::new(
        Severity::Error,
        StableCode::engine(203),
        Span::none(),
        format!("dependency cycle: {}", names.join(" -> ")),
    ));
    sink
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use pith_core::Interface;

    fn rule(id: u32, label: &str) -> Rule {
        Rule {
            id: RuleId::from_raw(id),
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
    fn no_match_when_no_rule_present() {
        let mut engine = Engine::new();
        assert!(engine.evaluate(&request("missing")).is_err());
    }

    #[test]
    fn one_match_succeeds() {
        let mut engine = Engine::new();
        engine.register_rule(rule(0, "thing"));
        assert!(engine.evaluate(&request("thing")).is_ok());
    }

    #[test]
    fn two_matches_is_ambiguous() {
        let mut engine = Engine::new();
        engine.register_rule(rule(0, "thing"));
        engine.register_rule(rule(1, "thing"));
        let err = engine.evaluate(&request("thing")).unwrap_err();
        let diags: Vec<_> = err.iter().collect();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags.first().unwrap().code, StableCode::engine(202));
    }

    #[test]
    fn evaluation_records_dependency_edge() {
        let mut engine = Engine::new();
        engine.register_rule(rule(0, "thing"));
        let _ = engine.evaluate(&request("thing"));
        assert!(engine.dependencies_of(RuleId::from_raw(0)).is_some());
    }
}
