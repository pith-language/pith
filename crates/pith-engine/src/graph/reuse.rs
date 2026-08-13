//! Reuse of completed pure computations, from this engine instance's arena and
//! from durable engine state (decision 0024).
//!
//! Two sources answer "has this computation already been done": the arena index
//! of completed nodes, and the reusable-attempt index in [`EngineStateStore`].
//! Both are gated by the same dependency revalidation, so an arena hit and a
//! hydrated hit accept exactly the same set of results.
//!
//! [`EngineStateStore`]: crate::state::EngineStateStore

use std::sync::Arc;

use indexmap::IndexMap;
use pith_core::{
    Action, ActionComputationKey, Content, Pure, PureComputationKey, Request, RuleId,
};
use pith_diag::{DiagnosticSink, PithResult};
use pith_ids::ComputationId;
use smallvec::SmallVec;

use super::{
    ActionPlan, ActionRecord, AttemptState, ComputationKind, ComputationNode, DependencyEdge,
    Engine, Evaluation, EvaluationSource, ReuseDecision, ReuseReason,
};
use crate::action::{ExecutionReport, ExecutorIdentity};
use crate::graph::capabilities::canonical_capabilities;
use crate::graph::diagnostics::{InternalInvariant, internal_diag, store_error_diag};
use crate::policy::ActionAuthorization;
use crate::state::{
    CompletedAttempt, DurableActionProvenance, DurableAttempt, DurableAttemptId,
    DurableAttemptState, DurableDependency, DurableProvenance, DurableReuseDecision,
};

impl Engine {
    /// Resolve `request` from an already-completed computation instead of
    /// running its rule body. Sources are tried in order: a completed node in
    /// this instance's arena, then a revalidated attempt in engine state.
    ///
    /// `Ok(None)` means no reusable result was available and the caller must
    /// evaluate. Adapter failures and records that contradict the store's own
    /// invariants are errors, never silent misses: decision 0024 treats
    /// corruption as an adapter error rather than a cache miss.
    ///
    /// # Errors
    /// `E-1207` (internal invariant) when engine state cannot be read or the
    /// record it returned is inconsistent with the index that produced it.
    pub(super) fn reusable_pure_evaluation(
        &mut self,
        rule: RuleId,
        request: &Request<Pure>,
    ) -> PithResult<Option<Evaluation>> {
        let Some(rule_metadata) = self.rules.get(rule) else {
            return Err(internal_diag(InternalInvariant::SelectedRuleHasNoMetadata));
        };
        let key = PureComputationKey::new(rule_metadata, request);
        if let Some(evaluation) = self.live_pure_reuse(key)? {
            return Ok(Some(evaluation));
        }
        self.hydrate_pure_computation(key, rule, request)
    }

    /// Reuse a completed node already in this instance's arena.
    fn live_pure_reuse(&self, key: PureComputationKey) -> PithResult<Option<Evaluation>> {
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
        // The arena says reusable. Decision 0024 ("dynamic dependencies and
        // revalidation") also requires confirming the durable record's
        // dependency set has not drifted.
        if !self.durable_reuse_is_valid(computation)? {
            return Ok(None);
        }
        Ok(Some(Evaluation {
            value: result,
            computation,
            source: EvaluationSource::Reused,
        }))
    }

    /// Load a completed attempt recorded by an earlier engine instance into a
    /// fresh arena node (decision 0024: "loading a graph creates fresh arena
    /// nodes and maintains an internal mapping from durable digests to local
    /// handles").
    ///
    /// The hydrated node arrives terminal. No new durable attempt is created
    /// for it; it is mapped onto the attempt it was loaded from, so a later
    /// computation that depends on it publishes an edge naming that original
    /// attempt rather than a duplicate of it.
    fn hydrate_pure_computation(
        &mut self,
        key: PureComputationKey,
        rule: RuleId,
        request: &Request<Pure>,
    ) -> PithResult<Option<Evaluation>> {
        let Some(attempt) = self.latest_reusable_attempt(key)? else {
            return Ok(None);
        };
        let completion = reusable_index_completion(&attempt, key)?;
        if !self.durable_completion_is_valid(completion)? {
            // A recorded dependency drifted, so the consumer is dirty and must
            // be re-evaluated. An ordinary miss, not a fault.
            return Ok(None);
        }
        let value = completion
            .result
            .decode()
            .map_err(|error| internal_diag(InternalInvariant::HydratedResultUndecodable(error)))?;
        // The computation key covers the requested interface, so a result of
        // another type contradicts the key this attempt was indexed under.
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
            // A hydrated node has no arena subgraph: nothing was evaluated
            // here, and a durable pure edge records a computation key rather
            // than the `Request` a child node would need. The recorded set
            // stays authoritative on the durable attempt, which
            // `EngineQuery::durable_attempt_of` exposes.
            dependencies: SmallVec::new(),
            state: AttemptState::Complete {
                result: value.clone(),
                reuse: ReuseDecision::Reusable,
            },
            action: None,
            // A reusable pure attempt has neither action nor capability-use
            // dependencies — `durable_dependency_is_valid` rejects both — so
            // its effective capability requirements are empty by induction over
            // the reusable subtree.
            capabilities: Box::new([]),
        });
        self.index_pure_computation(key, computation);
        self.durable_attempts.insert(computation, attempt.id);
        Ok(Some(Evaluation {
            value,
            computation,
            source: EvaluationSource::Hydrated,
        }))
    }

    /// Revalidate the durable record of a completed arena node against engine
    /// state (decision 0024).
    ///
    /// `Ok(false)` answers "this node is not currently valid for reuse", which
    /// includes a node that was never published, whose attempt is no longer
    /// retained, or whose attempt is not complete. Those are answers, not
    /// faults; only a broken adapter or a record that contradicts the store's
    /// own guarantees produces an error.
    ///
    /// # Errors
    /// `E-1207` (internal invariant) when engine state cannot be read or a
    /// recorded dependency contradicts the store's publication invariants.
    pub fn durable_reuse_is_valid(&self, computation: ComputationId) -> PithResult<bool> {
        let Some(attempt_id) = self.durable_attempts.get(&computation).copied() else {
            return Ok(false);
        };
        let Some(attempt) = self.state_store.attempt(attempt_id).map_err(read_failed)? else {
            return Ok(false);
        };
        let DurableAttemptState::Complete(completion) = &attempt.state else {
            return Ok(false);
        };
        self.durable_completion_is_valid(completion)
    }

    /// Revalidate one completed attempt's recorded dependency set.
    fn durable_completion_is_valid(&self, completion: &CompletedAttempt) -> PithResult<bool> {
        for dependency in &completion.dependencies {
            if !self.durable_dependency_is_valid(dependency)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn durable_dependency_is_valid(&self, dependency: &DurableDependency) -> PithResult<bool> {
        match dependency {
            DurableDependency::Pure {
                computation,
                attempt,
            } => self.durable_pure_dependency_is_valid(*computation, *attempt),
            // Content identity is the dependency: retained bytes that still
            // reproduce a `ContentId` cannot have drifted.
            DurableDependency::Blob { .. } => Ok(true),
            // A record of what an action used. Nothing to revalidate: whether
            // the capability may be exercised again is the current policy's
            // answer, reapplied before any reuse is served.
            DurableDependency::CapabilityUse { .. } => Ok(true),
            // A reusable attempt cannot carry an action edge: a pure attempt
            // that depends on an action is published `EffectfulDependency`
            // (decision 0031), and an action attempt's own edges are capability
            // use. The publication validator enforces both, so reaching here is
            // a fault, not a reason to recompute.
            DurableDependency::Action { .. } => Err(internal_diag(
                InternalInvariant::ReusableAttemptHasEffectfulDependency,
            )),
        }
    }

    /// A `Pure` dependency whose latest reusable attempt is a *different*
    /// attempt with a *different* result (canonical equality) makes the
    /// consumer dirty. A different attempt with an equal result leaves the
    /// consumer reusable — downstream propagation stops there, even though the
    /// newer attempt records the changed upstream provenance.
    fn durable_pure_dependency_is_valid(
        &self,
        computation: PureComputationKey,
        recorded: DurableAttemptId,
    ) -> PithResult<bool> {
        let Some(latest) = self.latest_reusable_attempt(computation)? else {
            // The dependency is no longer available as a reusable result, so
            // the consumer must be re-evaluated.
            return Ok(false);
        };
        if latest.id == recorded {
            return Ok(true);
        }
        let latest_completion = reusable_index_completion(&latest, computation)?;
        let Some(recorded_attempt) = self.state_store.attempt(recorded).map_err(read_failed)?
        else {
            // The recorded attempt is no longer retained, so the previous
            // result cannot be compared. Treat the consumer as dirty.
            return Ok(false);
        };
        let DurableAttemptState::Complete(recorded_completion) = &recorded_attempt.state else {
            return Err(internal_diag(
                InternalInvariant::DurableDependencyAttemptNotComplete,
            ));
        };
        Ok(latest_completion.result == recorded_completion.result)
    }

    fn latest_reusable_attempt(
        &self,
        computation: PureComputationKey,
    ) -> PithResult<Option<Arc<DurableAttempt>>> {
        self.state_store
            .latest_completed_reusable_attempt(computation)
            .map_err(read_failed)
    }

    /// Resolve `request` from a completed action attempt instead of executing
    /// it (decision 0031). The same two sources pure reuse uses, in the same
    /// order, with one extra gate: a recorded execution is admissible only if
    /// it happened in the environment this run would execute in.
    ///
    /// `Ok(None)` means the action must run. It covers a key nothing has been
    /// recorded under, a recorded attempt whose dependencies no longer
    /// revalidate, and a recorded attempt this environment will not accept.
    ///
    /// # Errors
    /// `E-1207` (internal invariant) when engine state or the content store
    /// cannot be read, or a record contradicts the index that produced it.
    pub(super) fn reusable_action_evaluation(
        &mut self,
        key: ActionComputationKey,
        request: &Request<Action>,
        plan: &ActionPlan,
        authorization: &ActionAuthorization,
        environment: &ExecutorIdentity,
    ) -> PithResult<Option<Evaluation>> {
        if !self.action_caching() {
            return Ok(None);
        }
        if let Some(evaluation) = self.live_action_reuse(key, environment)? {
            return Ok(Some(evaluation));
        }
        self.hydrate_action_computation(key, request, plan, authorization, environment)
    }

    fn live_action_reuse(
        &self,
        key: ActionComputationKey,
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
        if !self.durable_reuse_is_valid(computation)? {
            return Ok(None);
        }
        Ok(Some(Evaluation {
            value: result,
            computation,
            source: EvaluationSource::Reused,
        }))
    }

    /// Load a completed action attempt recorded by an earlier engine instance
    /// into a fresh arena node, on the terms decision 0024 set for pure
    /// hydration: the node arrives terminal, mapped onto the attempt it came
    /// from, and allocates no new one.
    ///
    /// The node keeps the contract and authorization this run planned and
    /// obtained. What is reused is the recorded execution; permission is the
    /// current policy's answer, which is reapplied before reuse is served.
    fn hydrate_action_computation(
        &mut self,
        key: ActionComputationKey,
        request: &Request<Action>,
        plan: &ActionPlan,
        authorization: &ActionAuthorization,
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
        // A completed action attempt always retains an imported report; the
        // publication validator refuses one that does not.
        let DurableProvenance::Action(DurableActionProvenance::Imported { imported_report }) =
            &completion.provenance
        else {
            return Err(internal_diag(
                InternalInvariant::CompletedActionMissingImportedReport,
            ));
        };
        if !self.execution_matches_environment(imported_report, environment)
            || !self.execution_is_admissible(imported_report)?
            || !self.durable_completion_is_valid(completion)?
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
                // Nothing executed here, so there is no executor observation
                // from this run. The imported report is the one being reused.
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

    /// Whether a recorded execution belongs to the environment this run would
    /// execute in. A result produced by another executor, or on another
    /// platform, answers a different question.
    fn execution_matches_environment(
        &self,
        report: &ExecutionReport,
        environment: &ExecutorIdentity,
    ) -> bool {
        report.executor == environment.executor && report.platform == environment.platform
    }

    /// Whether a recorded execution is still good enough to serve: its
    /// confinement meets this engine's floor, and the content it produced is
    /// still in the store. Reuse hands back those bytes, so a record whose
    /// content has been collected answers nothing.
    fn execution_is_admissible(&self, report: &ExecutionReport) -> PithResult<bool> {
        if !report.access.satisfies(self.minimum_access_verification()) {
            return Ok(false);
        }
        for output in &report.outputs {
            let present = match &output.content {
                Content::Blob(id) => self.store.get_blob(*id).map_err(store_error_diag)?.is_some(),
                Content::Tree(id) => self.store.get_tree(*id).map_err(store_error_diag)?.is_some(),
            };
            if !present {
                return Ok(false);
            }
        }
        Ok(true)
    }

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

    /// The reuse decision a completed node's dependencies support.
    ///
    /// A dependency that is not itself reusable is reported ahead of a merely
    /// effectful one, since both stop reuse and the first is the more specific
    /// answer. An action edge stops reuse even when the action it names is
    /// reusable, because this node's key does not carry the identity of the
    /// action rule that planned it (decision 0031).
    pub(super) fn reuse_decision(&self, dependencies: &[DependencyEdge]) -> ReuseDecision {
        let mut first_action = None;
        for dependency in dependencies {
            let computation = match dependency {
                DependencyEdge::Blob { .. } | DependencyEdge::CapabilityUse { .. } => continue,
                DependencyEdge::Action { computation, .. } => {
                    first_action = first_action.or(Some(*computation));
                    *computation
                }
                DependencyEdge::Request { computation, .. } => *computation,
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
        match first_action {
            Some(computation) => {
                ReuseDecision::NotReusable(ReuseReason::EffectfulDependency { computation })
            }
            None => ReuseDecision::Reusable,
        }
    }
}

/// Check the reusable index's contract before trusting the record it returned:
/// the attempt belongs to the requested computation, it is complete, and it is
/// marked reusable. Decision 0024 gives the index all three properties, and the
/// memory adapter enforces them on publication. An adapter that returns
/// anything else is faulty, and a faulty adapter must not be read as a miss.
fn reusable_index_completion(
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

fn read_failed(error: crate::state::EngineStateError) -> DiagnosticSink {
    internal_diag(InternalInvariant::EngineStateReadFailed(error))
}

pub(super) type PureComputationIndex = IndexMap<PureComputationKey, ComputationId>;
pub(super) type ActionComputationIndex = IndexMap<ActionComputationKey, ComputationId>;
