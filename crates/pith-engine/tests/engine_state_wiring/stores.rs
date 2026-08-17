use super::*;

/// A content store several engines share, the way a filesystem store is shared
/// by successive runs of a build.
#[derive(Clone, Default)]
pub struct SharedContentStore(Arc<std::sync::Mutex<MemoryContentStore>>);

impl SharedContentStore {
    pub fn put_fixture(&self, bytes: &[u8]) -> ContentId {
        match self.0.lock() {
            Ok(mut store) => put_fixture_blob(&mut store, bytes),
            Err(_) => unreachable!("the shared content store was poisoned"),
        }
    }
}

impl ContentStore for SharedContentStore {
    fn put_blob(&mut self, bytes: &[u8]) -> Result<ContentId, pith_store::StoreError> {
        match self.0.lock() {
            Ok(mut store) => store.put_blob(bytes),
            Err(_) => Err(pith_store::StoreError::new("shared content store poisoned")),
        }
    }

    fn get_blob(&self, id: ContentId) -> Result<Option<pith_store::Blob>, pith_store::StoreError> {
        match self.0.lock() {
            Ok(store) => store.get_blob(id),
            Err(_) => Err(pith_store::StoreError::new("shared content store poisoned")),
        }
    }

    fn put_tree(&mut self, tree: pith_store::Tree) -> Result<ContentId, pith_store::StoreError> {
        match self.0.lock() {
            Ok(mut store) => store.put_tree(tree),
            Err(_) => Err(pith_store::StoreError::new("shared content store poisoned")),
        }
    }

    fn get_tree(&self, id: ContentId) -> Result<Option<pith_store::Tree>, pith_store::StoreError> {
        match self.0.lock() {
            Ok(store) => store.get_tree(id),
            Err(_) => Err(pith_store::StoreError::new("shared content store poisoned")),
        }
    }
}

/// A store adapter whose `create_pending_attempt` always fails. Used to
/// exercise the error-hygiene path where the live engine cannot create a
/// durable attempt: the arena must not be left with an orphaned `Pending` node.
pub struct CreateFailingStore {
    pub inner: MemoryEngineStateStore,
}

impl CreateFailingStore {
    fn failure() -> EngineStateError {
        EngineStateError::Adapter {
            message: "fixture: create_pending_attempt disabled".into(),
        }
    }
}

impl EngineStateStore for CreateFailingStore {
    fn versions(&self) -> EngineStateVersions {
        self.inner.versions()
    }

    fn create_pending_attempt(
        &self,
        _computation: DurableComputation,
    ) -> Result<DurableAttemptId, EngineStateError> {
        Err(Self::failure())
    }

    fn publish_complete(
        &self,
        attempt: DurableAttemptId,
        completion: CompletedAttempt,
    ) -> Result<(), EngineStateError> {
        self.inner.publish_complete(attempt, completion)
    }

    fn publish_failed(
        &self,
        attempt: DurableAttemptId,
        failure: StoppedAttempt,
    ) -> Result<(), EngineStateError> {
        self.inner.publish_failed(attempt, failure)
    }

    fn publish_cancelled(
        &self,
        attempt: DurableAttemptId,
        cancellation: StoppedAttempt,
    ) -> Result<(), EngineStateError> {
        self.inner.publish_cancelled(attempt, cancellation)
    }

    fn attempt(
        &self,
        attempt: DurableAttemptId,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.inner.attempt(attempt)
    }

    fn attempt_history(
        &self,
        computation: PureComputationKey,
    ) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.inner.attempt_history(computation)
    }

    fn latest_completed_reusable_attempt(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.inner.latest_completed_reusable_attempt(computation)
    }

    fn latest_completed_reusable_action_attempt(
        &self,
        computation: ActionComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.inner
            .latest_completed_reusable_action_attempt(computation)
    }

    fn explain_invalidation(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<InvalidationExplanation>, EngineStateError> {
        self.inner.explain_invalidation(computation)
    }

    fn pending_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.inner.pending_attempts()
    }
}

/// A store adapter whose reusable-index read always fails. Decision 0024 treats
/// adapter failure as an error rather than a cache miss, so a broken adapter
/// must surface diagnostics instead of silently degrading into "recompute".
#[derive(Default)]
pub struct ReadFailingStore {
    inner: MemoryEngineStateStore,
}

impl ReadFailingStore {
    fn failure() -> EngineStateError {
        EngineStateError::Adapter {
            message: "fixture: reusable index unreadable".into(),
        }
    }
}

impl EngineStateStore for ReadFailingStore {
    fn versions(&self) -> EngineStateVersions {
        self.inner.versions()
    }

    fn create_pending_attempt(
        &self,
        computation: DurableComputation,
    ) -> Result<DurableAttemptId, EngineStateError> {
        self.inner.create_pending_attempt(computation)
    }

    fn publish_complete(
        &self,
        attempt: DurableAttemptId,
        completion: CompletedAttempt,
    ) -> Result<(), EngineStateError> {
        self.inner.publish_complete(attempt, completion)
    }

    fn publish_failed(
        &self,
        attempt: DurableAttemptId,
        failure: StoppedAttempt,
    ) -> Result<(), EngineStateError> {
        self.inner.publish_failed(attempt, failure)
    }

    fn publish_cancelled(
        &self,
        attempt: DurableAttemptId,
        cancellation: StoppedAttempt,
    ) -> Result<(), EngineStateError> {
        self.inner.publish_cancelled(attempt, cancellation)
    }

    fn attempt(
        &self,
        attempt: DurableAttemptId,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.inner.attempt(attempt)
    }

    fn attempt_history(
        &self,
        computation: PureComputationKey,
    ) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.inner.attempt_history(computation)
    }

    fn latest_completed_reusable_attempt(
        &self,
        _computation: PureComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        Err(Self::failure())
    }

    fn latest_completed_reusable_action_attempt(
        &self,
        _computation: ActionComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        Err(Self::failure())
    }

    fn explain_invalidation(
        &self,
        _computation: PureComputationKey,
    ) -> Result<Option<InvalidationExplanation>, EngineStateError> {
        Err(Self::failure())
    }

    fn pending_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.inner.pending_attempts()
    }
}

/// One durable substrate behind several [`Engine`] instances, which is how
/// decision 0024 describes a single process owning the writable engine
/// database. Hydration is not observable within one instance — the arena index
/// answers first — so these tests need a store that outlives an engine.
#[derive(Clone, Default)]
pub struct SharedEngineStateStore(Arc<std::sync::Mutex<MemoryEngineStateStore>>);

impl SharedEngineStateStore {
    fn read<T>(
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

impl EngineStateStore for SharedEngineStateStore {
    fn versions(&self) -> EngineStateVersions {
        match self.0.lock() {
            Ok(store) => store.versions(),
            Err(_) => pith_engine::state::CURRENT_ENGINE_STATE_VERSIONS,
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
