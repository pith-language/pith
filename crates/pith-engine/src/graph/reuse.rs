use indexmap::IndexMap;
use pith_core::{Interface, Pure, Request, RuleId, Value};
use pith_ids::ComputationId;

use super::{DependencyEdge, Engine, Evaluation, EvaluationSource, ReuseDecision, ReuseReason};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct PureApplicationKey {
    rule: RuleId,
    interface: Interface,
    inputs: Box<[Value]>,
}

impl PureApplicationKey {
    pub(super) fn new(rule: RuleId, request: &Request<Pure>) -> Self {
        Self {
            rule,
            interface: request.interface.clone(),
            inputs: request.inputs.clone(),
        }
    }
}

impl Engine {
    pub(super) fn reusable_pure_evaluation(
        &self,
        rule: RuleId,
        request: &Request<Pure>,
    ) -> Option<Evaluation> {
        let key = PureApplicationKey::new(rule, request);
        let computation = *self.pure_computations.get(&key)?;
        let node = self.computations.get(computation)?;
        if node.reuse != ReuseDecision::Reusable {
            return None;
        }
        let value = node.result.clone()?;
        Some(Evaluation {
            value,
            computation,
            source: EvaluationSource::Reused,
        })
    }

    pub(super) fn index_pure_computation(
        &mut self,
        rule: RuleId,
        request: &Request<Pure>,
        computation: ComputationId,
    ) {
        let key = PureApplicationKey::new(rule, request);
        self.pure_computations.insert(key, computation);
    }

    pub(super) fn pure_reuse_decision(&self, dependencies: &[DependencyEdge]) -> ReuseDecision {
        for dependency in dependencies {
            let computation = match dependency {
                DependencyEdge::Blob { .. } => continue,
                DependencyEdge::Request { computation, .. }
                | DependencyEdge::Action { computation, .. } => *computation,
            };
            let Some(node) = self.computations.get(computation) else {
                return ReuseDecision::NotReusable(ReuseReason::DependencyMissing { computation });
            };
            match node.reuse {
                ReuseDecision::Reusable => {}
                ReuseDecision::Pending => {
                    return ReuseDecision::NotReusable(ReuseReason::DependencyPending {
                        computation,
                    });
                }
                ReuseDecision::NotReusable(_) => {
                    return ReuseDecision::NotReusable(ReuseReason::DependencyNotReusable {
                        computation,
                    });
                }
            }
        }
        ReuseDecision::Reusable
    }
}

pub(super) type PureComputationIndex = IndexMap<PureApplicationKey, ComputationId>;
