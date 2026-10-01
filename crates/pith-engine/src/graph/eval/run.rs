//! The run driver's mirror of the step machine: the same loop and decisions
//! as the synchronous core, except a run's reuse checks await, because
//! revalidating a recorded observation re-attests it through the observer.

use super::*;

impl Engine {
    pub(in crate::graph) async fn advance_chain_run(
        &mut self,
        scheduler: &mut Scheduler,
        chain: ChainId,
        context: &ReuseContext<'_>,
        budget: &mut StepBudget,
        bound: &crate::RunBound,
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
                    self.handle_pure_need_run(scheduler, chain, request, context, bound)
                        .await?;
                }
                PureStep::NeedAll(requests) => {
                    self.handle_pure_need_all_run(scheduler, chain, requests, context, bound)
                        .await?;
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

    async fn handle_pure_need_run(
        &mut self,
        scheduler: &mut Scheduler,
        chain: ChainId,
        request: Request<Pure>,
        context: &ReuseContext<'_>,
        bound: &crate::RunBound,
    ) -> PithResult<()> {
        let parent = scheduler.top(chain)?.computation;
        let (rule, key) = self.select_request(scheduler, chain, &request)?;
        let prepared = self
            .prepare_request_run(parent, request, rule, key, context, bound)
            .await?;
        resume_or_push(scheduler, chain, prepared)
    }

    async fn handle_pure_need_all_run(
        &mut self,
        scheduler: &mut Scheduler,
        chain: ChainId,
        requests: Box<[Request<Pure>]>,
        context: &ReuseContext<'_>,
        bound: &crate::RunBound,
    ) -> PithResult<()> {
        let parent = scheduler.top(chain)?.computation;
        let mut prepared = Vec::with_capacity(requests.len());
        for request in requests.into_vec() {
            let (rule, key) = self.select_request(scheduler, chain, &request)?;
            prepared.push(
                self.prepare_request_run(parent, request, rule, key, context, bound)
                    .await?,
            );
        }
        resume_or_fan_out(scheduler, chain, prepared)
    }

    /// [`Self::prepare_request`] with the reuse check that awaits.
    async fn prepare_request_run(
        &mut self,
        parent: ComputationId,
        request: Request<Pure>,
        rule: RuleId,
        key: PureComputationKey,
        context: &ReuseContext<'_>,
        bound: &crate::RunBound,
    ) -> PithResult<PreparedRequest> {
        match self
            .reusable_pure_evaluation_run(rule, &request, context, bound)
            .await?
        {
            Some(reused) => self.reused_request(parent, request, reused),
            None => self.prepare_fresh_request(parent, request, rule, key),
        }
    }

    pub(in crate::graph) async fn open_roots_run(
        &mut self,
        requests: &[Request<Pure>],
        context: &ReuseContext<'_>,
        bound: &crate::RunBound,
    ) -> PithResult<RootPlan> {
        let mut reused = Vec::with_capacity(requests.len());
        let mut frames = Vec::new();
        for request in requests {
            match self.open_root_run(request, context, bound).await {
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

    async fn open_root_run(
        &mut self,
        request: &Request<Pure>,
        context: &ReuseContext<'_>,
        bound: &crate::RunBound,
    ) -> PithResult<OpenedRoot> {
        let rule = self.resolve_pure_rule(request)?;
        match self
            .reusable_pure_evaluation_run(rule, request, context, bound)
            .await?
        {
            Some(evaluation) => Ok(OpenedRoot::Reused(evaluation)),
            None => self.open_root_fresh(request, rule).map(OpenedRoot::Fresh),
        }
    }
}
