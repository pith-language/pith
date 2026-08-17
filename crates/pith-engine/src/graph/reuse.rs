//! Reuse of completed computations from the live arena and durable engine
//! state (decisions 0024, 0031, and 0033).

mod action;
mod pure;
mod revalidation;

use indexmap::IndexMap;
use pith_core::{ActionComputationKey, PureComputationKey};
use pith_diag::DiagnosticSink;
use pith_ids::ComputationId;

use super::{AttemptState, DependencyEdge, Engine, ReuseDecision, ReuseReason};
use crate::action::ExecutorIdentity;
use crate::graph::diagnostics::{InternalInvariant, internal_diag};
use crate::policy::ActionPolicy;

/// What revalidating a recorded action edge needs from the run considering it
/// (decision 0033).
pub enum ReuseContext<'a> {
    PureOnly,
    Run {
        policy: &'a dyn ActionPolicy,
        environment: &'a ExecutorIdentity,
    },
}

impl<'a> ReuseContext<'a> {
    pub(super) fn run(&self) -> Option<(&'a dyn ActionPolicy, &'a ExecutorIdentity)> {
        match self {
            Self::PureOnly => None,
            Self::Run {
                policy,
                environment,
            } => Some((*policy, *environment)),
        }
    }
}

impl Engine {
    pub(super) fn index_action_computation(
        &mut self,
        key: ActionComputationKey,
        computation: ComputationId,
    ) {
        self.action_computations.insert(key, computation);
    }

    pub(super) fn index_pure_computation(
        &mut self,
        key: PureComputationKey,
        computation: ComputationId,
    ) {
        self.pure_computations.insert(key, computation);
    }

    pub(super) fn reuse_decision(&self, dependencies: &[DependencyEdge]) -> ReuseDecision {
        for dependency in dependencies {
            let computation = match dependency {
                DependencyEdge::Blob { .. } | DependencyEdge::CapabilityUse { .. } => continue,
                DependencyEdge::Action { computation, .. }
                | DependencyEdge::Request { computation, .. } => *computation,
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
                | AttemptState::Failed { .. }
                | AttemptState::Cancelled { .. } => {
                    return ReuseDecision::NotReusable(ReuseReason::DependencyNotReusable {
                        computation,
                    });
                }
            }
        }
        ReuseDecision::Reusable
    }
}

fn read_failed(error: crate::state::EngineStateError) -> DiagnosticSink {
    internal_diag(InternalInvariant::EngineStateReadFailed(error))
}

pub(super) type PureComputationIndex = IndexMap<PureComputationKey, ComputationId>;
pub(super) type ActionComputationIndex = IndexMap<ActionComputationKey, ComputationId>;
