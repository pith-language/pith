use indexmap::IndexMap;
use pith_core::{Pure, PureComputationKey, Request, RuleId};
use pith_ids::ComputationId;

use super::{DependencyEdge, Engine, Evaluation, EvaluationSource, ReuseDecision, ReuseReason};

impl Engine {
    pub(super) fn reusable_pure_evaluation(
        &self,
        rule: RuleId,
        request: &Request<Pure>,
    ) -> Option<Evaluation> {
        let key = PureComputationKey::new(self.rules.get(rule)?, request);
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
        key: PureComputationKey,
        computation: ComputationId,
    ) {
        self.pure_computations.insert(key, computation);
    }

    pub(super) fn pure_reuse_decision(&self, dependencies: &[DependencyEdge]) -> ReuseDecision {
        for dependency in dependencies {
            let computation = match dependency {
                DependencyEdge::Blob { .. } | DependencyEdge::CapabilityUse { .. } => continue,
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

pub(super) type PureComputationIndex = IndexMap<PureComputationKey, ComputationId>;
