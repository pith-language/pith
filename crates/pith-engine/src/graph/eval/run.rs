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
        let (rule, key) = self.select_request_run(scheduler, chain, &request)?;
        match self
            .prepare_request_run(parent, request, rule, key, context, bound)
            .await?
        {
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
            let (rule, key) = self.select_request_run(scheduler, chain, &request)?;
            prepared.push(
                self.prepare_request_run(parent, request, rule, key, context, bound)
                    .await?,
            );
        }

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

    fn select_request_run(
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

    async fn prepare_request_run(
        &mut self,
        parent: ComputationId,
        request: Request<Pure>,
        rule: RuleId,
        key: PureComputationKey,
        context: &ReuseContext<'_>,
        bound: &crate::RunBound,
    ) -> PithResult<PreparedRequest> {
        if let Some(reused) = self
            .reusable_pure_evaluation_run(rule, &request, context, bound)
            .await?
        {
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

    pub(in crate::graph) async fn open_roots_run(
        &mut self,
        requests: &[Request<Pure>],
        context: &ReuseContext<'_>,
        bound: &crate::RunBound,
    ) -> PithResult<RootPlan> {
        let mut reused = Vec::with_capacity(requests.len());
        let mut frames = Vec::new();
        for request in requests {
            let opened = self.open_root_run(request, context, bound).await;
            match opened {
                Ok(OpenedRoot::Reused(evaluation)) => reused.push(Some(evaluation)),
                Ok(OpenedRoot::Fresh(frame)) => {
                    frames.push(frame);
                    reused.push(None);
                }
                Err(diagnostics) => {
                    let opened = frames
                        .iter()
                        .map(|frame| frame.computation)
                        .collect::<Vec<_>>();
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
}
