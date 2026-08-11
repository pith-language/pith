//! The two drivers that turn ready chains into progress (decision 0022).
//!
//! Both loop over the scheduler's ready chains and hand each to
//! [`Engine::advance_chain`]. They differ only in what they do with a chain
//! that stopped for an effect: the pure driver rejects it, and the run driver
//! serves it — reading blob bytes inline, and handing actions to the executor
//! so that actions belonging to independent chains overlap.
//!
//! Overlap is bounded. A chain that stops for an action joins a queue, and the
//! run driver starts from that queue only while fewer than
//! [`Engine::action_concurrency`] actions are running. The queue holds requests,
//! not invocations: an action waiting for a slot has not been planned, has no
//! computation node, and has materialized nothing.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::task::Poll;

use pith_core::{Action, ActionSpec, Request, Value};
use pith_diag::PithResult;
use pith_ids::ComputationId;

use super::action_pipeline::{ActionRuleMeta, ActionStart, PreparedAction};
use super::diagnostics::{cancelled_diag, effectful_in_pure_diag};
use super::eval::ChainPause;
use super::ir::{Resumption, StopReason};
use super::scheduler::{ChainId, Scheduler};
use super::{DependencyEdge, Engine};
use crate::action::{CapturedActionExecution, Executor};
use crate::cancel::CancelSignal;
use crate::policy::ActionPolicy;

/// Why a run stopped early, carried so the driver can record the work it was
/// holding under the right terminal state. An ordinary `?` on a diagnostic is
/// a failure; cancellation is constructed deliberately.
struct RunAbort {
    reason: StopReason,
    diagnostics: pith_diag::DiagnosticSink,
}

impl From<pith_diag::DiagnosticSink> for RunAbort {
    fn from(diagnostics: pith_diag::DiagnosticSink) -> Self {
        Self {
            reason: StopReason::Failed,
            diagnostics,
        }
    }
}

/// One action handed to an executor, with everything the engine needs to finish
/// it when the executor returns. The requesting chain is parked meanwhile, so
/// other chains — including ones with actions of their own — keep running.
struct InFlightAction<'a> {
    chain: ChainId,
    computation: ComputationId,
    request: Request<Action>,
    spec: ActionSpec,
    rule_meta: ActionRuleMeta,
    execution: Pin<Box<dyn Future<Output = PithResult<CapturedActionExecution>> + Send + 'a>>,
}

/// Wait for the first in-flight action to finish, reporting its index. Each
/// pending execution registers its waker, so the driver wakes as soon as any of
/// them makes progress; the rescan costs the number of actions running at once,
/// which is the width of the graph rather than its size.
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

impl Engine {
    /// Drive every chain to completion without leaving the synchronous core.
    pub(super) fn drive_pure(&mut self, scheduler: &mut Scheduler) -> PithResult<()> {
        while let Some(chain) = scheduler.next_ready() {
            match self.advance_chain(scheduler, chain)? {
                ChainPause::Settled => {}
                ChainPause::Blob(_) | ChainPause::Action(_) => {
                    return Err(effectful_in_pure_diag());
                }
            }
        }
        Ok(())
    }

    /// Drive every chain to completion, serving the effects they stop for.
    ///
    /// A run that ends early — cancelled, or aborted by a diagnostic — leaves
    /// chains parked and actions in flight. Both are recorded here, under the
    /// terminal state that matches why the run ended, before the diagnostics
    /// propagate.
    pub(super) async fn drive_run<'a, P: ActionPolicy, E: Executor, C: CancelSignal>(
        &mut self,
        scheduler: &mut Scheduler,
        policy: &P,
        executor: &'a E,
        cancel: &C,
    ) -> PithResult<()> {
        let mut in_flight: Vec<InFlightAction<'a>> = Vec::new();
        let Err(abort) = self
            .drive_chains(scheduler, policy, executor, cancel, &mut in_flight)
            .await
        else {
            return Ok(());
        };
        // An action still running was stopped, not broken: dropping its future
        // is what ends it, and nothing was learned about whether it would have
        // succeeded. That is cancellation whatever ended the run.
        for action in in_flight {
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
        in_flight: &mut Vec<InFlightAction<'a>>,
    ) -> Result<(), RunAbort> {
        let mut waiting: VecDeque<(ChainId, Request<Action>)> = VecDeque::new();
        loop {
            while let Some(chain) = scheduler.next_ready() {
                if cancel.is_cancelled() {
                    return Err(cancelled_abort());
                }
                match self.advance_chain(scheduler, chain)? {
                    ChainPause::Settled => {}
                    ChainPause::Blob(id) => {
                        let bytes = self.fetch_blob(id)?;
                        let parent = scheduler.top(chain)?.computation;
                        self.record_edge(parent, DependencyEdge::Blob { id })?;
                        scheduler.resume(chain, Resumption::One(Value::Bytes(bytes)))?;
                    }
                    ChainPause::Action(request) => waiting.push_back((chain, request)),
                }
            }

            while in_flight.len() < self.action_concurrency().get() {
                let Some((chain, request)) = waiting.pop_front() else {
                    break;
                };
                self.start_action(scheduler, chain, request, policy, executor, in_flight)?;
            }

            if in_flight.is_empty() {
                // Nothing is running, so nothing will free a slot. The queue is
                // empty too: the loop above starts at least one action while the
                // limit is non-zero, so a queued action cannot be left here.
                return Ok(());
            }
            let (index, captured) = first_finished(in_flight).await;
            let finished = in_flight.swap_remove(index);
            // Checked after the await as well as before stepping a chain: an
            // action can be the only thing a long run is doing, and a caller
            // that cancelled while it ran should not wait for the next one.
            if cancel.is_cancelled() {
                self.cancel_action(finished.computation, &cancelled_diag());
                return Err(cancelled_abort());
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
    fn start_action<'a, P: ActionPolicy, E: Executor>(
        &mut self,
        scheduler: &Scheduler,
        chain: ChainId,
        request: Request<Action>,
        policy: &P,
        executor: &'a E,
        in_flight: &mut Vec<InFlightAction<'a>>,
    ) -> PithResult<()> {
        let parent = scheduler.top(chain)?.computation;
        match self.begin_action(&request, policy) {
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
            ActionStart::Ready(prepared) => {
                let PreparedAction {
                    computation,
                    spec,
                    rule_meta,
                    invocation,
                } = *prepared;
                self.record_edge(
                    parent,
                    DependencyEdge::Action {
                        computation,
                        request: request.clone(),
                    },
                )?;
                in_flight.push(InFlightAction {
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
