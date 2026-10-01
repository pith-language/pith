use pith_core::{Pure, PureComputationKey, Request, RuleId, Value};
use pith_diag::PithResult;
use pith_ids::ComputationId;
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
        let key = self.pure_reuse_key(rule, request)?;
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
        let key = self.pure_reuse_key(rule, request)?;
        if let Some(evaluation) = self.live_pure_reuse(key, context)? {
            return Ok(Some(evaluation));
        }
        self.hydrate_pure_computation(key, rule, request, context)
    }

    /// The reusable-index key for one rule application.
    fn pure_reuse_key(
        &self,
        rule: RuleId,
        request: &Request<Pure>,
    ) -> PithResult<PureComputationKey> {
        let Some(rule_metadata) = self.pure.rules.get(rule) else {
            return Err(internal_diag(InternalInvariant::SelectedRuleHasNoMetadata));
        };
        Ok(PureComputationKey::new(rule_metadata, request))
    }

    fn live_pure_reuse(
        &self,
        key: PureComputationKey,
        context: &ReuseContext<'_>,
    ) -> PithResult<Option<Evaluation>> {
        let Some((computation, result)) = self.reusable_live_node(key) else {
            return Ok(None);
        };
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
        let Some((computation, result)) = self.reusable_live_node(key) else {
            return Ok(None);
        };
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

    /// The completed, reusable arena node a key names, if the live index
    /// holds one.
    fn reusable_live_node(&self, key: PureComputationKey) -> Option<(ComputationId, Value)> {
        let computation = self.pure.computations.get(&key).copied()?;
        let node = self.computations.get(computation)?;
        let AttemptState::Complete { result, reuse } = &node.state else {
            return None;
        };
        if reuse != &ReuseDecision::Reusable {
            return None;
        }
        Some((computation, result.clone()))
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
        self.pure.computations.insert(key, computation);
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
