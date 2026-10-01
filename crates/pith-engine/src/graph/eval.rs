//! The step machine's core: frame lifecycle, the pure steps, and every
//! decision a step forces. Both drivers call into it. Nothing here awaits,
//! and [`run`] holds the awaiting mirror.

mod run;

use pith_core::{Action, Observation, Pure, PureComputationKey, Request, RuleId, Value};
use pith_diag::{Diag, DiagnosticSink, EngineCode, PithResult};
use pith_ids::{ComputationId, ContentId};
use smallvec::SmallVec;

use super::Engine;
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
use crate::bound::StepBudget;

/// Why [`Engine::advance_chain`] stopped.
pub(super) enum ChainPause {
    /// The chain finished, or parked waiting on a fan-out group. Nothing is
    /// owed to it; the scheduler will make it ready again if it can.
    Settled,
    /// The chain needs the bytes of a blob, which is not an effect category
    /// and so has no [`EffectKind`](pith_core::EffectKind).
    Blob(ContentId),
    /// The chain needs an action executed: the stop an
    /// [`EffectKind::Action`](pith_core::EffectKind) request yields.
    Action(Request<Action>),
    /// The chain needs external state observed: the stop an
    /// [`EffectKind::Observation`](pith_core::EffectKind) request yields.
    Observation(Request<Observation>),
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

/// Make one prepared request the requesting chain's next state: a reused
/// result resumes the frame that asked, a fresh frame becomes the chain's
/// top.
fn resume_or_push(
    scheduler: &mut Scheduler,
    chain: ChainId,
    prepared: PreparedRequest,
) -> PithResult<()> {
    match prepared {
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

/// Make a prepared batch the requesting chain's next state: reused values
/// resume the chain, fresh requests fan out into a group.
fn resume_or_fan_out(
    scheduler: &mut Scheduler,
    chain: ChainId,
    prepared: Vec<PreparedRequest>,
) -> PithResult<()> {
    if !prepared
        .iter()
        .any(|request| matches!(request, PreparedRequest::Fresh(_)))
    {
        let values = prepared
            .into_iter()
            .filter_map(|request| match request {
                PreparedRequest::Reused(value) => Some(value),
                PreparedRequest::Fresh(_) => None,
            })
            .collect::<Vec<_>>();
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

/// Spend one unit of `budget` and advance `chain`'s top frame by one step.
/// A body can step many times between two scheduling boundaries, so the
/// budget is spent here rather than at the boundary.
fn take_step(
    scheduler: &mut Scheduler,
    chain: ChainId,
    budget: &mut StepBudget,
) -> PithResult<PureStep> {
    if !budget.spend() {
        let label = scheduler.top(chain)?.request.label.clone();
        let total = budget.total().unwrap_or_default();
        return Err(step_budget_diag(total, &label));
    }
    let Some(frame) = scheduler.stack_mut(chain)?.last_mut() else {
        return Err(internal_diag(InternalInvariant::PureLostRootFrame));
    };
    let resumption = frame.resume_with.take();
    frame.body.step(resumption)
}

impl Engine {
    /// Run `chain` on the step machine until it settles or needs an effect.
    pub(super) fn advance_chain(
        &mut self,
        scheduler: &mut Scheduler,
        chain: ChainId,
        context: &ReuseContext<'_>,
        budget: &mut StepBudget,
    ) -> PithResult<ChainPause> {
        loop {
            let step = take_step(scheduler, chain, budget)?;
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
                PureStep::NeedObservation(request) => {
                    return Ok(ChainPause::Observation(request));
                }
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
        let (rule, key) = self.select_request(scheduler, chain, &request)?;
        let prepared = self.prepare_request(parent, request, rule, key, context)?;
        resume_or_push(scheduler, chain, prepared)
    }

    /// Handle a `PureStep::NeedAll`: each request that still needs evaluating
    /// becomes a chain of its own, and the requesting chain parks until all
    /// of them land.
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
            let (rule, key) = self.select_request(scheduler, chain, &request)?;
            prepared.push(self.prepare_request(parent, request, rule, key, context)?);
        }
        resume_or_fan_out(scheduler, chain, prepared)
    }

    /// Resolve one requested pure computation's rule and key, rejecting a
    /// cycle.
    fn select_request(
        &self,
        scheduler: &Scheduler,
        chain: ChainId,
        request: &Request<Pure>,
    ) -> PithResult<(RuleId, PureComputationKey)> {
        let rule = self.resolve_pure_rule(request)?;
        let key = self.pure_key_for(rule, request)?;
        if let Some(labels) = scheduler.cycle_chain(chain, key.digest, &request.label) {
            let cycle: Vec<&str> = labels.iter().map(AsRef::as_ref).collect();
            return Err(cycle_diag(&cycle, request.span));
        }
        Ok((rule, key))
    }

    /// Prepare one request after its rule and key are resolved: reuse a
    /// completed result if there is one, and record the dependency edge on
    /// `parent` either way.
    fn prepare_request(
        &mut self,
        parent: ComputationId,
        request: Request<Pure>,
        rule: RuleId,
        key: PureComputationKey,
        context: &ReuseContext<'_>,
    ) -> PithResult<PreparedRequest> {
        match self.reusable_pure_evaluation(rule, &request, context)? {
            Some(reused) => self.reused_request(parent, request, reused),
            None => self.prepare_fresh_request(parent, request, rule, key),
        }
    }

    /// Record the edge to a computation the reuse index served, and finish the
    /// preparation with its result.
    fn reused_request(
        &mut self,
        parent: ComputationId,
        request: Request<Pure>,
        reused: Evaluation,
    ) -> PithResult<PreparedRequest> {
        let computation = reused.computation;
        self.record_edge(
            parent,
            DependencyEdge::Request {
                computation,
                request,
            },
        )?;
        Ok(PreparedRequest::Reused(reused.value))
    }

    /// Start the frame for a request the reuse index could not serve, and
    /// record the edge to the computation it will produce.
    fn prepare_fresh_request(
        &mut self,
        parent: ComputationId,
        request: Request<Pure>,
        rule: RuleId,
        key: PureComputationKey,
    ) -> PithResult<PreparedRequest> {
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
                    self.abort_root_openings(&frames, &diagnostics);
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
            None => self.open_root_fresh(request, rule).map(OpenedRoot::Fresh),
        }
    }

    /// Start the frame for a root the reuse index could not answer.
    fn open_root_fresh(&mut self, request: &Request<Pure>, rule: RuleId) -> PithResult<EvalFrame> {
        let key = self.pure_key_for(rule, request)?;
        self.start_frame(request.clone(), rule, key)
    }

    /// Fail the frames of roots opened before a failure, so nothing waits on
    /// a run that will not happen.
    fn abort_root_openings(&mut self, frames: &[EvalFrame], diagnostics: &DiagnosticSink) {
        let opened: Vec<ComputationId> = frames.iter().map(|frame| frame.computation).collect();
        self.stop_pending(&opened, diagnostics, StopReason::Failed);
    }

    pub(super) fn resolve_pure_rule(&self, request: &Request<Pure>) -> PithResult<RuleId> {
        request.validate_inputs().map_err(one_diag)?;
        self.pure
            .rules
            .select(request)
            .into_result(request, &self.pure.rules)
            .map_err(one_diag)
    }

    /// The computation key for applying `rule` to `request`.
    fn pure_key_for(
        &self,
        rule: RuleId,
        request: &Request<Pure>,
    ) -> PithResult<PureComputationKey> {
        let Some(rule_metadata) = self.pure.rules.get(rule) else {
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
        let Some(body) = self.pure.bodies.get(&rule) else {
            return Err(internal_diag(InternalInvariant::SelectedRuleHasNoBody));
        };
        let body = body.start(&request.inputs);
        let computation = self.computations.push(ComputationNode {
            kind: ComputationKind::Pure(request.clone()),
            rule,
            dependencies: SmallVec::new(),
            state: AttemptState::Pending,
            action: None,
            observation: None,
            capabilities: Box::new([]),
        });
        self.pure.computations.insert(key, computation);
        if let Err(diagnostics) = self.create_pending_pure_attempt(computation, key) {
            // Fail the orphaned arena node so nothing stays Pending. No
            // durable attempt exists, so no durable failure is published.
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

    /// Complete `completed` with `value`: type-check the result, mark the
    /// arena node terminal, and publish the durable record. The caller pops
    /// the frame afterwards, so the computation id is still valid here.
    fn finish_frame(&mut self, completed: &EvalFrame, value: Value) -> PithResult<Evaluation> {
        let Some(rule) = self.pure.rules.get(completed.rule) else {
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
                // attempt. Best effort, so publication cannot mask the
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

    /// Record a dependency of `parent`, checking that the requesting
    /// computation still exists.
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
