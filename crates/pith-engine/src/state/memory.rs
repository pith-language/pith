use std::sync::Arc;

use indexmap::{IndexMap, IndexSet};
use pith_core::PureComputationKey;

use crate::ActionAuthorization;

use super::{
    CURRENT_ENGINE_STATE_VERSIONS, CompletedAttempt, DurableActionProvenance, DurableAttempt,
    DurableAttemptId, DurableAttemptState, DurableComputation, DurableDependency,
    DurableProvenance, DurableReuseDecision, DurableReuseReason, EngineStateError,
    EngineStateStore, EngineStateVersions, FailedAttempt, InvalidActionLifecycleReason,
    InvalidDependencyReason,
};

enum TerminalAttemptState {
    Complete(CompletedAttempt),
    Failed(FailedAttempt),
}

impl TerminalAttemptState {
    fn provenance(&self) -> &DurableProvenance {
        match self {
            Self::Complete(completion) => &completion.provenance,
            Self::Failed(failure) => &failure.provenance,
        }
    }

    fn dependencies(&self) -> &[DurableDependency] {
        match self {
            Self::Complete(completion) => &completion.dependencies,
            Self::Failed(failure) => &failure.dependencies,
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self, Self::Complete(_))
    }

    fn into_attempt_state(self) -> DurableAttemptState {
        match self {
            Self::Complete(completion) => DurableAttemptState::Complete(completion),
            Self::Failed(failure) => DurableAttemptState::Failed(failure),
        }
    }
}

/// Deterministic in-memory implementation of [`EngineStateStore`].
pub struct MemoryEngineStateStore {
    versions: EngineStateVersions,
    next_attempt_identifier: u64,
    attempts: IndexMap<DurableAttemptId, Arc<DurableAttempt>>,
    pure_history: IndexMap<PureComputationKey, Vec<DurableAttemptId>>,
    latest_reusable: IndexMap<PureComputationKey, DurableAttemptId>,
    pending: IndexSet<DurableAttemptId>,
}

impl MemoryEngineStateStore {
    pub fn new(versions: EngineStateVersions) -> Self {
        Self {
            versions,
            next_attempt_identifier: 1,
            attempts: IndexMap::new(),
            pure_history: IndexMap::new(),
            latest_reusable: IndexMap::new(),
            pending: IndexSet::new(),
        }
    }

    fn publish(
        &mut self,
        attempt: DurableAttemptId,
        terminal_state: TerminalAttemptState,
    ) -> Result<(), EngineStateError> {
        let Some(pending_attempt) = self.attempts.get(&attempt) else {
            return Err(EngineStateError::AttemptNotFound { attempt });
        };
        if !matches!(pending_attempt.state, DurableAttemptState::Pending) {
            return Err(EngineStateError::AttemptNotPending {
                attempt,
                status: pending_attempt.state.status(),
            });
        }
        self.validate_publication(attempt, &pending_attempt.computation, &terminal_state)?;

        let reusable_computation = match (&pending_attempt.computation, &terminal_state) {
            (
                DurableComputation::Pure(computation),
                TerminalAttemptState::Complete(CompletedAttempt {
                    reuse: DurableReuseDecision::Reusable,
                    ..
                }),
            ) => Some(*computation),
            _ => None,
        };
        let terminal_attempt = Arc::new(DurableAttempt {
            id: attempt,
            computation: pending_attempt.computation.clone(),
            state: terminal_state.into_attempt_state(),
        });

        let _ = self.attempts.insert(attempt, terminal_attempt);
        let _ = self.pending.shift_remove(&attempt);
        if let Some(computation) = reusable_computation {
            self.latest_reusable.insert(computation, attempt);
        }
        Ok(())
    }

    fn attempts_by_id(
        &self,
        identifiers: impl IntoIterator<Item = DurableAttemptId>,
    ) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        identifiers
            .into_iter()
            .map(|attempt| {
                self.attempts
                    .get(&attempt)
                    .cloned()
                    .ok_or_else(|| EngineStateError::Adapter {
                        message: format!("engine-state index references missing attempt {attempt}")
                            .into(),
                    })
            })
            .collect()
    }

    fn validate_publication(
        &self,
        attempt: DurableAttemptId,
        computation: &DurableComputation,
        terminal_state: &TerminalAttemptState,
    ) -> Result<(), EngineStateError> {
        validate_provenance_category(attempt, computation, terminal_state.provenance())?;
        validate_action_lifecycle(attempt, computation, terminal_state)?;

        for dependency in terminal_state.dependencies() {
            self.validate_dependency(attempt, terminal_state, dependency)?;
        }
        Ok(())
    }

    fn validate_dependency(
        &self,
        attempt: DurableAttemptId,
        terminal_state: &TerminalAttemptState,
        dependency: &DurableDependency,
    ) -> Result<(), EngineStateError> {
        let dependency_attempt = match dependency {
            DurableDependency::Pure { attempt, .. } | DurableDependency::Action { attempt } => {
                *attempt
            }
            DurableDependency::Blob { .. } | DurableDependency::CapabilityUse { .. } => {
                return Ok(());
            }
        };
        let invalid = |reason| EngineStateError::InvalidDependency {
            attempt,
            dependency: dependency_attempt,
            reason,
        };
        if attempt == dependency_attempt {
            return Err(invalid(InvalidDependencyReason::SelfReference));
        }
        let Some(dependency_record) = self.attempts.get(&dependency_attempt) else {
            return Err(invalid(InvalidDependencyReason::MissingAttempt));
        };
        match dependency_record.state {
            DurableAttemptState::Pending => {
                return Err(invalid(InvalidDependencyReason::PendingAttempt));
            }
            DurableAttemptState::Failed(_) if terminal_state.is_complete() => {
                return Err(invalid(
                    InvalidDependencyReason::FailedDependencyForCompleteAttempt,
                ));
            }
            DurableAttemptState::Complete(_) | DurableAttemptState::Failed(_) => {}
        }

        match dependency {
            DurableDependency::Pure { computation, .. } => match &dependency_record.computation {
                DurableComputation::Pure(actual) if computation == actual => Ok(()),
                DurableComputation::Pure(_) => {
                    Err(invalid(InvalidDependencyReason::PureComputationMismatch))
                }
                DurableComputation::Action { .. } => {
                    Err(invalid(InvalidDependencyReason::ExpectedPureAttempt))
                }
            },
            DurableDependency::Action { .. } => match &dependency_record.computation {
                DurableComputation::Action { .. } => Ok(()),
                DurableComputation::Pure(_) => {
                    Err(invalid(InvalidDependencyReason::ExpectedActionAttempt))
                }
            },
            DurableDependency::Blob { .. } | DurableDependency::CapabilityUse { .. } => Ok(()),
        }
    }
}

fn validate_provenance_category(
    attempt: DurableAttemptId,
    computation: &DurableComputation,
    provenance: &DurableProvenance,
) -> Result<(), EngineStateError> {
    if matches!(
        (computation, provenance),
        (DurableComputation::Pure(_), DurableProvenance::Pure)
            | (
                DurableComputation::Action { .. },
                DurableProvenance::Action(_)
            )
    ) {
        Ok(())
    } else {
        Err(EngineStateError::ProvenanceCategoryMismatch { attempt })
    }
}

fn validate_action_lifecycle(
    attempt: DurableAttemptId,
    computation: &DurableComputation,
    terminal_state: &TerminalAttemptState,
) -> Result<(), EngineStateError> {
    let DurableComputation::Action { authorization, .. } = computation else {
        return Ok(());
    };
    let DurableProvenance::Action(action_provenance) = terminal_state.provenance() else {
        return Ok(());
    };
    let invalid = |reason| EngineStateError::InvalidActionLifecycle { attempt, reason };

    if terminal_state.is_complete() && matches!(authorization, ActionAuthorization::Denied { .. }) {
        return Err(invalid(InvalidActionLifecycleReason::DeniedActionCompleted));
    }
    if matches!(authorization, ActionAuthorization::Denied { .. })
        && !matches!(action_provenance, DurableActionProvenance::NotExecuted)
    {
        return Err(invalid(
            InvalidActionLifecycleReason::DeniedActionHasExecutorReport,
        ));
    }
    if let TerminalAttemptState::Complete(CompletedAttempt { reuse, .. }) = terminal_state {
        match action_provenance {
            DurableActionProvenance::NotExecuted => {
                return Err(invalid(
                    InvalidActionLifecycleReason::CompletedActionMissingExecutorReport,
                ));
            }
            DurableActionProvenance::Captured { .. } => {
                return Err(invalid(
                    InvalidActionLifecycleReason::CompletedActionMissingImportedReport,
                ));
            }
            DurableActionProvenance::Imported { .. } => {}
        }
        if *reuse != DurableReuseDecision::NotReusable(DurableReuseReason::ActionCachingDisabled) {
            return Err(invalid(InvalidActionLifecycleReason::ActionMarkedReusable));
        }
    }
    Ok(())
}

impl Default for MemoryEngineStateStore {
    fn default() -> Self {
        Self::new(CURRENT_ENGINE_STATE_VERSIONS)
    }
}

impl EngineStateStore for MemoryEngineStateStore {
    fn versions(&self) -> EngineStateVersions {
        self.versions
    }

    fn create_pending_attempt(
        &mut self,
        computation: DurableComputation,
    ) -> Result<DurableAttemptId, EngineStateError> {
        let Some(next_identifier) = self.next_attempt_identifier.checked_add(1) else {
            return Err(EngineStateError::AttemptIdentifierExhausted);
        };
        let attempt = DurableAttemptId::from_raw(self.next_attempt_identifier);
        self.next_attempt_identifier = next_identifier;

        if let Some(computation) = computation.pure_key() {
            self.pure_history
                .entry(computation)
                .or_default()
                .push(attempt);
        }
        let _ = self.pending.insert(attempt);
        let _ = self.attempts.insert(
            attempt,
            Arc::new(DurableAttempt {
                id: attempt,
                computation,
                state: DurableAttemptState::Pending,
            }),
        );
        Ok(attempt)
    }

    fn publish_complete(
        &mut self,
        attempt: DurableAttemptId,
        completion: CompletedAttempt,
    ) -> Result<(), EngineStateError> {
        self.publish(attempt, TerminalAttemptState::Complete(completion))
    }

    fn publish_failed(
        &mut self,
        attempt: DurableAttemptId,
        failure: FailedAttempt,
    ) -> Result<(), EngineStateError> {
        self.publish(attempt, TerminalAttemptState::Failed(failure))
    }

    fn attempt(
        &self,
        attempt: DurableAttemptId,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        Ok(self.attempts.get(&attempt).cloned())
    }

    fn attempt_history(
        &self,
        computation: PureComputationKey,
    ) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        let Some(attempts) = self.pure_history.get(&computation) else {
            return Ok(Box::new([]));
        };
        self.attempts_by_id(attempts.iter().copied())
    }

    fn latest_completed_reusable_attempt(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        let Some(attempt) = self.latest_reusable.get(&computation) else {
            return Ok(None);
        };
        let Some(record) = self.attempts.get(attempt) else {
            return Err(EngineStateError::Adapter {
                message: format!("reusable index references missing attempt {attempt}").into(),
            });
        };
        Ok(Some(record.clone()))
    }

    fn pending_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.attempts_by_id(self.pending.iter().copied())
    }
}
