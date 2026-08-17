use pith_core::{Action, ActionComputationKey, Request};
use pith_diag::PithResult;
use smallvec::SmallVec;

use super::{ReuseContext, read_failed};
use crate::action::ExecutorIdentity;
use crate::graph::capabilities::canonical_capabilities;
use crate::graph::diagnostics::{InternalInvariant, internal_diag};
use crate::graph::{
    ActionPlan, ActionRecord, AttemptState, ComputationKind, ComputationNode, Engine, Evaluation,
    EvaluationSource, ReuseDecision,
};
use crate::policy::ActionAuthorization;
use crate::state::{
    DurableActionProvenance, DurableAttemptState, DurableProvenance, DurableReuseDecision,
};

impl Engine {
    pub(in crate::graph) fn reusable_action_evaluation(
        &mut self,
        key: ActionComputationKey,
        request: &Request<Action>,
        plan: &ActionPlan,
        authorization: &ActionAuthorization,
        context: &ReuseContext<'_>,
        environment: &ExecutorIdentity,
    ) -> PithResult<Option<Evaluation>> {
        if !self.action_caching() {
            return Ok(None);
        }
        if let Some(evaluation) = self.live_action_reuse(key, context, environment)? {
            return Ok(Some(evaluation));
        }
        self.hydrate_action_computation(key, request, plan, authorization, context, environment)
    }

    fn live_action_reuse(
        &self,
        key: ActionComputationKey,
        context: &ReuseContext<'_>,
        environment: &ExecutorIdentity,
    ) -> PithResult<Option<Evaluation>> {
        let Some(computation) = self.action_computations.get(&key).copied() else {
            return Ok(None);
        };
        let Some(node) = self.computations.get(computation) else {
            return Ok(None);
        };
        let AttemptState::Complete { result, reuse } = &node.state else {
            return Ok(None);
        };
        if reuse != &ReuseDecision::Reusable {
            return Ok(None);
        }
        let Some(report) = node
            .action
            .as_ref()
            .and_then(|action| action.imported_report.as_ref())
        else {
            return Ok(None);
        };
        if !self.execution_matches_environment(report, environment)
            || !self.execution_is_admissible(report)?
        {
            return Ok(None);
        }
        let result = result.clone();
        if !self.durable_reuse_is_valid(computation, context)? {
            return Ok(None);
        }
        Ok(Some(Evaluation {
            value: result,
            computation,
            source: EvaluationSource::Reused,
        }))
    }

    fn hydrate_action_computation(
        &mut self,
        key: ActionComputationKey,
        request: &Request<Action>,
        plan: &ActionPlan,
        authorization: &ActionAuthorization,
        context: &ReuseContext<'_>,
        environment: &ExecutorIdentity,
    ) -> PithResult<Option<Evaluation>> {
        let Some(attempt) = self
            .state_store
            .latest_completed_reusable_action_attempt(key)
            .map_err(read_failed)?
        else {
            return Ok(None);
        };
        if attempt.computation.action_key() != Some(key) {
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
        let DurableProvenance::Action(DurableActionProvenance::Imported { imported_report }) =
            &completion.provenance
        else {
            return Err(internal_diag(
                InternalInvariant::CompletedActionMissingImportedReport,
            ));
        };
        if !self.execution_matches_environment(imported_report, environment)
            || !self.execution_is_admissible(imported_report)?
            || !self.durable_completion_is_valid(completion, context)?
        {
            return Ok(None);
        }
        let value = completion
            .result
            .decode()
            .map_err(|error| internal_diag(InternalInvariant::HydratedResultUndecodable(error)))?;
        if !value.is_type(&request.interface.output) {
            return Err(internal_diag(
                InternalInvariant::HydratedResultTypeMismatch {
                    expected: request.interface.output.clone(),
                    actual: value.value_type(),
                },
            ));
        }

        let computation = self.computations.push(ComputationNode {
            kind: ComputationKind::Action(request.clone()),
            rule: plan.rule,
            dependencies: SmallVec::new(),
            state: AttemptState::Complete {
                result: value.clone(),
                reuse: ReuseDecision::Reusable,
            },
            action: Some(ActionRecord {
                key,
                spec_digest: plan.spec_digest,
                spec: plan.spec.clone(),
                authorization: authorization.clone(),
                executor_report: None,
                imported_report: Some(imported_report.clone()),
            }),
            capabilities: canonical_capabilities(&plan.spec.capabilities),
        });
        self.index_action_computation(key, computation);
        self.durable_attempts.insert(computation, attempt.id);
        Ok(Some(Evaluation {
            value,
            computation,
            source: EvaluationSource::Hydrated,
        }))
    }
}
