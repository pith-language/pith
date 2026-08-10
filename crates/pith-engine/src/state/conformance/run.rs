//! Applying a scenario to both stores.

use pith_core::Value;

use crate::MemoryEngineStateStore;
use crate::policy::ActionAuthorization;
use crate::state::{
    CompletedAttempt, DurableActionProvenance, DurableAttemptId, DurableComputation,
    DurableProvenance, EncodedValue, EngineStateStore, StoppedAttempt,
};

use super::compare::{Divergence, DivergenceDetail, compare_outcome};
use super::fixtures::{action_computation, diagnostics, imported_report, pure_key};
use super::record::{
    ReuseOutcome, Space, materialize, resolve_dependencies, reuse_decision, with_capability_edges,
};
use super::scenario::{GeneratedDependency, Selector, Step};
#[derive(Clone)]
pub(super) struct Tracked {
    pub(super) model: DurableAttemptId,
    pub(super) subject: DurableAttemptId,
    pub(super) computation: DurableComputation,
    pub(super) terminal: Option<TerminalKind>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TerminalKind {
    CompleteReusable,
    CompleteNotReusable,
    Failed,
    Cancelled,
}

impl TerminalKind {
    pub(super) const fn is_complete(self) -> bool {
        matches!(self, Self::CompleteReusable | Self::CompleteNotReusable)
    }
}
pub(super) fn run_step(
    index: usize,
    step: &Step,
    model: &MemoryEngineStateStore,
    subject: &dyn EngineStateStore,
    tracked: &mut Vec<Tracked>,
) -> Result<(), Divergence> {
    match step {
        Step::CreatePure { rule, input } => {
            let computation = DurableComputation::Pure(pure_key(*rule, *input));
            create(index, computation, model, subject, tracked)
        }
        Step::CreateAction {
            rule,
            executable,
            capabilities,
            denied,
        } => match action_computation(*rule, *executable, capabilities, *denied) {
            Some(computation) => create(index, computation, model, subject, tracked),
            // An invalid fixture contract is a generator bug, not an adapter
            // property.
            None => Ok(()),
        },
        Step::Complete {
            attempt,
            dependencies,
            result,
            corrupt_reuse,
        } => complete(
            index,
            *attempt,
            dependencies,
            *result,
            *corrupt_reuse,
            model,
            subject,
            tracked,
        ),
        Step::Stop {
            attempt,
            dependencies,
            message_len,
            notes,
            cancelled,
        } => stop(
            index,
            *attempt,
            dependencies,
            *message_len,
            *notes,
            *cancelled,
            model,
            subject,
            tracked,
        ),
        Step::RepublishTerminal { attempt } => {
            republish_terminal(index, *attempt, model, subject, tracked)
        }
    }
}

fn create(
    index: usize,
    computation: DurableComputation,
    model: &MemoryEngineStateStore,
    subject: &dyn EngineStateStore,
    tracked: &mut Vec<Tracked>,
) -> Result<(), Divergence> {
    let model_id = model.create_pending_attempt(computation.clone());
    let subject_id = subject.create_pending_attempt(computation.clone());
    match (model_id, subject_id) {
        (Ok(model), Ok(subject)) => {
            tracked.push(Tracked {
                model,
                subject,
                computation,
                terminal: None,
            });
            Ok(())
        }
        (model, subject) => Err(Divergence {
            step: index,
            detail: DivergenceDetail::Outcome {
                operation: "create_pending_attempt",
                model: model.map(|_| ()).map_err(|error| error.to_string()),
                subject: subject.map(|_| ()).map_err(|error| error.to_string()),
            },
        }),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the harness threads both stores, the tracking table, and the generated step's fields"
)]
fn complete(
    index: usize,
    selector: Selector,
    dependencies: &[GeneratedDependency],
    result: i64,
    corrupt_reuse: bool,
    model: &MemoryEngineStateStore,
    subject: &dyn EngineStateStore,
    tracked: &mut [Tracked],
) -> Result<(), Divergence> {
    let Some(position) = select_pending(selector, tracked) else {
        return Ok(());
    };
    let Some(target) = tracked.get(position).cloned() else {
        return Ok(());
    };

    // Shared validation rejects a complete attempt that depends on a failed one.
    let resolved = resolve_dependencies(dependencies, tracked, TerminalKind::is_complete);
    let provenance = match &target.computation {
        DurableComputation::Pure(_) => DurableProvenance::Pure,
        DurableComputation::Action { authorization, .. } => {
            if matches!(authorization, ActionAuthorization::Denied { .. }) {
                // A denied action cannot complete; `Stop` covers that path.
                return Ok(());
            }
            DurableProvenance::Action(DurableActionProvenance::Imported {
                imported_report: imported_report(&target.computation),
            })
        }
    };
    let reuse = reuse_decision(&target.computation, &resolved, tracked, corrupt_reuse);
    let completion = |space: Space| CompletedAttempt {
        dependencies: with_capability_edges(materialize(&resolved, tracked, space), &provenance),
        result: EncodedValue::from_value(&Value::Int(result)),
        provenance: provenance.clone(),
        reuse: reuse.materialize(tracked, space),
    };

    let model_outcome = model.publish_complete(target.model, completion(Space::Model));
    let subject_outcome = subject.publish_complete(target.subject, completion(Space::Subject));
    compare_outcome(
        index,
        "publish_complete",
        &model_outcome,
        &subject_outcome,
        tracked,
    )?;

    if model_outcome.is_ok()
        && let Some(entry) = tracked.get_mut(position)
    {
        entry.terminal = Some(match reuse {
            ReuseOutcome::Reusable => TerminalKind::CompleteReusable,
            ReuseOutcome::NotReusable(_) => TerminalKind::CompleteNotReusable,
        });
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "the harness threads both stores, the tracking table, and the generated step's fields"
)]
fn stop(
    index: usize,
    selector: Selector,
    dependencies: &[GeneratedDependency],
    message_len: u8,
    notes: u8,
    cancelled: bool,
    model: &MemoryEngineStateStore,
    subject: &dyn EngineStateStore,
    tracked: &mut [Tracked],
) -> Result<(), Divergence> {
    let Some(position) = select_pending(selector, tracked) else {
        return Ok(());
    };
    let Some(target) = tracked.get(position).cloned() else {
        return Ok(());
    };

    let resolved = resolve_dependencies(dependencies, tracked, |_| true);
    let provenance = match &target.computation {
        DurableComputation::Pure(_) => DurableProvenance::Pure,
        DurableComputation::Action { .. } => {
            DurableProvenance::Action(DurableActionProvenance::NotExecuted)
        }
    };
    let diagnostics = diagnostics(message_len, notes);
    let stopped = |space: Space| StoppedAttempt {
        dependencies: with_capability_edges(materialize(&resolved, tracked, space), &provenance),
        diagnostics: diagnostics.clone(),
        provenance: provenance.clone(),
    };

    let (operation, model_outcome, subject_outcome) = if cancelled {
        (
            "publish_cancelled",
            model.publish_cancelled(target.model, stopped(Space::Model)),
            subject.publish_cancelled(target.subject, stopped(Space::Subject)),
        )
    } else {
        (
            "publish_failed",
            model.publish_failed(target.model, stopped(Space::Model)),
            subject.publish_failed(target.subject, stopped(Space::Subject)),
        )
    };
    compare_outcome(index, operation, &model_outcome, &subject_outcome, tracked)?;

    if model_outcome.is_ok()
        && let Some(entry) = tracked.get_mut(position)
    {
        entry.terminal = Some(if cancelled {
            TerminalKind::Cancelled
        } else {
            TerminalKind::Failed
        });
    }
    Ok(())
}

fn republish_terminal(
    index: usize,
    selector: Selector,
    model: &MemoryEngineStateStore,
    subject: &dyn EngineStateStore,
    tracked: &[Tracked],
) -> Result<(), Divergence> {
    let terminal: Vec<&Tracked> = tracked
        .iter()
        .filter(|entry| entry.terminal.is_some())
        .collect();
    let Some(target) = pick(selector, &terminal) else {
        return Ok(());
    };
    let failure = StoppedAttempt {
        dependencies: Box::new([]),
        diagnostics: Box::new([]),
        provenance: match &target.computation {
            DurableComputation::Pure(_) => DurableProvenance::Pure,
            DurableComputation::Action { .. } => {
                DurableProvenance::Action(DurableActionProvenance::NotExecuted)
            }
        },
    };
    let model_outcome = model.publish_failed(target.model, failure.clone());
    let subject_outcome = subject.publish_failed(target.subject, failure);
    compare_outcome(
        index,
        "publish_failed over a terminal attempt",
        &model_outcome,
        &subject_outcome,
        tracked,
    )
}

pub(super) fn pick<T>(selector: Selector, candidates: &[T]) -> Option<&T> {
    candidates.get(usize::from(selector).checked_rem(candidates.len())?)
}

fn select_pending(selector: Selector, tracked: &[Tracked]) -> Option<usize> {
    let pending: Vec<usize> = tracked
        .iter()
        .enumerate()
        .filter_map(|(position, entry)| entry.terminal.is_none().then_some(position))
        .collect();
    pick(selector, &pending).copied()
}
