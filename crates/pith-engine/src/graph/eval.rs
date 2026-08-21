//! The synchronous core: frame lifecycle and the pure steps (decision 0022).
//!
//! Nothing here awaits. [`Engine::advance_chain`] is the only place the step
//! machine is driven, and it serves exactly the steps that need nothing outside
//! the engine. A chain that needs blob bytes or an action stops and says so,
//! leaving the effect to a driver — which is what keeps the claim "the core is
//! structurally pure" checkable rather than aspirational.

use pith_core::{Action, Pure, Request, RuleId, Value};
use pith_diag::{Diag, DiagnosticSink, EngineCode, PithResult};
use pith_ids::{ComputationId, ContentId};
use smallvec::SmallVec;

use super::diagnostics::{
    InternalInvariant, content_unavailable_diag, cycle_diag, internal_diag, one_diag,
    step_budget_diag, store_error_diag,
};
use super::ir::{
    AttemptState, ComputationKind, ComputationNode, DependencyEdge, EvalFrame, Evaluation,
    EvaluationSource, PureStep, Resumption, StopReason,
};
use super::reuse::ReuseContext;
use super::scheduler::{ChainId, Scheduler};
use super::{Engine, PureComputationKey};
use crate::bound::StepBudget;

/// Why [`Engine::advance_chain`] stopped.
pub(super) enum ChainPause {
    /// The chain finished, or parked waiting on a fan-out group. Nothing is
    /// owed to it; the scheduler will make it ready again if it can.
    Settled,
    /// The chain needs the bytes of a blob before it can continue.
    Blob(ContentId),
    /// The chain needs an action executed before it can continue.
    Action(Request<Action>),
}

/// One request of a fan-out group, once the engine knows whether it needs
/// evaluating at all.
enum PreparedRequest {
    Reused(Value),
    Fresh(EvalFrame),
}

/// A root request, once the engine knows the same.
enum OpenedRoot {
    Reused(Evaluation),
    Fresh(EvalFrame),
}

/// The chains a run starts with, plus the roots that needed no evaluation.
pub(super) struct RootPlan {
    pub(super) scheduler: Scheduler,
    /// One slot per requested root, in order: `Some` for a root answered from
    /// the reusable index, `None` for one the scheduler is evaluating.
    reused: Vec<Option<Evaluation>>,
}

impl RootPlan {
    /// The evaluations in request order, pairing the roots answered from the
    /// index with the ones the scheduler computed.
    pub(super) fn into_evaluations(self) -> PithResult<Box<[Evaluation]>> {
        let Self { scheduler, reused } = self;
        let mut computed = scheduler.into_roots().into_iter();
        let mut evaluations = Vec::with_capacity(reused.len());
        for slot in reused {
            let evaluation = match slot {
                Some(evaluation) => evaluation,
                None => match computed.next().flatten() {
                    Some(evaluation) => evaluation,
                    None => return Err(internal_diag(InternalInvariant::SchedulerLostRootSlot)),
                },
            };
            evaluations.push(evaluation);
        }
        Ok(evaluations.into_boxed_slice())
    }
}

/// Unwrap the single evaluation of a one-root run.
pub(super) fn single_evaluation(evaluations: Box<[Evaluation]>) -> PithResult<Evaluation> {
    match evaluations.into_vec().pop() {
        Some(evaluation) => Ok(evaluation),
        None => Err(internal_diag(InternalInvariant::SchedulerLostRootSlot)),
    }
}

impl Engine {
    /// Run `chain` on the step machine until it settles or needs an effect.
    ///
    /// Each step spends one unit of `budget`. A body that yields an unbounded
    /// sequence of distinct requests steps many times between two scheduling
    /// boundaries, so the budget is spent here rather than at the boundary
    /// (decision 0059).
    pub(super) fn advance_chain(
        &mut self,
        scheduler: &mut Scheduler,
        chain: ChainId,
        context: &ReuseContext<'_>,
        budget: &mut StepBudget,
    ) -> PithResult<ChainPause> {
        loop {
            if !budget.spend() {
                let label = scheduler.top(chain)?.request.label.clone();
                let total = budget.total().unwrap_or_default();
                return Err(step_budget_diag(total, &label));
            }
            let step = {
                let Some(frame) = scheduler.stack_mut(chain)?.last_mut() else {
                    return Err(internal_diag(InternalInvariant::PureLostRootFrame));
                };
                let resumption = frame.resume_with.take();
                frame.body.step(resumption)?
            };
            match step {
                PureStep::Complete(value) => {
                    if self.complete_top_frame(scheduler, chain, value)? {
                        return Ok(ChainPause::Settled);
                    }
                }
                PureStep::Need(request) => {
                    self.handle_pure_need(scheduler, chain, request, context)?;
                }
                PureStep::NeedAll(requests) => {
                    self.handle_pure_need_all(scheduler, chain, requests, context)?;
                    return Ok(ChainPause::Settled);
                }
                PureStep::NeedBlob(id) => return Ok(ChainPause::Blob(id)),
                PureStep::NeedAction(request) => return Ok(ChainPause::Action(request)),
            }
        }
    }

    /// Finish the top frame of `chain` with `value`. Reports whether the chain
    /// itself is done, which it is when the frame that completed was its last.
    fn complete_top_frame(
        &mut self,
        scheduler: &mut Scheduler,
        chain: ChainId,
        value: Value,
    ) -> PithResult<bool> {
        let completed = self.finish_frame(scheduler.top(chain)?, value)?;
        scheduler.pop_frame(chain)?;
        match scheduler.stack_mut(chain)?.last_mut() {
            Some(parent) => {
                parent.resume_with = Some(Resumption::One(completed.value.clone()));
                Ok(false)
            }
            None => {
                scheduler.complete_chain(chain, completed)?;
                Ok(true)
            }
        }
    }

    /// Handle a `PureStep::Need`: the requested computation becomes the next
    /// frame of the same chain, so the request is ordered after everything the
    /// body has already asked for.
    fn handle_pure_need(
        &mut self,
        scheduler: &mut Scheduler,
        chain: ChainId,
        request: Request<Pure>,
        context: &ReuseContext<'_>,
    ) -> PithResult<()> {
        let parent = scheduler.top(chain)?.computation;
        match self.prepare_request(scheduler, chain, parent, request, context)? {
            PreparedRequest::Reused(value) => {
                let Some(frame) = scheduler.stack_mut(chain)?.last_mut() else {
                    return Err(internal_diag(InternalInvariant::PureLostRequestingFrame));
                };
                frame.resume_with = Some(Resumption::One(value));
            }
            PreparedRequest::Fresh(frame) => scheduler.push_frame(chain, frame)?,
        }
        Ok(())
    }

    /// Handle a `PureStep::NeedAll`: each request that still needs evaluating
    /// becomes a chain of its own, and the requesting chain parks until all of
    /// them land. Two identical requests in one batch get a computation each —
    /// the reusable index dedupes across time, not within a batch.
    fn handle_pure_need_all(
        &mut self,
        scheduler: &mut Scheduler,
        chain: ChainId,
        requests: Box<[Request<Pure>]>,
        context: &ReuseContext<'_>,
    ) -> PithResult<()> {
        let parent = scheduler.top(chain)?.computation;
        let mut prepared = Vec::with_capacity(requests.len());
        for request in requests.into_vec() {
            prepared.push(self.prepare_request(scheduler, chain, parent, request, context)?);
        }

        // Every request was already computed, so there is nothing to wait for
        // and no group to open. An empty batch lands here too.
        if !prepared
            .iter()
            .any(|request| matches!(request, PreparedRequest::Fresh(_)))
        {
            let mut values = Vec::with_capacity(prepared.len());
            for request in prepared {
                if let PreparedRequest::Reused(value) = request {
                    values.push(value);
                }
            }
            return scheduler.resume(chain, Resumption::Many(values.into_boxed_slice()));
        }

        let group = scheduler.open_group(chain, prepared.len());
        for (slot, request) in prepared.into_iter().enumerate() {
            match request {
                PreparedRequest::Reused(value) => scheduler.fill_group_slot(group, slot, value)?,
                PreparedRequest::Fresh(frame) => {
                    scheduler.start_group_chain(group, slot, frame);
                }
            }
        }
        Ok(())
    }

    /// Resolve one requested pure computation: select its rule, reject a cycle,
    /// reuse a completed result if there is one, and record the dependency edge
    /// on `parent` either way.
    fn prepare_request(
        &mut self,
        scheduler: &Scheduler,
        chain: ChainId,
        parent: ComputationId,
        request: Request<Pure>,
        context: &ReuseContext<'_>,
    ) -> PithResult<PreparedRequest> {
        let rule = self.resolve_pure_rule(&request)?;
        let key = self.pure_key_for(rule, &request)?;
        if let Some(labels) = scheduler.cycle_chain(chain, key.digest, &request.label) {
            let cycle: Vec<&str> = labels.iter().map(AsRef::as_ref).collect();
            return Err(cycle_diag(&cycle, request.span));
        }
        if let Some(reused) = self.reusable_pure_evaluation(rule, &request, context)? {
            self.record_edge(
                parent,
                DependencyEdge::Request {
                    computation: reused.computation,
                    request,
                },
            )?;
            return Ok(PreparedRequest::Reused(reused.value));
        }
        let frame = self.start_frame(request.clone(), rule, key)?;
        self.record_edge(
            parent,
            DependencyEdge::Request {
                computation: frame.computation,
                request,
            },
        )?;
        Ok(PreparedRequest::Fresh(frame))
    }

    /// Open one chain per root request, skipping the roots the reusable index
    /// can already answer.
    pub(super) fn open_roots(
        &mut self,
        requests: &[Request<Pure>],
        context: &ReuseContext<'_>,
    ) -> PithResult<RootPlan> {
        let mut reused = Vec::with_capacity(requests.len());
        let mut frames = Vec::new();
        for request in requests {
            match self.open_root(request, context) {
                Ok(OpenedRoot::Reused(evaluation)) => reused.push(Some(evaluation)),
                Ok(OpenedRoot::Fresh(frame)) => {
                    frames.push(frame);
                    reused.push(None);
                }
                Err(diagnostics) => {
                    // The roots opened before this one already have Pending
                    // arena nodes. Fail them so an aborted setup leaves nothing
                    // waiting on a run that will not happen.
                    let opened: Vec<ComputationId> =
                        frames.iter().map(|frame| frame.computation).collect();
                    self.stop_pending(&opened, &diagnostics, StopReason::Failed);
                    return Err(diagnostics);
                }
            }
        }
        Ok(RootPlan {
            scheduler: Scheduler::with_roots(frames),
            reused,
        })
    }

    fn open_root(
        &mut self,
        request: &Request<Pure>,
        context: &ReuseContext<'_>,
    ) -> PithResult<OpenedRoot> {
        let rule = self.resolve_pure_rule(request)?;
        match self.reusable_pure_evaluation(rule, request, context)? {
            Some(evaluation) => Ok(OpenedRoot::Reused(evaluation)),
            None => {
                let key = self.pure_key_for(rule, request)?;
                Ok(OpenedRoot::Fresh(self.start_frame(
                    request.clone(),
                    rule,
                    key,
                )?))
            }
        }
    }

    pub(super) fn resolve_pure_rule(&self, request: &Request<Pure>) -> PithResult<RuleId> {
        request.validate_inputs().map_err(one_diag)?;
        self.rules
            .select(request)
            .into_result(request, &self.rules)
            .map_err(one_diag)
    }

    /// The computation key for applying `rule` to `request`.
    ///
    /// Derived once per prepared request and carried on the frame, because three
    /// things want it: the cycle predicate, the reusable index, and the durable
    /// attempt (decision 0050).
    fn pure_key_for(
        &self,
        rule: RuleId,
        request: &Request<Pure>,
    ) -> PithResult<PureComputationKey> {
        let Some(rule_metadata) = self.rules.get(rule) else {
            return Err(internal_diag(InternalInvariant::SelectedRuleHasNoMetadata));
        };
        Ok(PureComputationKey::new(rule_metadata, request))
    }

    fn start_frame(
        &mut self,
        request: Request<Pure>,
        rule: RuleId,
        key: PureComputationKey,
    ) -> PithResult<EvalFrame> {
        let Some(body) = self.bodies.get(&rule) else {
            return Err(internal_diag(InternalInvariant::SelectedRuleHasNoBody));
        };
        let body = body.start(&request.inputs);
        let computation = self.computations.push(ComputationNode {
            kind: ComputationKind::Pure(request.clone()),
            rule,
            dependencies: SmallVec::new(),
            state: AttemptState::Pending,
            action: None,
            capabilities: Box::new([]),
        });
        self.index_pure_computation(key, computation);
        if let Err(diagnostics) = self.create_pending_pure_attempt(computation, key) {
            // The durable attempt could not be created (only a failing adapter
            // reaches this; the memory adapter is infallible). Mirror the action
            // path's error hygiene: fail the orphaned arena node so it is not
            // left Pending. No durable failure is published because no durable
            // attempt exists; the diagnostics propagate to the caller.
            self.fail_pure_orphan(computation, &diagnostics);
            return Err(diagnostics);
        }
        Ok(EvalFrame {
            computation,
            rule,
            request,
            key_digest: key.digest,
            body,
            resume_with: None,
        })
    }

    /// Complete `completed` with `value`: type-check the result, mark the arena
    /// node terminal, and publish the durable record. The caller pops the frame
    /// afterwards, so the frame's computation id is still valid for the store
    /// call (decision 0024).
    fn finish_frame(&mut self, completed: &EvalFrame, value: Value) -> PithResult<Evaluation> {
        let Some(rule) = self.rules.get(completed.rule) else {
            return Err(internal_diag(
                InternalInvariant::PureLostSelectedRuleMetadata,
            ));
        };
        if !value.is_type(&completed.request.interface.output) {
            let actual = value.value_type();
            return Err(one_diag(Diag::engine(
                EngineCode::ResultTypeMismatch,
                rule.span,
                format!(
                    "rule `{}` returned {}, expected {}",
                    rule.label, actual, completed.request.interface.output
                ),
            )));
        }
        let (reuse, capabilities) = match self.computations.get(completed.computation) {
            Some(node) => (
                self.reuse_decision(&node.dependencies),
                self.effective_capabilities(&node.dependencies),
            ),
            None => return Err(internal_diag(InternalInvariant::PureLostComputationNode)),
        };
        let Some(capabilities) = capabilities else {
            return Err(internal_diag(
                InternalInvariant::PureLostCapabilityComputation,
            ));
        };
        let Some(node) = self.computations.get_mut(completed.computation) else {
            return Err(internal_diag(InternalInvariant::PureLostComputationNode));
        };
        node.state = AttemptState::Complete {
            result: value.clone(),
            reuse,
        };
        node.capabilities = capabilities;
        let computation = completed.computation;
        self.publish_pure_completion(computation)?;
        Ok(Evaluation {
            value,
            computation,
            source: EvaluationSource::Computed,
        })
    }

    /// Stop every computation the scheduler still holds a frame for. A run that
    /// ends early leaves chains parked mid-evaluation; without this their arena
    /// nodes would stay `Pending` forever.
    pub(super) fn stop_live_frames(
        &mut self,
        scheduler: &Scheduler,
        diagnostics: &DiagnosticSink,
        reason: StopReason,
    ) {
        let live: Vec<ComputationId> = scheduler
            .live_frames()
            .map(|frame| frame.computation)
            .collect();
        self.stop_pending(&live, diagnostics, reason);
    }

    fn stop_pending(
        &mut self,
        computations: &[ComputationId],
        diagnostics: &DiagnosticSink,
        reason: StopReason,
    ) {
        for computation in computations {
            let Some(node) = self.computations.get_mut(*computation) else {
                continue;
            };
            if matches!(node.state, AttemptState::Pending) {
                node.state = reason.attempt_state(diagnostics.iter().cloned().collect());
                // Scheduling boundary: publish the durable record for this pure
                // attempt. The store call is best-effort with respect to the
                // returned diagnostics — publication must not mask the
                // diagnostics that caused it.
                let _ = self.publish_pure_stop(*computation);
            }
        }
    }

    /// Reconcile an orphaned pure computation whose durable attempt could not be
    /// created: mark it `Failed` in the arena so no `Pending` node remains. No
    /// durable failure is published because no durable attempt exists.
    fn fail_pure_orphan(&mut self, computation: ComputationId, diagnostics: &DiagnosticSink) {
        if let Some(node) = self.computations.get_mut(computation) {
            node.state = AttemptState::Failed {
                diagnostics: diagnostics.iter().cloned().collect(),
            };
        }
    }

    pub(super) fn fetch_blob(&self, id: ContentId) -> PithResult<Box<[u8]>> {
        match self.store.get_blob(id).map_err(store_error_diag)? {
            Some(blob) => Ok(blob.as_bytes().to_vec().into_boxed_slice()),
            None => Err(content_unavailable_diag(id)),
        }
    }

    /// Record a dependency of `parent`. Every edge the evaluator adds goes
    /// through here, so "the requesting computation must still exist" is stated
    /// once instead of at each edge site.
    pub(super) fn record_edge(
        &mut self,
        parent: ComputationId,
        edge: DependencyEdge,
    ) -> PithResult<()> {
        let Some(node) = self.computations.get_mut(parent) else {
            return Err(internal_diag(InternalInvariant::PureLostParentComputation));
        };
        node.dependencies.push(edge);
        Ok(())
    }
}
