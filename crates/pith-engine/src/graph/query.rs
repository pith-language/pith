//! The read-only query interface over a built graph (requirement K-12).

use std::sync::Arc;

use pith_core::{Action, CapabilityRequirement, Pure, Request, Rule, RuleId};
use pith_diag::{Diag, PithResult};
use pith_ids::ComputationId;

use super::Engine;
use super::diagnostics::{InternalInvariant, internal_diag};
use super::ir::{
    ActionPlan, AttemptState, ComputationNode, DependencyEdge, LiveInvalidationExplanation,
    LiveInvalidationReason, ReuseDecision, ReuseReason, RuleSelection,
};
use crate::state::{DurableAttempt, EngineStateReader, EngineStateStore};

pub struct EngineQuery<'engine, S: EngineStateReader + ?Sized = dyn EngineStateStore> {
    engine: &'engine Engine<S>,
}

impl<'engine, S: EngineStateReader + ?Sized> EngineQuery<'engine, S> {
    /// # Errors
    /// Returns `E-1101`, `E-1102`, or `E-1103` when the request cannot select
    /// exactly one rule.
    pub fn select(&self, request: &Request<Pure>) -> Result<RuleSelection, Diag> {
        request.validate_inputs()?;
        let rule = self
            .engine
            .rules
            .select(request)
            .into_result(request, &self.engine.rules)?;
        Ok(RuleSelection {
            rule,
            interface: request.interface.clone(),
        })
    }

    /// Select and plan an action without executing external work.
    ///
    /// # Errors
    /// Returns selection, input-validation, or planner diagnostics.
    pub fn plan_action(&self, request: &Request<Action>) -> PithResult<ActionPlan> {
        self.engine.plan_action(request)
    }

    pub fn rules(&self) -> impl Iterator<Item = (RuleId, &'engine Rule<Pure>)> + 'engine {
        self.engine.rules.iter()
    }

    pub fn rule(&self, id: RuleId) -> Option<&'engine Rule<Pure>> {
        self.engine.rules.get(id)
    }

    pub fn computations(
        &self,
    ) -> impl Iterator<Item = (ComputationId, &'engine ComputationNode)> + 'engine {
        self.engine.computations.iter()
    }

    pub fn computation(&self, id: ComputationId) -> Option<&'engine ComputationNode> {
        self.engine.computations.get(id)
    }

    pub fn dependencies_of(&self, id: ComputationId) -> Option<&'engine [DependencyEdge]> {
        self.engine
            .computations
            .get(id)
            .map(|node| node.dependencies.as_slice())
    }

    pub fn capabilities_of(&self, id: ComputationId) -> Option<&'engine [CapabilityRequirement]> {
        self.engine
            .computations
            .get(id)
            .map(|node| node.capabilities.as_ref())
    }

    pub fn capability_uses_of(
        &self,
        id: ComputationId,
    ) -> Option<impl Iterator<Item = &'engine CapabilityRequirement>> {
        self.engine.computations.get(id).map(|node| {
            node.dependencies.iter().filter_map(|edge| match edge {
                DependencyEdge::CapabilityUse { capability } => Some(capability),
                DependencyEdge::Request { .. }
                | DependencyEdge::Blob { .. }
                | DependencyEdge::Action { .. }
                | DependencyEdge::Observation { .. } => None,
            })
        })
    }

    /// The durable attempt this engine recorded or loaded for `id`, carrying
    /// the recorded dependency set, result, provenance, and reuse decision.
    ///
    /// This is the authoritative dependency provenance for a computation whose
    /// [`EvaluationSource`] is [`Hydrated`]: such a node was loaded terminal
    /// from engine state, so [`Self::dependencies_of`] reports the empty arena
    /// subgraph while the recorded edges live here.
    ///
    /// `Ok(None)` when `id` is unknown to this engine or never left `Pending`.
    ///
    /// [`EvaluationSource`]: super::EvaluationSource
    /// [`Hydrated`]: super::EvaluationSource::Hydrated
    ///
    /// # Errors
    /// `E-1207` (internal invariant) when the engine-state adapter cannot be
    /// read.
    pub fn durable_attempt_of(&self, id: ComputationId) -> PithResult<Option<Arc<DurableAttempt>>> {
        let Some(attempt) = self.engine.durable_attempt_for(id) else {
            return Ok(None);
        };
        self.engine
            .state_store
            .attempt(attempt)
            .map_err(|error| internal_diag(InternalInvariant::EngineStateReadFailed(error)))
    }

    pub fn dependents_of(
        &self,
        dependency: ComputationId,
    ) -> impl Iterator<Item = (ComputationId, &'engine DependencyEdge)> + 'engine {
        self.engine
            .computations
            .iter()
            .flat_map(move |(computation, node)| {
                node.dependencies
                    .iter()
                    .filter(move |edge| edge.computation_id() == Some(dependency))
                    .map(move |edge| (computation, edge))
            })
    }

    /// Explain why `computation` is not reusable, as a chain over the live
    /// arena graph. `None` when the computation is unknown, not complete, or
    /// reusable (there is nothing to explain). The live-graph mirror of the
    /// durable [`EngineStateReader::explain_invalidation`], keyed on
    /// [`ComputationId`] rather than `DurableAttemptId`.
    ///
    /// The chain follows the single dependency the computation's own
    /// [`ReuseReason`] names, so the explanation
    /// matches the reuse decision the engine computed for the node.
    ///
    /// [`EngineStateReader::explain_invalidation`]:
    ///     crate::state::EngineStateReader::explain_invalidation
    #[must_use]
    pub fn explain_invalidation(
        &self,
        computation: ComputationId,
    ) -> Option<LiveInvalidationExplanation> {
        Self::explain(self.engine, computation, &mut Vec::new())
    }

    fn explain(
        engine: &Engine<S>,
        computation: ComputationId,
        visiting: &mut Vec<ComputationId>,
    ) -> Option<LiveInvalidationExplanation> {
        let node = engine.computations.get(computation)?;
        let AttemptState::Complete {
            reuse: ReuseDecision::NotReusable(reason),
            ..
        } = &node.state
        else {
            return None;
        };
        let reason = reason.clone();
        let dependencies = node.dependencies.clone();
        // Cycle guard: the live graph is built by the step machine under a
        // cycle check, but a corrupt arena is a lookup fault, not a reason to
        // overflow.
        if visiting.contains(&computation) {
            return Some(LiveInvalidationExplanation {
                computation,
                reason: LiveInvalidationReason::Leaf(reason),
            });
        }
        visiting.push(computation);
        let named = live_named_dependency(&reason);
        let edge = named.and_then(|target| {
            dependencies
                .iter()
                .find(|edge| edge.computation_id() == Some(target))
                .cloned()
        });
        let child = match (named, edge) {
            (Some(_), Some(edge)) => {
                let child_id = edge.computation_id()?;
                Self::explain(engine, child_id, visiting)
            }
            _ => None,
        };
        let reason = match child {
            Some(child) => LiveInvalidationReason::DependencyInvalidated {
                edge: dependencies
                    .iter()
                    .find(|edge| edge.computation_id() == Some(child.computation))
                    .cloned()?,
                child: Box::new(child),
            },
            None => LiveInvalidationReason::Leaf(reason),
        };
        Some(LiveInvalidationExplanation {
            computation,
            reason,
        })
    }
}

/// The computation a live reuse reason names as the cause of non-reuse, if any.
fn live_named_dependency(reason: &ReuseReason) -> Option<ComputationId> {
    match reason {
        ReuseReason::EffectfulDependency { computation }
        | ReuseReason::DependencyPending { computation }
        | ReuseReason::DependencyNotReusable { computation }
        | ReuseReason::DependencyMissing { computation } => Some(*computation),
        ReuseReason::ActionCachingDisabled => None,
    }
}

impl<S: EngineStateReader + ?Sized> Engine<S> {
    pub fn query(&self) -> EngineQuery<'_, S> {
        EngineQuery { engine: self }
    }
}
