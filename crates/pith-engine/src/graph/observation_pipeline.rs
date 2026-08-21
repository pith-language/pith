//! Selection, observation, freshness admission, and provenance publication.

use pith_core::{Observation, ObservationComputationKey, Request, RuleId, Value};
use pith_diag::{Diag, DiagnosticSink, EngineCode, PithResult};
use pith_ids::ComputationId;
use smallvec::SmallVec;

use super::diagnostics::{InternalInvariant, internal_diag, observer_missing_diag, one_diag};
use super::{
    AttemptState, ComputationKind, ComputationNode, Engine, ObservationRecord, ReuseDecision,
};
use crate::state::{DurableComputation, DurableObservationRequest, DurableRule, EncodedValue};

pub(super) struct ObservationServing {
    pub(super) computation: ComputationId,
    pub(super) value: Value,
}

struct PreparedObservation {
    rule: RuleId,
    subject: Value,
    key: ObservationComputationKey,
}

impl Engine {
    pub(super) async fn serve_observation(
        &mut self,
        request: &Request<Observation>,
        bound: &crate::RunBound,
    ) -> PithResult<ObservationServing> {
        let prepared = self.prepare_observation(request)?;
        let identity = self
            .observer
            .as_deref()
            .ok_or_else(|| observer_missing_diag(request.span))?
            .identity();

        if let Some(serving) = self
            .admit_live_observation(prepared.key, &identity, bound)
            .await?
        {
            return Ok(serving);
        }

        let computation = self.allocate_observation(request, &prepared);
        if let Err(diagnostics) =
            self.persist_pending_observation(computation, request, &prepared, &identity)
        {
            self.fail_observation_orphan(computation, &diagnostics);
            return Err(diagnostics);
        }

        let observed = match self
            .observer
            .as_deref()
            .ok_or_else(|| observer_missing_diag(request.span))?
            .observe(&prepared.subject, bound)
            .await
        {
            Ok(observed) => observed,
            Err(diagnostics) => {
                self.fail_observation(computation, &identity, &diagnostics, None)?;
                return Err(diagnostics);
            }
        };

        if !observed.value.is_type(&request.interface.output) {
            let diagnostics = one_diag(Diag::engine(
                EngineCode::ResultTypeMismatch,
                request.span,
                format!(
                    "observation returned {}, expected {}",
                    observed.value.value_type(),
                    request.interface.output
                ),
            ));
            self.fail_observation(
                computation,
                &identity,
                &diagnostics,
                Some(&observed.revision),
            )?;
            return Err(diagnostics);
        }

        self.complete_observation(
            computation,
            prepared,
            identity,
            observed.value,
            observed.revision,
        )
    }

    fn prepare_observation(
        &self,
        request: &Request<Observation>,
    ) -> PithResult<PreparedObservation> {
        request.validate_inputs().map_err(one_diag)?;
        let rule = self
            .observation_rules
            .select(request)
            .into_result(request, &self.observation_rules)
            .map_err(one_diag)?;
        let Some(body) = self.observation_bodies.get(&rule) else {
            return Err(internal_diag(
                InternalInvariant::SelectedObservationRuleHasNoBody,
            ));
        };
        let subject = body.subject(&request.inputs)?;
        let Some(metadata) = self.observation_rules.get(rule) else {
            return Err(internal_diag(
                InternalInvariant::SelectedObservationRuleHasNoMetadata,
            ));
        };
        let key = ObservationComputationKey::new(metadata, request, &subject);
        Ok(PreparedObservation { rule, subject, key })
    }

    async fn admit_live_observation(
        &self,
        key: ObservationComputationKey,
        identity: &crate::ObserverIdentity,
        bound: &crate::RunBound,
    ) -> PithResult<Option<ObservationServing>> {
        let Some(computation) = self.observation_computations.get(&key).copied() else {
            return Ok(None);
        };
        let Some(node) = self.computations.get(computation) else {
            return Ok(None);
        };
        let AttemptState::Complete {
            result,
            reuse: ReuseDecision::Reusable,
        } = &node.state
        else {
            return Ok(None);
        };
        let Some(record) = node.observation.as_ref() else {
            return Err(internal_diag(
                InternalInvariant::ObservationLostObservationRecord,
            ));
        };
        if &record.observer != identity {
            return Ok(None);
        }
        let current = self
            .observer
            .as_deref()
            .ok_or_else(|| observer_missing_diag(pith_diag::Span::none()))?
            .attest(&record.subject, bound)
            .await?;
        if current != record.revision {
            return Ok(None);
        }
        Ok(Some(ObservationServing {
            computation,
            value: result.clone(),
        }))
    }

    fn allocate_observation(
        &mut self,
        request: &Request<Observation>,
        prepared: &PreparedObservation,
    ) -> ComputationId {
        self.computations.push(ComputationNode {
            kind: ComputationKind::Observation(request.clone()),
            rule: prepared.rule,
            dependencies: SmallVec::new(),
            state: AttemptState::Pending,
            action: None,
            observation: None,
            capabilities: Box::new([]),
        })
    }

    fn persist_pending_observation(
        &mut self,
        computation: ComputationId,
        request: &Request<Observation>,
        prepared: &PreparedObservation,
        observer: &crate::ObserverIdentity,
    ) -> PithResult<()> {
        let Some(rule) = self.observation_rules.get(prepared.rule) else {
            return Err(internal_diag(
                InternalInvariant::SelectedObservationRuleHasNoMetadata,
            ));
        };
        let durable = DurableComputation::Observation {
            computation_digest: prepared.key.digest,
            request: durable_observation_request(request),
            rule: DurableRule::new(rule.revision),
            subject: EncodedValue::from_value(&prepared.subject),
            observer: observer.observer.clone(),
        };
        self.create_pending_effect_attempt(computation, durable)
    }

    fn complete_observation(
        &mut self,
        computation: ComputationId,
        prepared: PreparedObservation,
        observer: crate::ObserverIdentity,
        value: Value,
        revision: Value,
    ) -> PithResult<ObservationServing> {
        let Some(node) = self.computations.get_mut(computation) else {
            return Err(internal_diag(
                InternalInvariant::ObservationLostComputationNode,
            ));
        };
        node.observation = Some(ObservationRecord {
            key: prepared.key,
            subject: prepared.subject,
            observer,
            revision,
        });
        node.state = AttemptState::Complete {
            result: value.clone(),
            reuse: ReuseDecision::Reusable,
        };
        self.observation_computations
            .insert(prepared.key, computation);
        self.publish_observation_completion(computation)?;
        Ok(ObservationServing { computation, value })
    }

    fn fail_observation_orphan(
        &mut self,
        computation: ComputationId,
        diagnostics: &DiagnosticSink,
    ) {
        if let Some(node) = self.computations.get_mut(computation) {
            node.state = AttemptState::Failed {
                diagnostics: diagnostics.iter().cloned().collect(),
            };
        }
    }
}

fn durable_observation_request(request: &Request<Observation>) -> DurableObservationRequest {
    DurableObservationRequest {
        interface: request.interface.clone(),
        inputs: request
            .inputs
            .iter()
            .map(EncodedValue::from_value)
            .collect(),
    }
}
