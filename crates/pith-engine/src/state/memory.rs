use std::sync::{Arc, Mutex, MutexGuard};

use indexmap::{IndexMap, IndexSet};
use pith_core::{ActionComputationKey, PureComputationKey};

use super::validate::{AttemptLookup, TerminalAttemptState, validate_publication};
use super::{
    AttemptStatistics, CURRENT_ENGINE_STATE_VERSIONS, CompletedAttempt, DurableAttempt,
    DurableAttemptId, DurableAttemptState, DurableComputation, EngineStateError, EngineStateReader,
    EngineStateStore, EngineStateVersions, InvalidationExplanation, StoppedAttempt,
};

/// Deterministic in-memory implementation of [`EngineStateStore`].
#[derive(Clone)]
pub struct MemoryEngineStateStore {
    versions: EngineStateVersions,
    records: Arc<Mutex<Records>>,
}

#[derive(Default)]
struct Records {
    next_attempt_identifier: u64,
    attempts: IndexMap<DurableAttemptId, Arc<DurableAttempt>>,
    pure_history: IndexMap<PureComputationKey, Vec<DurableAttemptId>>,
    latest_reusable: IndexMap<PureComputationKey, DurableAttemptId>,
    latest_reusable_action: IndexMap<ActionComputationKey, DurableAttemptId>,
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
            records: Arc::new(Mutex::new(Records {
                next_attempt_identifier: 1,
                ..Records::default()
            })),
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
        let reusable = terminal_state.is_reusable().then(|| computation.clone());
        let terminal_attempt = Arc::new(DurableAttempt {
            id: attempt,
            computation,
            state: terminal_state.into_attempt_state(),
        });

        let _ = records.attempts.insert(attempt, terminal_attempt);
        let _ = records.pending.shift_remove(&attempt);
        match reusable {
            Some(DurableComputation::Pure(key)) => {
                records.latest_reusable.insert(key, attempt);
            }
            Some(computation @ DurableComputation::Action { .. }) => {
                if let Some(key) = computation.action_key() {
                    records.latest_reusable_action.insert(key, attempt);
                }
            }
            Some(DurableComputation::Observation { .. }) => {}
            None => {}
        }
        Ok(())
    }
}

impl Records {
    /// Resolve an entry the reusable index produced. The index only ever names
    /// attempts this store published, so a missing record is an adapter fault
    /// and not an empty answer.
    fn indexed_attempt(
        &self,
        attempt: Option<DurableAttemptId>,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        let Some(attempt) = attempt else {
            return Ok(None);
        };
        match self.attempts.get(&attempt) {
            Some(record) => Ok(Some(record.clone())),
            None => Err(EngineStateError::Adapter {
                message: format!("reusable index references missing attempt {attempt}").into(),
            }),
        }
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
}

impl Default for MemoryEngineStateStore {
    fn default() -> Self {
        Self::new(CURRENT_ENGINE_STATE_VERSIONS)
    }
}

impl EngineStateReader for MemoryEngineStateStore {
    fn versions(&self) -> EngineStateVersions {
        self.versions
    }

    fn attempt_statistics(&self) -> Result<AttemptStatistics, EngineStateError> {
        let records = self.locked()?;
        let mut statistics = AttemptStatistics::default();
        for attempt in records.attempts.values() {
            statistics.record(attempt.state.status());
        }
        statistics.reusable_index = u64::try_from(
            records
                .latest_reusable
                .len()
                .saturating_add(records.latest_reusable_action.len()),
        )
        .unwrap_or(u64::MAX);
        Ok(statistics)
    }

    fn all_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        let records = self.locked()?;
        Ok(records.attempts.values().cloned().collect())
    }

    fn reusable_index_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        let records = self.locked()?;
        let mut indexed: Vec<DurableAttemptId> = records
            .latest_reusable
            .values()
            .chain(records.latest_reusable_action.values())
            .copied()
            .collect();
        indexed.sort_unstable();
        records.attempts_by_id(indexed)
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
        let indexed = records.latest_reusable.get(&computation).copied();
        records.indexed_attempt(indexed)
    }

    fn latest_completed_reusable_action_attempt(
        &self,
        computation: ActionComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        let records = self.locked()?;
        let indexed = records.latest_reusable_action.get(&computation).copied();
        records.indexed_attempt(indexed)
    }

    fn explain_invalidation(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<InvalidationExplanation>, EngineStateError> {
        let records = self.locked()?;
        let indexed = records.latest_reusable.get(&computation).copied();
        let latest = records.indexed_attempt(indexed)?;
        super::explain::explain_latest(&*records, latest)
    }

    fn pending_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        let records = self.locked()?;
        let pending: Vec<_> = records.pending.iter().copied().collect();
        records.attempts_by_id(pending)
    }
}

impl EngineStateStore for MemoryEngineStateStore {
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
        failure: StoppedAttempt,
    ) -> Result<(), EngineStateError> {
        self.publish(attempt, TerminalAttemptState::Failed(failure))
    }

    fn publish_cancelled(
        &self,
        attempt: DurableAttemptId,
        cancellation: StoppedAttempt,
    ) -> Result<(), EngineStateError> {
        self.publish(attempt, TerminalAttemptState::Cancelled(cancellation))
    }
}
