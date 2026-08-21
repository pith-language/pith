use pith_core::{Pure, PureComputationKey, Request, RuleId};
use pith_diag::PithResult;
use smallvec::SmallVec;

use super::revalidation::reusable_index_completion;
use super::{ReuseContext, read_failed};
use crate::graph::diagnostics::{InternalInvariant, internal_diag};
use crate::graph::{
    AttemptState, ComputationKind, ComputationNode, Engine, Evaluation, EvaluationSource,
    ReuseDecision,
};
use crate::state::DurableAttempt;
use std::sync::Arc;

impl Engine {
    pub(in crate::graph) async fn reusable_pure_evaluation_run(
        &mut self,
        rule: RuleId,
        request: &Request<Pure>,
        context: &ReuseContext<'_>,
        bound: &crate::RunBound,
    ) -> PithResult<Option<Evaluation>> {
        let Some(rule_metadata) = self.rules.get(rule) else {
            return Err(internal_diag(InternalInvariant::SelectedRuleHasNoMetadata));
        };
        let key = PureComputationKey::new(rule_metadata, request);
        if let Some(evaluation) = self.live_pure_reuse_run(key, context, bound).await? {
            return Ok(Some(evaluation));
        }
        self.hydrate_pure_computation_run(key, rule, request, context, bound)
            .await
    }

    pub(in crate::graph) fn reusable_pure_evaluation(
        &mut self,
        rule: RuleId,
        request: &Request<Pure>,
        context: &ReuseContext<'_>,
    ) -> PithResult<Option<Evaluation>> {
        let Some(rule_metadata) = self.rules.get(rule) else {
            return Err(internal_diag(InternalInvariant::SelectedRuleHasNoMetadata));
        };
        let key = PureComputationKey::new(rule_metadata, request);
        if let Some(evaluation) = self.live_pure_reuse(key, context)? {
            return Ok(Some(evaluation));
        }
        self.hydrate_pure_computation(key, rule, request, context)
    }

    fn live_pure_reuse(
        &self,
        key: PureComputationKey,
        context: &ReuseContext<'_>,
    ) -> PithResult<Option<Evaluation>> {
        let Some(computation) = self.pure_computations.get(&key).copied() else {
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

    async fn live_pure_reuse_run(
        &self,
        key: PureComputationKey,
        context: &ReuseContext<'_>,
        bound: &crate::RunBound,
    ) -> PithResult<Option<Evaluation>> {
        let Some(computation) = self.pure_computations.get(&key).copied() else {
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
        let result = result.clone();
        if !self
            .durable_reuse_is_valid_run(computation, context, bound)
            .await?
        {
            return Ok(None);
        }
        Ok(Some(Evaluation {
            value: result,
            computation,
            source: EvaluationSource::Reused,
        }))
    }

    fn hydrate_pure_computation(
        &mut self,
        key: PureComputationKey,
        rule: RuleId,
        request: &Request<Pure>,
        context: &ReuseContext<'_>,
    ) -> PithResult<Option<Evaluation>> {
        let Some(attempt) = self.latest_reusable_attempt(key)? else {
            return Ok(None);
        };
        let completion = reusable_index_completion(&attempt, key)?;
        if !self.durable_completion_is_valid(completion, context)? {
            return Ok(None);
        }
        self.install_hydrated_pure(key, rule, request, &attempt, completion)
            .map(Some)
    }

    async fn hydrate_pure_computation_run(
        &mut self,
        key: PureComputationKey,
        rule: RuleId,
        request: &Request<Pure>,
        context: &ReuseContext<'_>,
        bound: &crate::RunBound,
    ) -> PithResult<Option<Evaluation>> {
        let Some(attempt) = self.latest_reusable_attempt(key)? else {
            return Ok(None);
        };
        let completion = reusable_index_completion(&attempt, key)?;
        if !self
            .durable_completion_is_valid_run(completion, context, bound)
            .await?
        {
            return Ok(None);
        }
        self.install_hydrated_pure(key, rule, request, &attempt, completion)
            .map(Some)
    }

    fn install_hydrated_pure(
        &mut self,
        key: PureComputationKey,
        rule: RuleId,
        request: &Request<Pure>,
        attempt: &DurableAttempt,
        completion: &crate::state::CompletedAttempt,
    ) -> PithResult<Evaluation> {
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
            kind: ComputationKind::Pure(request.clone()),
            rule,
            dependencies: SmallVec::new(),
            state: AttemptState::Complete {
                result: value.clone(),
                reuse: ReuseDecision::Reusable,
            },
            action: None,
            observation: None,
            capabilities: completion.capabilities.clone(),
        });
        self.index_pure_computation(key, computation);
        self.durable_attempts.insert(computation, attempt.id);
        Ok(Evaluation {
            value,
            computation,
            source: EvaluationSource::Hydrated,
        })
    }

    pub(super) fn latest_reusable_attempt(
        &self,
        computation: PureComputationKey,
    ) -> PithResult<Option<Arc<DurableAttempt>>> {
        self.state_store
            .latest_completed_reusable_attempt(computation)
            .map_err(read_failed)
    }
}
