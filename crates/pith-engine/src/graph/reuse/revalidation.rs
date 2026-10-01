use std::sync::Arc;

use indexmap::IndexSet;
use pith_core::{
    Action, Content, Observation, ObservationComputationKey, PureComputationKey, Request,
};
use pith_diag::{PithResult, Span};
use pith_ids::ComputationId;

use super::{ReuseContext, read_failed};
use crate::action::{ExecutionReport, ExecutorIdentity};
use crate::graph::diagnostics::{InternalInvariant, internal_diag, store_error_diag};
use crate::graph::{AttemptState, Engine, ReuseDecision};
use crate::policy::ActionAuthorization;
use crate::state::{
    CompletedAttempt, DurableActionProvenance, DurableActionRequest, DurableAttempt,
    DurableAttemptId, DurableAttemptState, DurableComputation, DurableComputationKey,
    DurableDependency, DurableObservationProvenance, DurableObservationRequest, DurableProvenance,
    DurableReuseDecision,
};

impl Engine {
    /// Revalidate a completed arena node, including asynchronous observation
    /// attestations, under one run's policy, environment, and bound.
    ///
    /// # Errors
    /// Returns diagnostics when durable state cannot be read, a retained
    /// record violates an invariant, or an observer cannot attest its subject.
    pub async fn durable_reuse_is_valid_run(
        &self,
        computation: ComputationId,
        context: &ReuseContext<'_>,
        bound: &crate::RunBound,
    ) -> PithResult<bool> {
        let Some(attempt) = self.durable_record(computation)? else {
            return Ok(false);
        };
        let DurableAttemptState::Complete(completion) = &attempt.state else {
            return Ok(false);
        };
        self.durable_completion_is_valid_run(completion, context, bound)
            .await
    }

    pub(super) async fn durable_completion_is_valid_run(
        &self,
        completion: &CompletedAttempt,
        context: &ReuseContext<'_>,
        bound: &crate::RunBound,
    ) -> PithResult<bool> {
        let mut walk = RecordWalk::default();
        if !self
            .durable_edges_are_valid_run(&completion.dependencies, context, bound, &mut walk)
            .await?
        {
            return Ok(false);
        }
        while let Some(attempt) = walk.frontier.pop() {
            let DurableAttemptState::Complete(completion) = &attempt.state else {
                return Err(internal_diag(
                    InternalInvariant::DurableDependencyAttemptNotComplete,
                ));
            };
            if !self
                .durable_edges_are_valid_run(&completion.dependencies, context, bound, &mut walk)
                .await?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn durable_edges_are_valid_run(
        &self,
        dependencies: &[DurableDependency],
        context: &ReuseContext<'_>,
        bound: &crate::RunBound,
        walk: &mut RecordWalk,
    ) -> PithResult<bool> {
        for dependency in dependencies {
            let valid = match dependency {
                DurableDependency::Pure {
                    computation,
                    attempt,
                } => self.durable_pure_dependency_is_valid(*computation, *attempt, walk)?,
                DurableDependency::Action { attempt } => {
                    self.durable_action_dependency_is_valid(*attempt, context)?
                }
                DurableDependency::Observation { attempt } => {
                    self.durable_observation_dependency_is_valid(*attempt, bound)
                        .await?
                }
                DurableDependency::Blob { .. } | DurableDependency::CapabilityUse { .. } => true,
            };
            if !valid {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn durable_observation_dependency_is_valid(
        &self,
        recorded: DurableAttemptId,
        bound: &crate::RunBound,
    ) -> PithResult<bool> {
        let Some(observer) = self.observer.as_deref() else {
            return Ok(false);
        };
        let Some(attempt) = self.state_store.attempt(recorded).map_err(read_failed)? else {
            return Ok(false);
        };
        let DurableAttemptState::Complete(completion) = &attempt.state else {
            return Ok(false);
        };
        if completion.reuse != DurableReuseDecision::Reusable {
            return Ok(false);
        }
        let DurableComputation::Observation {
            request, subject, ..
        } = &attempt.computation
        else {
            return Err(internal_diag(
                InternalInvariant::DurableObservationEdgeTargetNotObservation,
            ));
        };
        let DurableProvenance::Observation(DurableObservationProvenance::Observed {
            observer: recorded_observer,
            revision,
        }) = &completion.provenance
        else {
            return Err(internal_diag(
                InternalInvariant::CompletedObservationMissingRevision,
            ));
        };
        if observer.identity().observer != *recorded_observer {
            return Ok(false);
        }

        let request = recorded_observation_request(request)?;
        let subject = subject.decode().map_err(|error| {
            internal_diag(InternalInvariant::RecordedObservationSubjectUndecodable(
                error,
            ))
        })?;
        let Ok(rule) = self
            .observation
            .rules
            .select(&request)
            .into_result(&request, &self.observation.rules)
        else {
            return Ok(false);
        };
        let Some(body) = self.observation.bodies.get(&rule) else {
            return Err(internal_diag(
                InternalInvariant::SelectedObservationRuleHasNoBody,
            ));
        };
        let Ok(current_subject) = body.subject(&request.inputs) else {
            return Ok(false);
        };
        let Some(metadata) = self.observation.rules.get(rule) else {
            return Err(internal_diag(
                InternalInvariant::SelectedObservationRuleHasNoMetadata,
            ));
        };
        let current_key = ObservationComputationKey::new(metadata, &request, &current_subject);
        if attempt.computation.key() != DurableComputationKey::Observation(current_key)
            || current_subject != subject
        {
            return Ok(false);
        }
        let revision = revision.decode().map_err(|error| {
            internal_diag(InternalInvariant::RecordedObservationRevisionUndecodable(
                error,
            ))
        })?;
        Ok(observer.attest(&subject, bound).await? == revision)
    }

    /// Revalidate the durable record of a completed arena node against engine
    /// state.
    ///
    /// # Errors
    /// Returns an internal-invariant diagnostic when engine state cannot be
    /// read or a recorded dependency contradicts publication invariants.
    pub fn durable_reuse_is_valid(
        &self,
        computation: ComputationId,
        context: &ReuseContext<'_>,
    ) -> PithResult<bool> {
        let Some(attempt) = self.durable_record(computation)? else {
            return Ok(false);
        };
        let DurableAttemptState::Complete(completion) = &attempt.state else {
            return Ok(false);
        };
        self.durable_completion_is_valid(completion, context)
    }

    /// The durable attempt backing an arena computation, when the side table
    /// and the store agree one exists.
    fn durable_record(
        &self,
        computation: ComputationId,
    ) -> PithResult<Option<Arc<DurableAttempt>>> {
        let Some(attempt_id) = self.durable_attempts.get(&computation).copied() else {
            return Ok(None);
        };
        self.state_store.attempt(attempt_id).map_err(read_failed)
    }

    /// Revalidate a completed record against engine state, and everything the
    /// record reaches.
    ///
    /// Checking the immediate edges is not enough. An edge revalidates on its
    /// own terms and stops, so a dependency two levels down whose rule was
    /// revised would leave the edge above it intact; the walk descends so the
    /// consumer never hydrates a result derived from a rule body this engine
    /// does not have.
    pub(super) fn durable_completion_is_valid(
        &self,
        completion: &CompletedAttempt,
        context: &ReuseContext<'_>,
    ) -> PithResult<bool> {
        let mut walk = RecordWalk::default();
        if !self.durable_edges_are_valid(&completion.dependencies, context, &mut walk)? {
            return Ok(false);
        }
        while let Some(attempt) = walk.frontier.pop() {
            let DurableAttemptState::Complete(completion) = &attempt.state else {
                return Err(internal_diag(
                    InternalInvariant::DurableDependencyAttemptNotComplete,
                ));
            };
            if !self.durable_edges_are_valid(&completion.dependencies, context, &mut walk)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn durable_edges_are_valid(
        &self,
        dependencies: &[DurableDependency],
        context: &ReuseContext<'_>,
        walk: &mut RecordWalk,
    ) -> PithResult<bool> {
        for dependency in dependencies {
            if !self.durable_dependency_is_valid(dependency, context, walk)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn durable_dependency_is_valid(
        &self,
        dependency: &DurableDependency,
        context: &ReuseContext<'_>,
        walk: &mut RecordWalk,
    ) -> PithResult<bool> {
        match dependency {
            DurableDependency::Pure {
                computation,
                attempt,
            } => self.durable_pure_dependency_is_valid(*computation, *attempt, walk),
            DurableDependency::Action { attempt } => {
                self.durable_action_dependency_is_valid(*attempt, context)
            }
            // Observation admission requires an async observer call. The
            // synchronous path is used only by `evaluate_pure`; a run uses the
            // async mirror and can re-attest this edge.
            DurableDependency::Observation { .. } => Ok(false),
            DurableDependency::Blob { .. } | DurableDependency::CapabilityUse { .. } => Ok(true),
        }
    }

    /// Revalidate an action edge by re-selecting and re-planning it. Nothing is
    /// added to the walk: blob and action edges are recorded
    /// on the pure computation that requested them, so an action attempt's own
    /// recorded dependencies are the capability uses its executor reported.
    fn durable_action_dependency_is_valid(
        &self,
        recorded: DurableAttemptId,
        context: &ReuseContext<'_>,
    ) -> PithResult<bool> {
        let Some((policy, environment)) = context.run() else {
            return Ok(false);
        };
        let Some(attempt) = self.state_store.attempt(recorded).map_err(read_failed)? else {
            return Ok(false);
        };
        let DurableAttemptState::Complete(completion) = &attempt.state else {
            return Ok(false);
        };
        let DurableComputation::Action { request, .. } = &attempt.computation else {
            return Err(internal_diag(
                InternalInvariant::DurableActionEdgeTargetNotAction,
            ));
        };
        let DurableComputationKey::Action(recorded_key) = attempt.computation.key() else {
            return Err(internal_diag(
                InternalInvariant::DurableActionEdgeTargetNotAction,
            ));
        };

        let request = recorded_action_request(request)?;
        let Ok(plan) = self.plan_action(&request) else {
            return Ok(false);
        };
        if !matches!(policy.authorize(&plan), ActionAuthorization::Allowed { .. }) {
            return Ok(false);
        }
        let key = self.action_computation_key(plan.rule, &request, plan.spec_digest)?;
        if key == recorded_key {
            return self.recorded_execution_is_reusable(completion, environment);
        }

        let Some(latest) = self
            .state_store
            .latest_completed_reusable_action_attempt(key)
            .map_err(read_failed)?
        else {
            return Ok(false);
        };
        let DurableAttemptState::Complete(latest_completion) = &latest.state else {
            return Err(internal_diag(
                InternalInvariant::ReusableIndexEntryNotComplete,
            ));
        };
        if latest_completion.result != completion.result {
            return Ok(false);
        }
        self.recorded_execution_is_reusable(latest_completion, environment)
    }

    fn recorded_execution_is_reusable(
        &self,
        completion: &CompletedAttempt,
        environment: &ExecutorIdentity,
    ) -> PithResult<bool> {
        let DurableProvenance::Action(DurableActionProvenance::Imported { imported_report }) =
            &completion.provenance
        else {
            return Err(internal_diag(
                InternalInvariant::CompletedActionMissingImportedReport,
            ));
        };
        Ok(
            self.execution_matches_environment(imported_report, environment)
                && self.execution_is_admissible(imported_report)?,
        )
    }

    fn durable_pure_dependency_is_valid(
        &self,
        computation: PureComputationKey,
        recorded: DurableAttemptId,
        walk: &mut RecordWalk,
    ) -> PithResult<bool> {
        // The recorded key names the revision the dependency was computed
        // under. Checking only that the recorded key's latest reusable attempt
        // is unchanged cannot see a revision that moved: a revised rule mints
        // a new key, leaving the old key's attempt still latest under it, so
        // the edge would revalidate and the consumer would hydrate a result
        // derived from a rule body this engine does not have.
        if !self.pure_rule_is_registered_at(computation.rule_identity, computation.rule_revision) {
            return Ok(false);
        }
        let Some(latest) = self.latest_reusable_attempt(computation)? else {
            return Ok(false);
        };
        let latest_completion = reusable_index_completion(&latest, computation)?;
        if latest.id != recorded {
            let Some(recorded_attempt) = self.state_store.attempt(recorded).map_err(read_failed)?
            else {
                return Ok(false);
            };
            let DurableAttemptState::Complete(recorded_completion) = &recorded_attempt.state else {
                return Err(internal_diag(
                    InternalInvariant::DurableDependencyAttemptNotComplete,
                ));
            };
            if latest_completion.result != recorded_completion.result {
                return Ok(false);
            }
        }
        if self.run_holds_attempt(computation, latest.id) {
            return Ok(true);
        }
        // Descend through the attempt accepted as current, never the superseded
        // one the edge recorded. They differ exactly when a dependency was
        // recomputed to an equal result; the superseded record's own edges may
        // name results the current rule set has replaced, and refusing on
        // those would throw away the early cutoff just established.
        walk.enqueue(latest);
        Ok(true)
    }

    /// Whether the attempt an edge accepted is one this run already holds live
    /// and reusable in the arena. Such an attempt was established when it
    /// entered the arena, computed here or reused and hydrated through this
    /// check, so descending into its record would re-derive an answer the run
    /// already has, making the walk quadratic rather than linear.
    ///
    /// The short-circuit holds because within one run the durable state an
    /// arena node was established against moves only by this engine's own
    /// publications; the walk can stop at the live frontier.
    fn run_holds_attempt(
        &self,
        computation: PureComputationKey,
        attempt: DurableAttemptId,
    ) -> bool {
        let Some(node) = self.pure.computations.get(&computation).copied() else {
            return false;
        };
        if self.durable_attempts.get(&node) != Some(&attempt) {
            return false;
        }
        matches!(
            self.computations.get(node).map(|node| &node.state),
            Some(AttemptState::Complete {
                reuse: ReuseDecision::Reusable,
                ..
            })
        )
    }

    pub(super) fn execution_matches_environment(
        &self,
        report: &ExecutionReport,
        environment: &ExecutorIdentity,
    ) -> bool {
        report.executor == environment.executor && report.platform == environment.platform
    }

    pub(super) fn execution_is_admissible(&self, report: &ExecutionReport) -> PithResult<bool> {
        if !report.access.satisfies(self.minimum_access_verification()) {
            return Ok(false);
        }
        for output in &report.outputs {
            let present = match &output.content {
                Content::Blob(id) => self
                    .store
                    .get_blob(*id)
                    .map_err(store_error_diag)?
                    .is_some(),
                Content::Tree(id) => self
                    .store
                    .get_tree(*id)
                    .map_err(store_error_diag)?
                    .is_some(),
            };
            if !present {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// The state one revalidation pass carries while it walks the record beneath a
/// completion.
///
/// An explicit frontier rather than recursion: the recorded graph is as deep as
/// the build is, and the evaluator itself is a stack machine for the same
/// reason. `seen` keeps a diamond from being walked once per path and bounds
/// the pass if a store ever hands back a cyclic record; an attempt enters it
/// when enqueued rather than when validated, which is sound because a failure
/// anywhere abandons the whole pass.
#[derive(Default)]
struct RecordWalk {
    seen: IndexSet<DurableAttemptId>,
    frontier: Vec<Arc<DurableAttempt>>,
}

impl RecordWalk {
    fn enqueue(&mut self, attempt: Arc<DurableAttempt>) {
        if self.seen.insert(attempt.id) {
            self.frontier.push(attempt);
        }
    }
}

fn recorded_action_request(recorded: &DurableActionRequest) -> PithResult<Request<Action>> {
    let inputs = recorded.decoded_inputs().map_err(|error| {
        internal_diag(InternalInvariant::RecordedActionRequestUndecodable(error))
    })?;
    Ok(Request::new(
        "",
        recorded.interface.clone(),
        inputs,
        Span::none(),
    ))
}

fn recorded_observation_request(
    recorded: &DurableObservationRequest,
) -> PithResult<Request<Observation>> {
    let inputs = recorded.decoded_inputs().map_err(|error| {
        internal_diag(InternalInvariant::RecordedObservationRequestUndecodable(
            error,
        ))
    })?;
    Ok(Request::new(
        "",
        recorded.interface.clone(),
        inputs,
        Span::none(),
    ))
}

pub(super) fn reusable_index_completion(
    attempt: &DurableAttempt,
    computation: PureComputationKey,
) -> PithResult<&CompletedAttempt> {
    if attempt.computation.key() != DurableComputationKey::Pure(computation) {
        return Err(internal_diag(
            InternalInvariant::ReusableIndexEntryKeyMismatch,
        ));
    }
    let DurableAttemptState::Complete(completion) = &attempt.state else {
        return Err(internal_diag(
            InternalInvariant::ReusableIndexEntryNotComplete,
        ));
    };
    if completion.reuse != DurableReuseDecision::Reusable {
        return Err(internal_diag(
            InternalInvariant::ReusableIndexEntryNotReusable,
        ));
    }
    Ok(completion)
}
