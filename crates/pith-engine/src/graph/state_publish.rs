//! Translation from arena computation state into durable engine-state records.
//!
//! The live [`Engine`] publishes every computation that leaves the `Pending`
//! state through [`crate::state::EngineStateStore`] (decision 0024). Arena
//! handles never cross the boundary: the private `Engine::durable_attempts`
//! table maps each computation node to its durable attempt identifier, and
//! edges are translated into the typed durable records re-exported by
//! [`crate::state`].
//!
//! Store calls happen only at engine scheduling boundaries (decision 0024,
//! "adapter boundaries") — frame allocation, terminal transitions, and the
//! reuse check. The synchronous pure evaluator's rule step never touches the
//! store.

use pith_core::PureComputationKey;
use pith_diag::{DiagnosticSink, PithResult};
use pith_ids::ComputationId;

use super::Engine;
use super::ir::{
    AttemptState, ComputationKind, DependencyEdge, ReuseDecision, ReuseReason, StopReason,
};
use crate::graph::diagnostics::{InternalInvariant, internal_diag};
use crate::state::{
    CompletedAttempt, DurableAttemptId, DurableDependency, DurableDiagnostic, DurableProvenance,
    DurableReuseDecision, DurableReuseReason, EncodedValue, StoppedAttempt,
};

impl Engine {
    pub(super) fn create_pending_pure_attempt(
        &mut self,
        computation: ComputationId,
        key: PureComputationKey,
    ) -> PithResult<()> {
        let attempt = self
            .state_store
            .create_pending_attempt(crate::state::DurableComputation::Pure(key))
            .map_err(engine_state_diag)?;
        self.durable_attempts.insert(computation, attempt);
        Ok(())
    }

    pub(super) fn create_pending_action_attempt(
        &mut self,
        computation: ComputationId,
        computation_kind: crate::state::DurableComputation,
    ) -> PithResult<()> {
        let attempt = self
            .state_store
            .create_pending_attempt(computation_kind)
            .map_err(engine_state_diag)?;
        self.durable_attempts.insert(computation, attempt);
        Ok(())
    }

    pub(super) fn publish_pure_completion(&mut self, computation: ComputationId) -> PithResult<()> {
        let dependencies = match self.computations.get(computation) {
            Some(node) => self.translate_dependencies(&node.dependencies)?,
            None => return Err(internal_diag(InternalInvariant::PureLostComputationNode)),
        };
        let (result, reuse) = match self.computations.get(computation) {
            Some(node) => match &node.state {
                AttemptState::Complete { result, reuse } => (result.clone(), reuse.clone()),
                _ => {
                    return Err(internal_diag(
                        InternalInvariant::DurablePublicationForNonCompleteAttempt,
                    ));
                }
            },
            None => return Err(internal_diag(InternalInvariant::PureLostComputationNode)),
        };
        let reuse_decision =
            self.translate_pure_reuse_decision(&reuse, &self.dependencies_of(computation))?;
        let completion = CompletedAttempt {
            dependencies,
            result: EncodedValue::from_value(&result),
            provenance: DurableProvenance::Pure,
            reuse: reuse_decision,
        };
        let attempt = self.require_durable_attempt(computation)?;
        self.state_store
            .publish_complete(attempt, completion)
            .map_err(engine_state_diag)
    }

    /// Publish the terminal record for a pure attempt that stopped without a
    /// result. Which terminal state is published comes from the arena node, so
    /// the durable record cannot disagree with the graph about whether the
    /// computation failed or was cancelled.
    pub(super) fn publish_pure_stop(&mut self, computation: ComputationId) -> PithResult<()> {
        let (dependencies, diagnostics, reason) = match self.computations.get(computation) {
            Some(node) => {
                let (diagnostics, reason) = match &node.state {
                    AttemptState::Failed { diagnostics } => {
                        (diagnostics.clone(), StopReason::Failed)
                    }
                    AttemptState::Cancelled { diagnostics } => {
                        (diagnostics.clone(), StopReason::Cancelled)
                    }
                    _ => {
                        return Err(internal_diag(
                            InternalInvariant::DurablePublicationForNonFailedAttempt,
                        ));
                    }
                };
                let dependencies = self.translate_dependencies(&node.dependencies)?;
                (dependencies, diagnostics, reason)
            }
            None => return Err(internal_diag(InternalInvariant::PureLostComputationNode)),
        };
        let stopped = StoppedAttempt {
            dependencies,
            diagnostics: diagnostics.iter().map(DurableDiagnostic::from).collect(),
            provenance: DurableProvenance::Pure,
        };
        let attempt = self.require_durable_attempt(computation)?;
        self.publish_stopped(attempt, stopped, reason)
    }

    /// Hand a stopped record to the adapter under the terminal state `reason`
    /// names. The two publications differ only here, so the record they carry
    /// cannot drift apart.
    fn publish_stopped(
        &self,
        attempt: DurableAttemptId,
        stopped: StoppedAttempt,
        reason: StopReason,
    ) -> PithResult<()> {
        match reason {
            StopReason::Failed => self.state_store.publish_failed(attempt, stopped),
            StopReason::Cancelled => self.state_store.publish_cancelled(attempt, stopped),
        }
        .map_err(engine_state_diag)
    }

    /// Publish a completed action attempt. Provenance comes from the action
    /// record: a completed action always has an imported report.
    pub(super) fn publish_action_completion(
        &mut self,
        computation: ComputationId,
    ) -> PithResult<()> {
        let dependencies = match self.computations.get(computation) {
            Some(node) => self.translate_dependencies(&node.dependencies)?,
            None => return Err(internal_diag(InternalInvariant::ActionLostComputationNode)),
        };
        let (result, action) = match self.computations.get(computation) {
            Some(node) => match (&node.state, node.action.as_ref()) {
                (AttemptState::Complete { result, .. }, Some(action)) => (result.clone(), action),
                _ => {
                    return Err(internal_diag(
                        InternalInvariant::DurablePublicationForNonCompleteAttempt,
                    ));
                }
            },
            None => return Err(internal_diag(InternalInvariant::ActionLostComputationNode)),
        };
        let Some(imported_report) = action.imported_report.clone() else {
            return Err(internal_diag(
                InternalInvariant::CompletedActionMissingImportedReport,
            ));
        };
        let completion = CompletedAttempt {
            dependencies,
            result: EncodedValue::from_value(&result),
            provenance: DurableProvenance::Action(
                crate::state::DurableActionProvenance::Imported { imported_report },
            ),
            reuse: DurableReuseDecision::NotReusable(DurableReuseReason::ActionCachingDisabled),
        };
        let attempt = self.require_durable_attempt(computation)?;
        self.state_store
            .publish_complete(attempt, completion)
            .map_err(engine_state_diag)
    }

    /// Publish a failed action attempt, retaining whatever provenance the
    /// lifecycle reached: `NotExecuted` when the executor was never called
    /// (denial, materialization failure), `Captured` when the executor
    /// returned but the engine failed to import outputs.
    pub(super) fn publish_action_stop(
        &mut self,
        computation: ComputationId,
        diagnostics: &DiagnosticSink,
        reason: StopReason,
    ) -> PithResult<()> {
        let dependencies = match self.computations.get(computation) {
            Some(node) => self.translate_dependencies(&node.dependencies)?,
            None => return Err(internal_diag(InternalInvariant::ActionLostComputationNode)),
        };
        let provenance = self.action_failure_provenance(computation);
        let stopped = StoppedAttempt {
            dependencies,
            diagnostics: diagnostics.iter().map(DurableDiagnostic::from).collect(),
            provenance,
        };
        let attempt = self.require_durable_attempt(computation)?;
        self.publish_stopped(attempt, stopped, reason)
    }

    fn action_failure_provenance(&self, computation: ComputationId) -> DurableProvenance {
        let Some(node) = self.computations.get(computation) else {
            return DurableProvenance::Action(crate::state::DurableActionProvenance::NotExecuted);
        };
        let provenance = match node.action.as_ref().and_then(|action| {
            action.executor_report.as_ref().map(|captured| {
                crate::state::DurableActionProvenance::Captured {
                    executor_report: crate::state::DurableCapturedExecutionReport::from(captured),
                }
            })
        }) {
            Some(captured) => captured,
            None => crate::state::DurableActionProvenance::NotExecuted,
        };
        DurableProvenance::Action(provenance)
    }

    fn require_durable_attempt(&self, computation: ComputationId) -> PithResult<DurableAttemptId> {
        self.durable_attempts
            .get(&computation)
            .copied()
            .ok_or_else(|| internal_diag(InternalInvariant::DurableAttemptMissingForComputation))
    }

    fn dependencies_of(&self, computation: ComputationId) -> Box<[DependencyEdge]> {
        self.computations
            .get(computation)
            .map(|node| node.dependencies.to_vec().into_boxed_slice())
            .unwrap_or_default()
    }

    /// Translate arena dependency edges into durable edges, preserving order.
    /// `Request` edges become `Pure` edges carrying the child's computation key
    /// and durable attempt id; `Action` edges carry the action's durable id.
    fn translate_dependencies(
        &self,
        dependencies: &[DependencyEdge],
    ) -> PithResult<Box<[DurableDependency]>> {
        dependencies
            .iter()
            .map(|edge| match edge {
                DependencyEdge::Request { computation, .. } => {
                    self.translate_pure_edge(*computation)
                }
                DependencyEdge::Action { computation, .. } => {
                    self.translate_action_edge(*computation)
                }
                DependencyEdge::Blob { id } => Ok(DurableDependency::Blob { content: *id }),
                DependencyEdge::CapabilityUse { capability } => {
                    Ok(DurableDependency::CapabilityUse {
                        capability: capability.clone(),
                    })
                }
            })
            .collect()
    }

    fn translate_pure_edge(&self, computation: ComputationId) -> PithResult<DurableDependency> {
        let key = self
            .pure_computation_key(computation)
            .ok_or_else(|| internal_diag(InternalInvariant::DurablePureEdgeTargetNotPure))?;
        let attempt = self.require_durable_attempt(computation)?;
        Ok(DurableDependency::Pure {
            computation: key,
            attempt,
        })
    }

    fn translate_action_edge(&self, computation: ComputationId) -> PithResult<DurableDependency> {
        let attempt = self.require_durable_attempt(computation)?;
        Ok(DurableDependency::Action { attempt })
    }

    /// The durable reuse decision for a pure attempt. Pure parents derive their
    /// reuse from their dependencies; the memory adapter validates this, so the
    /// reason's durable attempt id must match the first non-reusable edge.
    fn translate_pure_reuse_decision(
        &self,
        decision: &ReuseDecision,
        dependencies: &[DependencyEdge],
    ) -> PithResult<DurableReuseDecision> {
        match decision {
            ReuseDecision::Reusable => Ok(DurableReuseDecision::Reusable),
            ReuseDecision::NotReusable(reason) => match reason {
                ReuseReason::ActionCachingDisabled => Ok(DurableReuseDecision::NotReusable(
                    DurableReuseReason::ActionCachingDisabled,
                )),
                ReuseReason::DependencyPending { computation }
                | ReuseReason::DependencyNotReusable { computation }
                | ReuseReason::DependencyMissing { computation } => {
                    let attempt = dependencies
                        .iter()
                        .find_map(|edge| match edge {
                            DependencyEdge::Request {
                                computation: target,
                                ..
                            }
                            | DependencyEdge::Action {
                                computation: target,
                                ..
                            } if target == computation => Some(*target),
                            _ => None,
                        })
                        .and_then(|target| self.durable_attempts.get(&target).copied());
                    match attempt {
                        Some(attempt) => match reason {
                            ReuseReason::DependencyPending { .. } => {
                                Ok(DurableReuseDecision::NotReusable(
                                    DurableReuseReason::DependencyPending { attempt },
                                ))
                            }
                            ReuseReason::DependencyNotReusable { .. }
                            | ReuseReason::DependencyMissing { .. } => {
                                Ok(DurableReuseDecision::NotReusable(
                                    DurableReuseReason::DependencyNotReusable { attempt },
                                ))
                            }
                            ReuseReason::ActionCachingDisabled => {
                                Ok(DurableReuseDecision::NotReusable(
                                    DurableReuseReason::ActionCachingDisabled,
                                ))
                            }
                        },
                        None => {
                            let key = self.pure_computation_key(*computation).ok_or_else(|| {
                                internal_diag(InternalInvariant::DurablePureEdgeTargetNotPure)
                            })?;
                            Ok(DurableReuseDecision::NotReusable(
                                DurableReuseReason::DependencyMissing { computation: key },
                            ))
                        }
                    }
                }
            },
        }
    }
}

impl Engine {
    /// A pure computation key requires a `Pure` request. `None` when the node
    /// is an action computation; callers that depend on a key report it as an
    /// invariant violation.
    pub(super) fn pure_computation_key(
        &self,
        computation: ComputationId,
    ) -> Option<PureComputationKey> {
        let node = self.computations.get(computation)?;
        let ComputationKind::Pure(request) = &node.kind else {
            return None;
        };
        let rule = self.rules.get(node.rule)?;
        Some(PureComputationKey::new(rule, request))
    }

    /// Build a durable action plan from the selected action rule's revision and
    /// the validated spec. The rule's stable identity is derived from its
    /// revision (decision 0023); the arena `RuleId` never crosses the boundary.
    pub(super) fn durable_action_plan(
        &self,
        rule: pith_core::RuleId,
        spec: &pith_core::ActionSpec,
    ) -> PithResult<crate::state::DurableActionPlan> {
        let Some(action_rule) = self.action_rules.get(rule) else {
            return Err(internal_diag(
                InternalInvariant::SelectedActionRuleHasNoMetadata,
            ));
        };
        let durable_rule = crate::state::DurableRule::new(action_rule.revision);
        crate::state::DurableActionPlan::new(durable_rule, spec.clone()).map_err(|diagnostic| {
            let mut sink = DiagnosticSink::new();
            sink.push(diagnostic);
            sink
        })
    }
}

fn engine_state_diag(error: crate::state::EngineStateError) -> DiagnosticSink {
    internal_diag(InternalInvariant::EngineStateStoreError(error))
}
