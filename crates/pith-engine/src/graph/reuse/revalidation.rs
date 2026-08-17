use pith_core::{Action, Content, PureComputationKey, Request};
use pith_diag::{PithResult, Span};
use pith_ids::ComputationId;

use super::{ReuseContext, read_failed};
use crate::action::{ExecutionReport, ExecutorIdentity};
use crate::graph::Engine;
use crate::graph::diagnostics::{InternalInvariant, internal_diag, store_error_diag};
use crate::policy::ActionAuthorization;
use crate::state::{
    CompletedAttempt, DurableActionProvenance, DurableActionRequest, DurableAttempt,
    DurableAttemptId, DurableAttemptState, DurableComputation, DurableDependency,
    DurableProvenance, DurableReuseDecision,
};

impl Engine {
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
        let Some(attempt_id) = self.durable_attempts.get(&computation).copied() else {
            return Ok(false);
        };
        let Some(attempt) = self.state_store.attempt(attempt_id).map_err(read_failed)? else {
            return Ok(false);
        };
        let DurableAttemptState::Complete(completion) = &attempt.state else {
            return Ok(false);
        };
        self.durable_completion_is_valid(completion, context)
    }

    pub(super) fn durable_completion_is_valid(
        &self,
        completion: &CompletedAttempt,
        context: &ReuseContext<'_>,
    ) -> PithResult<bool> {
        for dependency in &completion.dependencies {
            if !self.durable_dependency_is_valid(dependency, context)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn durable_dependency_is_valid(
        &self,
        dependency: &DurableDependency,
        context: &ReuseContext<'_>,
    ) -> PithResult<bool> {
        match dependency {
            DurableDependency::Pure {
                computation,
                attempt,
            } => self.durable_pure_dependency_is_valid(*computation, *attempt),
            DurableDependency::Action { attempt } => {
                self.durable_action_dependency_is_valid(*attempt, context)
            }
            DurableDependency::Blob { .. } | DurableDependency::CapabilityUse { .. } => Ok(true),
        }
    }

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
        let Some(recorded_key) = attempt.computation.action_key() else {
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
    ) -> PithResult<bool> {
        // The recorded key names the revision the dependency was computed
        // under. Asking only whether that key's latest reusable attempt is
        // still the recorded one cannot see a revision that moved: a revised
        // rule mints a *new* key, which leaves the old key's attempt
        // undisturbed and still latest under it, so the edge would revalidate
        // and the consumer would hydrate a result derived from a rule body
        // this engine no longer has (decision 0049).
        if !self.pure_rule_is_registered_at(computation.rule_identity, computation.rule_revision) {
            return Ok(false);
        }
        let Some(latest) = self.latest_reusable_attempt(computation)? else {
            return Ok(false);
        };
        if latest.id == recorded {
            return Ok(true);
        }
        let latest_completion = reusable_index_completion(&latest, computation)?;
        let Some(recorded_attempt) = self.state_store.attempt(recorded).map_err(read_failed)?
        else {
            return Ok(false);
        };
        let DurableAttemptState::Complete(recorded_completion) = &recorded_attempt.state else {
            return Err(internal_diag(
                InternalInvariant::DurableDependencyAttemptNotComplete,
            ));
        };
        Ok(latest_completion.result == recorded_completion.result)
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

pub(super) fn reusable_index_completion(
    attempt: &DurableAttempt,
    computation: PureComputationKey,
) -> PithResult<&CompletedAttempt> {
    if attempt.computation.pure_key() != Some(computation) {
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
