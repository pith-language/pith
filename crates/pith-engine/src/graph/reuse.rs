use indexmap::IndexMap;
use pith_core::{Pure, PureComputationKey, Request, RuleId};
use pith_ids::ComputationId;

use super::{
    AttemptState, DependencyEdge, Engine, Evaluation, EvaluationSource, ReuseDecision, ReuseReason,
};
use crate::state::{
    CompletedAttempt, DurableAttemptState, DurableDependency, DurableReuseDecision,
};

impl Engine {
    pub(super) fn reusable_pure_evaluation(
        &self,
        rule: RuleId,
        request: &Request<Pure>,
    ) -> Option<Evaluation> {
        let key = PureComputationKey::new(self.rules.get(rule)?, request);
        let computation = *self.pure_computations.get(&key)?;
        let node = self.computations.get(computation)?;
        let AttemptState::Complete { result, reuse } = &node.state else {
            return None;
        };
        if reuse != &ReuseDecision::Reusable {
            return None;
        }
        // The arena graph says reusable. Decision 0024 ("dynamic dependencies
        // and revalidation") also requires confirming the durable record's
        // dependency set has not drifted: a dependency whose durable result
        // identity changed makes the consumer dirty. Within one live engine
        // instance the in-memory index keeps this trivially consistent, so this
        // gate is a correctness check that is ready for cross-instance reuse.
        if !self.durable_reuse_is_valid(computation) {
            return None;
        }
        Some(Evaluation {
            value: result.clone(),
            computation,
            source: EvaluationSource::Reused,
        })
    }

    /// Revalidate a reusable pure attempt against its durable dependency set
    /// (decision 0024). A `Pure` dependency whose latest reusable attempt is a
    /// *different* attempt with a *different* result (canonical equality) makes
    /// the consumer dirty. A different attempt with an equal result leaves the
    /// consumer reusable — downstream propagation stops, even though a new
    /// attempt would record the changed upstream provenance. `Blob` and
    /// `CapabilityUse` edges have stable identity and need no revalidation.
    /// `Action` edges cannot appear on a reusable pure attempt (the store
    /// validates this), so they are rejected defensively.
    pub fn durable_reuse_is_valid(&self, computation: ComputationId) -> bool {
        let Some(attempt_id) = self.durable_attempts.get(&computation).copied() else {
            return false;
        };
        let Ok(Some(attempt)) = self.state_store.attempt(attempt_id) else {
            return false;
        };
        let DurableAttemptState::Complete(CompletedAttempt { dependencies, .. }) = &attempt.state
        else {
            return false;
        };
        for dependency in dependencies {
            if !self.durable_dependency_is_valid(dependency) {
                return false;
            }
        }
        true
    }

    fn durable_dependency_is_valid(&self, dependency: &DurableDependency) -> bool {
        match dependency {
            DurableDependency::Pure {
                computation,
                attempt: recorded_attempt,
            } => {
                let Ok(latest) = self
                    .state_store
                    .latest_completed_reusable_attempt(*computation)
                else {
                    return false;
                };
                let Some(latest) = latest else {
                    // The dependency is no longer available as a reusable
                    // result: the consumer must be re-evaluated.
                    return false;
                };
                if latest.id == *recorded_attempt {
                    return true;
                }
                // The dependency was recomputed under a new attempt. If its
                // result identity is unchanged (canonical equality), the
                // consumer is not dirty: propagation stops here.
                let DurableAttemptState::Complete(CompletedAttempt {
                    result: latest_result,
                    reuse: DurableReuseDecision::Reusable,
                    ..
                }) = &latest.state
                else {
                    return false;
                };
                let Ok(Some(recorded)) = self.state_store.attempt(*recorded_attempt) else {
                    return false;
                };
                let DurableAttemptState::Complete(CompletedAttempt {
                    result: recorded_result,
                    ..
                }) = &recorded.state
                else {
                    return false;
                };
                *latest_result == *recorded_result
            }
            DurableDependency::Blob { .. } | DurableDependency::CapabilityUse { .. } => true,
            // A reusable pure attempt never has action dependencies: the store
            // validates that a reusable pure computation's reuse decision is
            // consistent with its dependencies. Treat an action edge as invalid
            // rather than silently accepting it.
            DurableDependency::Action { .. } => false,
        }
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
            match &node.state {
                AttemptState::Complete {
                    reuse: ReuseDecision::Reusable,
                    ..
                } => {}
                AttemptState::Pending => {
                    return ReuseDecision::NotReusable(ReuseReason::DependencyPending {
                        computation,
                    });
                }
                AttemptState::Complete {
                    reuse: ReuseDecision::NotReusable(_),
                    ..
                }
                | AttemptState::Failed { .. } => {
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
