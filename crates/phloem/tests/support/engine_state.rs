use std::sync::{Arc, Mutex};

use pith_core::{ActionComputationKey, PureComputationKey};
use pith_engine::state::{
    CompletedAttempt, DurableAttempt, DurableAttemptId, DurableComputation, EngineStateError,
    EngineStateStore, EngineStateVersions, InvalidationExplanation, MemoryEngineStateStore,
    StoppedAttempt,
};

#[derive(Clone, Default)]
pub struct SharedState(Arc<Mutex<MemoryEngineStateStore>>);

impl SharedState {
    pub fn read<T>(
        &self,
        read: impl FnOnce(&MemoryEngineStateStore) -> Result<T, EngineStateError>,
    ) -> Result<T, EngineStateError> {
        match self.0.lock() {
            Ok(store) => read(&store),
            Err(_) => Err(lock_poisoned()),
        }
    }

    fn write<T>(
        &self,
        write: impl FnOnce(&mut MemoryEngineStateStore) -> Result<T, EngineStateError>,
    ) -> Result<T, EngineStateError> {
        match self.0.lock() {
            Ok(mut store) => write(&mut store),
            Err(_) => Err(lock_poisoned()),
        }
    }
}

fn lock_poisoned() -> EngineStateError {
    EngineStateError::Adapter {
        message: "fixture: shared engine state lock was poisoned".into(),
    }
}

impl EngineStateStore for SharedState {
    fn versions(&self) -> EngineStateVersions {
        match self.0.lock() {
            Ok(store) => store.versions(),
            Err(_) => unreachable!("the shared engine state lock was poisoned"),
        }
    }

    fn create_pending_attempt(
        &self,
        computation: DurableComputation,
    ) -> Result<DurableAttemptId, EngineStateError> {
        self.write(|store| store.create_pending_attempt(computation))
    }

    fn publish_complete(
        &self,
        attempt: DurableAttemptId,
        completion: CompletedAttempt,
    ) -> Result<(), EngineStateError> {
        self.write(|store| store.publish_complete(attempt, completion))
    }

    fn publish_failed(
        &self,
        attempt: DurableAttemptId,
        failure: StoppedAttempt,
    ) -> Result<(), EngineStateError> {
        self.write(|store| store.publish_failed(attempt, failure))
    }

    fn publish_cancelled(
        &self,
        attempt: DurableAttemptId,
        cancellation: StoppedAttempt,
    ) -> Result<(), EngineStateError> {
        self.write(|store| store.publish_cancelled(attempt, cancellation))
    }

    fn attempt(
        &self,
        attempt: DurableAttemptId,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.read(|store| store.attempt(attempt))
    }

    fn attempt_history(
        &self,
        computation: PureComputationKey,
    ) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.read(|store| store.attempt_history(computation))
    }

    fn latest_completed_reusable_attempt(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.read(|store| store.latest_completed_reusable_attempt(computation))
    }

    fn latest_completed_reusable_action_attempt(
        &self,
        computation: ActionComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.read(|store| store.latest_completed_reusable_action_attempt(computation))
    }

    fn explain_invalidation(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<InvalidationExplanation>, EngineStateError> {
        self.read(|store| store.explain_invalidation(computation))
    }

    fn pending_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.read(|store| store.pending_attempts())
    }
}
