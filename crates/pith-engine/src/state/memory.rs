use std::sync::{Arc, Mutex, MutexGuard};

use indexmap::{IndexMap, IndexSet};
use pith_core::PureComputationKey;

use super::validate::{AttemptLookup, TerminalAttemptState, validate_publication};
use super::{
    CURRENT_ENGINE_STATE_VERSIONS, CompletedAttempt, DurableAttempt, DurableAttemptId,
    DurableAttemptState, DurableComputation, EngineStateError, EngineStateStore,
    EngineStateVersions, FailedAttempt,
};

/// Deterministic in-memory implementation of [`EngineStateStore`].
pub struct MemoryEngineStateStore {
    versions: EngineStateVersions,
    records: Mutex<Records>,
}

#[derive(Default)]
struct Records {
    next_attempt_identifier: u64,
    attempts: IndexMap<DurableAttemptId, Arc<DurableAttempt>>,
    pure_history: IndexMap<PureComputationKey, Vec<DurableAttemptId>>,
    latest_reusable: IndexMap<PureComputationKey, DurableAttemptId>,
    pending: IndexSet<DurableAttemptId>,
}

impl AttemptLookup for Records {
    fn lookup(
        &self,
        attempt: DurableAttemptId,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        Ok(self.attempts.get(&attempt).cloned())
    }
}

impl MemoryEngineStateStore {
    pub fn new(versions: EngineStateVersions) -> Self {
        Self {
            versions,
            records: Mutex::new(Records {
                next_attempt_identifier: 1,
                ..Records::default()
            }),
        }
    }

    fn locked(&self) -> Result<MutexGuard<'_, Records>, EngineStateError> {
        self.records.lock().map_err(|_| EngineStateError::Adapter {
            message: "the in-memory engine state was poisoned by a panic".into(),
        })
    }

    fn publish(
        &self,
        attempt: DurableAttemptId,
        terminal_state: TerminalAttemptState,
    ) -> Result<(), EngineStateError> {
        let mut records = self.locked()?;
        let Some(pending_attempt) = records.attempts.get(&attempt) else {
            return Err(EngineStateError::AttemptNotFound { attempt });
        };
        if !matches!(pending_attempt.state, DurableAttemptState::Pending) {
            return Err(EngineStateError::AttemptNotPending {
                attempt,
                status: pending_attempt.state.status(),
            });
        }
        let computation = pending_attempt.computation.clone();
        validate_publication(&*records, attempt, &computation, &terminal_state)?;
        let reusable_computation = terminal_state.reusable_computation(&computation);
        let terminal_attempt = Arc::new(DurableAttempt {
            id: attempt,
            computation,
            state: terminal_state.into_attempt_state(),
        });

        let _ = records.attempts.insert(attempt, terminal_attempt);
        let _ = records.pending.shift_remove(&attempt);
        if let Some(computation) = reusable_computation {
            records.latest_reusable.insert(computation, attempt);
        }
        Ok(())
    }
}

impl Records {
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
        &self,
        computation: DurableComputation,
    ) -> Result<DurableAttemptId, EngineStateError> {
        let mut records = self.locked()?;
        let Some(next_identifier) = records.next_attempt_identifier.checked_add(1) else {
            return Err(EngineStateError::AttemptIdentifierExhausted);
        };
        let attempt = DurableAttemptId::from_raw(records.next_attempt_identifier);
        records.next_attempt_identifier = next_identifier;

        if let Some(computation) = computation.pure_key() {
            records
                .pure_history
                .entry(computation)
                .or_default()
                .push(attempt);
        }
        let _ = records.pending.insert(attempt);
        let _ = records.attempts.insert(
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
        &self,
        attempt: DurableAttemptId,
        completion: CompletedAttempt,
    ) -> Result<(), EngineStateError> {
        self.publish(attempt, TerminalAttemptState::Complete(completion))
    }

    fn publish_failed(
        &self,
        attempt: DurableAttemptId,
        failure: FailedAttempt,
    ) -> Result<(), EngineStateError> {
        self.publish(attempt, TerminalAttemptState::Failed(failure))
    }

    fn attempt(
        &self,
        attempt: DurableAttemptId,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        Ok(self.locked()?.attempts.get(&attempt).cloned())
    }

    fn attempt_history(
        &self,
        computation: PureComputationKey,
    ) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        let records = self.locked()?;
        let Some(attempts) = records.pure_history.get(&computation) else {
            return Ok(Box::new([]));
        };
        records.attempts_by_id(attempts.iter().copied())
    }

    fn latest_completed_reusable_attempt(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        let records = self.locked()?;
        let Some(attempt) = records.latest_reusable.get(&computation) else {
            return Ok(None);
        };
        let Some(record) = records.attempts.get(attempt) else {
            return Err(EngineStateError::Adapter {
                message: format!("reusable index references missing attempt {attempt}").into(),
            });
        };
        Ok(Some(record.clone()))
    }

    fn pending_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        let records = self.locked()?;
        let pending: Vec<_> = records.pending.iter().copied().collect();
        records.attempts_by_id(pending)
    }
}
