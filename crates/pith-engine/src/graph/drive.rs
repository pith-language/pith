//! The two drivers that turn ready chains into progress. Both loop over the
//! scheduler's ready chains and hand each to [`Engine::advance_chain`]; they
//! differ in what they do with a chain that stopped for an effect: the pure
//! driver rejects it, the run driver serves it (blobs inline, actions through
//! the executor, so actions of independent chains overlap).
//!
//! Overlap is bounded: the run driver starts queued actions only while fewer
//! than [`Engine::action_concurrency`] actions run. The queue holds requests,
//! not invocations: a queued action has not been planned, has no computation
//! node, and has materialized nothing.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::task::Poll;

use pith_core::{Action, ActionSpec, Request, Value};
use pith_diag::{Diag, DiagnosticSink, EngineCode, PithResult, Span};
use pith_ids::{ComputationId, ContentId};

use super::action_pipeline::{ActionRuleMeta, ActionStart, PreparedAction};
use super::diagnostics::{cancelled_diag, effectful_in_pure_diag, is_bound_stop, wall_bound_diag};
use super::eval::ChainPause;
use super::ir::{EntryActionPlan, Resumption, StopReason};
use super::reuse::ReuseContext;
use super::scheduler::{ChainId, Scheduler};
use super::{DependencyEdge, Engine};
use crate::action::{CapturedActionExecution, Executor};
use crate::bound::{RunBound, StepBudget};
use crate::cancel::CancelSignal;
use crate::policy::ActionPolicy;

/// Why a run stopped early, carried so the work it held records under the
/// right terminal state. A plain `?` on a diagnostic is a failure;
/// cancellation is constructed deliberately.
struct RunAbort {
    reason: StopReason,
    diagnostics: pith_diag::DiagnosticSink,
}

impl From<pith_diag::DiagnosticSink> for RunAbort {
    fn from(diagnostics: pith_diag::DiagnosticSink) -> Self {
        // A diagnostic the bound produced stops the run the way cancellation
        // does: the work being held is stopped, not broken. One exception:
        // the action that exceeded a wall clock was already recorded failed
        // where it ran, before this conversion sees it.
        let reason = if is_bound_stop(&diagnostics) {
            StopReason::Cancelled
        } else {
            StopReason::Failed
        };
        Self {
            reason,
            diagnostics,
        }
    }
}

/// What starting an action produces: an execution to wait on, or a chain to
/// wake with a result an earlier run recorded.
#[derive(Default)]
struct StartedActions<'a> {
    in_flight: Vec<InFlightAction<'a>>,
    resumed: VecDeque<(ChainId, Value)>,
}

/// One action handed to an executor, with everything the engine needs to finish
/// it when the executor returns. The requesting chain is parked meanwhile, so
/// other chains keep running.
struct InFlightAction<'a> {
    chain: ChainId,
    computation: ComputationId,
    request: Request<Action>,
    spec: ActionSpec,
    rule_meta: ActionRuleMeta,
    execution: Pin<Box<dyn Future<Output = PithResult<CapturedActionExecution>> + Send + 'a>>,
}

/// Wait for the first in-flight action to finish, reporting its index. Each
/// pending execution registers its waker, so the driver wakes as soon as any
/// makes progress; the rescan costs the number of actions in flight, not the
/// size of the graph.
async fn first_finished(
    actions: &mut [InFlightAction<'_>],
) -> (usize, PithResult<CapturedActionExecution>) {
    std::future::poll_fn(|context| {
        for (index, action) in actions.iter_mut().enumerate() {
            if let Poll::Ready(captured) = action.execution.as_mut().poll(context) {
                return Poll::Ready((index, captured));
            }
        }
        Poll::Pending
    })
    .await
}

fn cancelled_abort() -> RunAbort {
    RunAbort {
        reason: StopReason::Cancelled,
        diagnostics: cancelled_diag(),
    }
}

fn bound_abort() -> RunAbort {
    RunAbort {
        reason: StopReason::Cancelled,
        diagnostics: wall_bound_diag(),
    }
}

/// What one action's start needs from its run: the reuse context it plans
/// under and the bound its deadline descends from.
struct Serving<'a> {
    context: &'a ReuseContext<'a>,
    bound: RunBound,
}

impl Engine {
    /// Serve a chain's blob stop: fetch the bytes, record the content edge,
    /// and resume the chain with them.
    fn serve_blob(
        &mut self,
        scheduler: &mut Scheduler,
        chain: ChainId,
        id: ContentId,
    ) -> PithResult<()> {
        let bytes = self.fetch_blob(id)?;
        let parent = scheduler.top(chain)?.computation;
        self.record_edge(parent, DependencyEdge::Blob { id })?;
        scheduler.resume(chain, Resumption::One(Value::Bytes(bytes)))
    }

    /// Evaluate an entry's pure prefix, admitting blobs, and stop at the first
    /// action request with its selected contract. No action is executed. Live
    /// frames record as cancelled: the caller paused them deliberately at the
    /// effect boundary.
    ///
    /// # Errors
    /// Returns pure-evaluation, content, action-selection, or planner
    /// diagnostics, and `E-1219` when the entry completes without an action.
    pub fn plan_entry(
        &mut self,
        request: &Request<pith_core::Pure>,
    ) -> PithResult<EntryActionPlan> {
        let mut roots = self.open_roots(std::slice::from_ref(request), &ReuseContext::PureOnly)?;
        let mut budget = StepBudget::unbounded();
        while let Some(chain) = roots.scheduler.next_ready() {
            let pause = self.advance_chain(
                &mut roots.scheduler,
                chain,
                &ReuseContext::PureOnly,
                &mut budget,
            );
            let pause = match pause {
                Ok(pause) => pause,
                Err(diagnostics) => {
                    self.stop_live_frames(&roots.scheduler, &diagnostics, StopReason::Failed);
                    return Err(diagnostics);
                }
            };
            match pause {
                ChainPause::Settled => {}
                ChainPause::Blob(id) => {
                    if let Err(diagnostics) = self.serve_blob(&mut roots.scheduler, chain, id) {
                        self.stop_live_frames(&roots.scheduler, &diagnostics, StopReason::Failed);
                        return Err(diagnostics);
                    }
                }
                ChainPause::Action(action_request) => {
                    let action = match self.plan_action(&action_request) {
                        Ok(action) => action,
                        Err(diagnostics) => {
                            self.stop_live_frames(
                                &roots.scheduler,
                                &diagnostics,
                                StopReason::Failed,
                            );
                            return Err(diagnostics);
                        }
                    };
                    self.stop_live_frames(
                        &roots.scheduler,
                        &DiagnosticSink::new(),
                        StopReason::Cancelled,
                    );
                    return Ok(EntryActionPlan {
                        request: action_request,
                        action,
                    });
                }
                ChainPause::Observation(_) => {
                    let mut diagnostics = DiagnosticSink::new();
                    diagnostics.push(Diag::engine(
                        EngineCode::EffectfulStepInPure,
                        Span::none(),
                        "entry planning reached an observation before an action",
                    ));
                    self.stop_live_frames(&roots.scheduler, &diagnostics, StopReason::Failed);
                    return Err(diagnostics);
                }
            }
        }
        let _ = roots.into_evaluations()?;
        let mut diagnostics = DiagnosticSink::new();
        diagnostics.push(Diag::engine(
            EngineCode::EntryHasNoAction,
            request.span,
            format!(
                "entry `{}` completed without requesting an action",
                request.label
            ),
        ));
        Err(diagnostics)
    }

    /// Drive every chain to completion without leaving the synchronous core.
    pub(super) fn drive_pure(&mut self, scheduler: &mut Scheduler) -> PithResult<()> {
        let mut budget = StepBudget::unbounded();
        while let Some(chain) = scheduler.next_ready() {
            match self.advance_chain(scheduler, chain, &ReuseContext::PureOnly, &mut budget)? {
                ChainPause::Settled => {}
                ChainPause::Blob(_) | ChainPause::Action(_) | ChainPause::Observation(_) => {
                    return Err(effectful_in_pure_diag());
                }
            }
        }
        Ok(())
    }

    pub(super) fn drive_with_content(&mut self, scheduler: &mut Scheduler) -> PithResult<()> {
        let mut budget = StepBudget::unbounded();
        while let Some(chain) = scheduler.next_ready() {
            match self.advance_chain(scheduler, chain, &ReuseContext::PureOnly, &mut budget)? {
                ChainPause::Settled => {}
                ChainPause::Blob(id) => self.serve_blob(scheduler, chain, id)?,
                ChainPause::Action(_) | ChainPause::Observation(_) => {
                    return Err(effectful_in_pure_diag());
                }
            }
        }
        Ok(())
    }

    /// Drive every chain to completion, serving the effects they stop for.
    ///
    /// A run that ends early (cancelled, past its bound, or aborted by a
    /// diagnostic) leaves chains parked and actions in flight. Both are
    /// recorded here under the terminal state that matches why the run ended,
    /// before the diagnostics propagate.
    pub(super) async fn drive_run<P: ActionPolicy, E: Executor, C: CancelSignal>(
        &mut self,
        scheduler: &mut Scheduler,
        policy: &P,
        executor: &E,
        cancel: &C,
        bound: &RunBound,
    ) -> PithResult<()> {
        let mut started = StartedActions::default();
        let Err(abort) = self
            .drive_chains(scheduler, policy, executor, cancel, bound, &mut started)
            .await
        else {
            return Ok(());
        };
        // An action still running is cancelled whatever ended the run:
        // dropping its future ends it, and nothing was learned about the
        // outcome.
        for action in started.in_flight {
            self.cancel_action(action.computation, &abort.diagnostics);
        }
        self.stop_live_frames(scheduler, &abort.diagnostics, abort.reason);
        Err(abort.diagnostics)
    }

    async fn drive_chains<'a, P: ActionPolicy, E: Executor, C: CancelSignal>(
        &mut self,
        scheduler: &mut Scheduler,
        policy: &P,
        executor: &'a E,
        cancel: &C,
        bound: &RunBound,
        started: &mut StartedActions<'a>,
    ) -> Result<(), RunAbort> {
        // Revalidating a recorded action edge re-plans the request behind it
        // and shows the contract to this run's policy, so the reuse path
        // needs both for as long as the run lasts.
        let environment = executor.identity();
        let context = ReuseContext::Run {
            policy,
            environment: &environment,
        };
        let mut steps = bound.step_budget();
        let mut waiting: VecDeque<(ChainId, Request<Action>)> = VecDeque::new();
        loop {
            while let Some(chain) = scheduler.next_ready() {
                if cancel.is_cancelled() {
                    return Err(cancelled_abort());
                }
                if bound.deadline_exceeded() {
                    return Err(bound_abort());
                }
                match self
                    .advance_chain_run(scheduler, chain, &context, &mut steps, bound)
                    .await?
                {
                    ChainPause::Settled => {}
                    ChainPause::Blob(id) => self.serve_blob(scheduler, chain, id)?,
                    ChainPause::Action(request) => waiting.push_back((chain, request)),
                    ChainPause::Observation(request) => {
                        let serving = self.serve_observation(&request, bound).await?;
                        let parent = scheduler.top(chain)?.computation;
                        self.record_edge(
                            parent,
                            DependencyEdge::Observation {
                                computation: serving.computation,
                                request,
                            },
                        )?;
                        scheduler.resume(chain, Resumption::One(serving.value))?;
                    }
                }
            }

            while started.in_flight.len() < self.action_concurrency().get() {
                let Some((chain, request)) = waiting.pop_front() else {
                    break;
                };
                let serving = Serving {
                    context: &context,
                    bound: *bound,
                };
                self.start_action(scheduler, chain, request, executor, &serving, started)?;
            }
            // A reused action has no future to finish, so nothing else will
            // wake the chain that asked for it.
            let woke_a_chain = !started.resumed.is_empty();
            while let Some((chain, value)) = started.resumed.pop_front() {
                scheduler.resume(chain, Resumption::One(value))?;
            }

            if started.in_flight.is_empty() {
                if woke_a_chain {
                    continue;
                }
                // Nothing is running, so nothing will free a slot. The queue
                // is empty too: while the concurrency limit is non-zero the
                // starting loop launches at least one action, so a queued
                // action cannot be left here.
                return Ok(());
            }
            let (index, captured) = first_finished(&mut started.in_flight).await;
            let finished = started.in_flight.swap_remove(index);
            // Checked after the await as well as before stepping a chain: an
            // action can be the only thing a long run is doing, and a caller
            // that cancelled while it ran should not wait for the next one.
            // The deadline is checked here for the same reason, and covers an
            // executor that ignored the deadline it was handed: a first-party
            // executor kills the child at it and refuses, which arrives as the
            // bound's diagnostic.
            if cancel.is_cancelled() {
                self.cancel_action(finished.computation, &cancelled_diag());
                return Err(cancelled_abort());
            }
            if bound.deadline_exceeded() {
                self.cancel_action(finished.computation, &wall_bound_diag());
                return Err(bound_abort());
            }
            let value = self.finish_action(
                finished.computation,
                &finished.request,
                &finished.spec,
                &finished.rule_meta,
                captured,
            )?;
            scheduler.resume(finished.chain, Resumption::One(value))?;
        }
    }

    /// Plan an action and hand it to the executor, parking `chain` until it
    /// returns. The dependency edge is recorded now rather than on completion,
    /// so the graph shows what a running action belongs to.
    fn start_action<'a, E: Executor>(
        &mut self,
        scheduler: &Scheduler,
        chain: ChainId,
        request: Request<Action>,
        executor: &'a E,
        serving: &Serving<'_>,
        started: &mut StartedActions<'a>,
    ) -> PithResult<()> {
        let parent = scheduler.top(chain)?.computation;
        match self.begin_action(&request, serving.context) {
            ActionStart::PlanningFailed(diagnostics) => Err(diagnostics),
            ActionStart::Refused {
                computation,
                diagnostics,
            } => {
                self.record_edge(
                    parent,
                    DependencyEdge::Action {
                        computation,
                        request,
                    },
                )?;
                Err(diagnostics)
            }
            ActionStart::Reused(evaluation) => {
                self.record_edge(
                    parent,
                    DependencyEdge::Action {
                        computation: evaluation.computation,
                        request,
                    },
                )?;
                started.resumed.push_back((chain, evaluation.value));
                Ok(())
            }
            ActionStart::Ready(prepared) => {
                let PreparedAction {
                    computation,
                    spec,
                    rule_meta,
                    mut invocation,
                } = *prepared;
                self.record_edge(
                    parent,
                    DependencyEdge::Action {
                        computation,
                        request: request.clone(),
                    },
                )?;
                invocation.deadline = serving.bound.action_deadline();
                started.in_flight.push(InFlightAction {
                    chain,
                    computation,
                    request,
                    spec,
                    rule_meta,
                    execution: Box::pin(async move { executor.execute(&invocation).await }),
                });
                Ok(())
            }
        }
    }
}
