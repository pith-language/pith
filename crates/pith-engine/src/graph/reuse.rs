//! Reuse of completed computations from the live arena and durable engine
//! state.

mod action;
mod pure;
mod revalidation;

use indexmap::IndexMap;
use pith_core::ComputationKey;
use pith_diag::DiagnosticSink;
use pith_ids::ComputationId;

use super::{AttemptState, DependencyEdge, Engine, ReuseDecision, ReuseReason};
use crate::action::ExecutorIdentity;
use crate::graph::diagnostics::{InternalInvariant, internal_diag};
use crate::policy::ActionPolicy;

/// What revalidating a recorded action edge needs from the run considering it.
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
    pub(super) fn reuse_decision(&self, dependencies: &[DependencyEdge]) -> ReuseDecision {
        for dependency in dependencies {
            // Blob and capability edges point at nothing with a result, so
            // they cannot disqualify the computation that recorded them.
            let Some(computation) = dependency.computation_id() else {
                continue;
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

/// The live reuse index for one category's rule applications.
pub(super) type ComputationIndex<K> = IndexMap<ComputationKey<K>, ComputationId>;
