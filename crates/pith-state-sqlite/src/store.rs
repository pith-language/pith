use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use pith_core::{ActionComputationKey, PureComputationKey};
use pith_engine::state::validate::{AttemptLookup, TerminalAttemptState, validate_publication};
use pith_engine::state::{
    AttemptStatistics, CompletedAttempt, DurableAttempt, DurableAttemptId, DurableAttemptStatus,
    DurableComputation, EngineStateError, EngineStateReader, EngineStateStore, EngineStateVersions,
    InvalidationExplanation, StoppedAttempt,
};

use crate::rows::{
    Failure, all_attempt_rows, attempt_computation, attempt_status_column,
    attempts_for_computation, find_pure_computation, insert_pending_attempt, intern_computation,
    latest_attempt_row, load_attempt, load_attempts, pending_attempt_rows, publish_reusable,
    reusable_action_attempt_row, reusable_attempt_row, reusable_index_attempt_rows,
    reusable_index_size, write_terminal_state,
};
use crate::schema::CURRENT_VERSIONS;

/// Failure to open an engine-state database.
///
/// Distinct from [`EngineStateError`], which describes a store that is open and
/// rejecting an operation. Opening can fail in ways that have no equivalent
/// once the store exists: an unreadable directory, or an incompatible database
/// that could not be moved aside.
#[derive(Debug)]
pub enum SqliteStateError {
    Database(diesel::result::Error),
    Connection(diesel::ConnectionError),
    Filesystem {
        path: PathBuf,
        error: std::io::Error,
    },
    /// Every candidate path for moving an incompatible database aside was
    /// taken. Rebuilding would destroy the existing records, so opening fails
    /// instead.
    NoFreeQuarantinePath {
        path: PathBuf,
    },
    /// The path cannot be given to sqlite as a connection string.
    UnusablePath {
        path: PathBuf,
    },
    /// The file exists but holds no engine-state schema, so there is nothing
    /// to read. A writable open would build one; a read-only open cannot, and
    /// an empty file is more likely a mistaken path than a cache.
    NothingToRead {
        path: PathBuf,
    },
    /// A read-only open found a database whose versions this build refuses to
    /// read. The writable open moves such a database aside and rebuilds; a
    /// read-only open cannot, so it refuses instead.
    IncompatibleReadOnly {
        path: PathBuf,
    },
    /// A version number this build reports does not fit sqlite's integer type.
    UnrepresentableVersion {
        version: u32,
    },
    /// An attempt left `Pending` could not be marked failed on reopen because its
    /// stored record contradicts the recovery write. The database is inconsistent.
    Recovery {
        attempt: Option<DurableAttemptId>,
        reason: EngineStateError,
    },
    /// A thread panicked while holding the connection, so its state is unknown.
    Poisoned,
}

impl std::fmt::Display for SqliteStateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "engine-state database error: {error}"),
            Self::Connection(error) => {
                write!(
                    formatter,
                    "could not open the engine-state database: {error}"
                )
            }
            Self::Filesystem { path, error } => {
                write!(formatter, "engine-state path {}: {error}", path.display())
            }
            Self::NoFreeQuarantinePath { path } => write!(
                formatter,
                "could not move the incompatible engine-state database at {} aside",
                path.display()
            ),
            Self::UnusablePath { path } => write!(
                formatter,
                "the engine-state path {} is not valid utf-8",
                path.display()
            ),
            Self::NothingToRead { path } => write!(
                formatter,
                "the file at {} holds no engine state: nothing to read",
                path.display()
            ),
            Self::IncompatibleReadOnly { path } => write!(
                formatter,
                "the engine-state database at {} records versions this build refuses to read",
                path.display()
            ),
            Self::UnrepresentableVersion { version } => write!(
                formatter,
                "engine-state version {version} exceeds the storable range"
            ),
            Self::Recovery { attempt, reason } => match attempt {
                Some(attempt) => write!(
                    formatter,
                    "engine-state attempt {attempt} could not be marked failed on reopen: {reason}"
                ),
                None => write!(
                    formatter,
                    "engine-state recovery could not enumerate pending attempts: {reason}"
                ),
            },
            Self::Poisoned => {
                formatter.write_str("the engine-state connection was poisoned by a panic")
            }
        }
    }
}

impl std::error::Error for SqliteStateError {}

impl From<diesel::result::Error> for SqliteStateError {
    fn from(error: diesel::result::Error) -> Self {
        Self::Database(error)
    }
}

impl From<diesel::ConnectionError> for SqliteStateError {
    fn from(error: diesel::ConnectionError) -> Self {
        Self::Connection(error)
    }
}

/// Durable engine state in SQLite. The connection is serialized because Diesel
/// connections are `Send` but not `Sync`, while [`EngineStateStore`] is both.
pub struct SqliteEngineStateStore {
    connection: Mutex<SqliteConnection>,
}

impl SqliteEngineStateStore {
    /// Open the engine-state database at `path`, creating it if absent.
    ///
    /// An incompatible pre-release database is moved aside and rebuilt rather
    /// than interpreted under a different schema or encoding.
    ///
    /// # Errors
    /// Returns [`SqliteStateError`] when the database cannot be opened,
    /// initialized, or moved aside.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SqliteStateError> {
        let connection = crate::database::open(path.as_ref())?;
        Ok(Self::from_connection(connection))
    }

    /// Open a private in-memory engine-state database. Exercises the same
    /// schema and transactions as [`Self::open`] without touching a filesystem.
    ///
    /// # Errors
    /// Returns [`SqliteStateError`] when the database cannot be initialized.
    pub fn open_in_memory() -> Result<Self, SqliteStateError> {
        let connection = crate::database::open_in_memory()?;
        Ok(Self::from_connection(connection))
    }

    fn from_connection(connection: SqliteConnection) -> Self {
        Self {
            connection: Mutex::new(connection),
        }
    }

    /// The versions this build reads and writes.
    #[must_use]
    pub const fn current_versions() -> EngineStateVersions {
        CURRENT_VERSIONS
    }

    fn locked(&self) -> Result<MutexGuard<'_, SqliteConnection>, EngineStateError> {
        self.connection
            .lock()
            .map_err(|_| EngineStateError::Adapter {
                message: "the engine-state connection was poisoned by a panic".into(),
            })
    }

    fn publish(
        &self,
        attempt: DurableAttemptId,
        terminal_state: TerminalAttemptState,
    ) -> Result<(), EngineStateError> {
        let mut guard = self.locked()?;
        guard
            .transaction::<(), Failure, _>(|connection| {
                let Some((computation_id, computation, status)) =
                    attempt_computation(connection, attempt)?
                else {
                    return Err(EngineStateError::AttemptNotFound { attempt }.into());
                };
                if status != DurableAttemptStatus::Pending {
                    return Err(EngineStateError::AttemptNotPending { attempt, status }.into());
                }
                {
                    let lookup = ConnectionLookup::publishing(connection);
                    validate_publication(&lookup, attempt, &computation, &terminal_state)?;
                }
                let reusable = terminal_state.is_reusable()
                    && !matches!(computation, DurableComputation::Observation { .. });
                write_terminal_state(connection, attempt, &terminal_state)?;
                if reusable {
                    publish_reusable(connection, computation_id, attempt)?;
                }
                Ok(())
            })
            .map_err(EngineStateError::from)
    }

    fn read<T>(
        &self,
        query: impl FnOnce(&mut SqliteConnection) -> Result<T, Failure>,
    ) -> Result<T, EngineStateError> {
        let mut guard = self.locked()?;
        query(&mut guard).map_err(EngineStateError::from)
    }
}

/// A SQLite connection opened with `mode=ro` and exposing only
/// [`EngineStateReader`] authority.
pub struct ReadOnlySqliteEngineStateStore {
    inner: SqliteEngineStateStore,
}

impl ReadOnlySqliteEngineStateStore {
    /// Open the engine-state database at `path` for reading only.
    ///
    /// Unlike [`SqliteEngineStateStore::open`], nothing is created, moved
    /// aside, or recovered: the database must exist and record versions this
    /// build reads, or opening fails.
    ///
    /// # Errors
    /// Returns [`SqliteStateError`] when the database is absent, holds no
    /// engine state, or records incompatible versions.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self, SqliteStateError> {
        let connection = crate::database::open_read_only(path.as_ref())?;
        Ok(Self {
            inner: SqliteEngineStateStore::from_connection(connection),
        })
    }
}

impl EngineStateReader for ReadOnlySqliteEngineStateStore {
    fn versions(&self) -> EngineStateVersions {
        self.inner.versions()
    }

    fn attempt_statistics(&self) -> Result<AttemptStatistics, EngineStateError> {
        self.inner.attempt_statistics()
    }

    fn all_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.inner.all_attempts()
    }

    fn reusable_index_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.inner.reusable_index_attempts()
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

    fn latest_attempt(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.inner.latest_attempt(computation)
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

/// Resolves dependency edges during validation from inside the publishing
/// transaction, so an edge is checked against the same snapshot it is written
/// into.
struct ConnectionLookup<'connection> {
    connection: RefCell<&'connection mut SqliteConnection>,
    borrowed_message: &'static str,
}

impl<'connection> ConnectionLookup<'connection> {
    fn publishing(connection: &'connection mut SqliteConnection) -> Self {
        Self {
            connection: RefCell::new(connection),
            borrowed_message: "the engine-state connection was already in use during validation",
        }
    }

    fn reading(connection: &'connection mut SqliteConnection) -> Self {
        Self {
            connection: RefCell::new(connection),
            borrowed_message: "the engine-state connection was already in use",
        }
    }
}

impl AttemptLookup for ConnectionLookup<'_> {
    fn lookup(
        &self,
        attempt: DurableAttemptId,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        let mut connection =
            self.connection
                .try_borrow_mut()
                .map_err(|_| EngineStateError::Adapter {
                    message: self.borrowed_message.into(),
                })?;
        let record = load_attempt(&mut connection, attempt).map_err(EngineStateError::from)?;
        Ok(record.map(Arc::new))
    }
}

impl EngineStateReader for SqliteEngineStateStore {
    fn versions(&self) -> EngineStateVersions {
        CURRENT_VERSIONS
    }

    fn attempt_statistics(&self) -> Result<AttemptStatistics, EngineStateError> {
        self.read(|connection| {
            connection.transaction::<AttemptStatistics, Failure, _>(|connection| {
                let mut statistics = AttemptStatistics::default();
                for status in attempt_status_column(connection)? {
                    statistics.record(status);
                }
                let indexed = reusable_index_size(connection)?;
                statistics.reusable_index = u64::try_from(indexed).unwrap_or(u64::MAX);
                Ok(statistics)
            })
        })
    }

    fn all_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.read(|connection| {
            let rows = all_attempt_rows(connection)?;
            Ok(shared(load_attempts(connection, rows)?))
        })
    }

    fn reusable_index_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.read(|connection| {
            connection.transaction::<Box<[Arc<DurableAttempt>]>, Failure, _>(|connection| {
                let rows = reusable_index_attempt_rows(connection)?;
                Ok(shared(load_attempts(connection, rows)?))
            })
        })
    }

    fn attempt(
        &self,
        attempt: DurableAttemptId,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.read(|connection| Ok(load_attempt(connection, attempt)?.map(Arc::new)))
    }

    fn attempt_history(
        &self,
        computation: PureComputationKey,
    ) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.read(|connection| {
            let Some(computation) = find_pure_computation(connection, computation)? else {
                return Ok(Vec::new().into_boxed_slice());
            };
            let rows = attempts_for_computation(connection, computation)?;
            Ok(shared(load_attempts(connection, rows)?))
        })
    }

    fn latest_attempt(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.read(|connection| {
            let Some(computation) = find_pure_computation(connection, computation)? else {
                return Ok(None);
            };
            let Some(row) = latest_attempt_row(connection, computation)? else {
                return Ok(None);
            };
            Ok(load_attempts(connection, vec![row])?
                .into_iter()
                .next()
                .map(Arc::new))
        })
    }

    fn latest_completed_reusable_attempt(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.read(|connection| {
            let Some(computation) = find_pure_computation(connection, computation)? else {
                return Ok(None);
            };
            let Some(row) = reusable_attempt_row(connection, computation)? else {
                return Ok(None);
            };
            Ok(first(load_attempts(connection, vec![row])?))
        })
    }

    fn latest_completed_reusable_action_attempt(
        &self,
        computation: ActionComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.read(|connection| {
            let Some(row) = reusable_action_attempt_row(connection, computation)? else {
                return Ok(None);
            };
            Ok(first(load_attempts(connection, vec![row])?))
        })
    }

    fn explain_invalidation(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<InvalidationExplanation>, EngineStateError> {
        self.read(|connection| {
            let latest = match find_pure_computation(connection, computation)? {
                Some(computation) => match reusable_attempt_row(connection, computation)? {
                    Some(row) => {
                        let attempt = load_attempts(connection, vec![row])?
                            .into_iter()
                            .next()
                            .ok_or_else(|| {
                                Failure::Engine(EngineStateError::Adapter {
                                    message:
                                        "the reusable index references an attempt that cannot be read"
                                            .into(),
                                })
                            })?;
                        Some(Arc::new(attempt))
                    }
                    None => None,
                },
                None => None,
            };
            let lookup = ConnectionLookup::reading(connection);
            Ok(pith_engine::state::explain::explain_latest(&lookup, latest)?)
        })
    }

    fn pending_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.read(|connection| {
            let rows = pending_attempt_rows(connection)?;
            Ok(shared(load_attempts(connection, rows)?))
        })
    }
}

impl EngineStateStore for SqliteEngineStateStore {
    fn create_pending_attempt(
        &self,
        computation: DurableComputation,
    ) -> Result<DurableAttemptId, EngineStateError> {
        let mut guard = self.locked()?;
        guard
            .transaction::<DurableAttemptId, Failure, _>(|connection| {
                let computation = intern_computation(connection, &computation)?;
                insert_pending_attempt(connection, computation)
            })
            .map_err(EngineStateError::from)
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

fn shared(attempts: Box<[DurableAttempt]>) -> Box<[Arc<DurableAttempt>]> {
    attempts.into_iter().map(Arc::new).collect()
}

fn first(attempts: Box<[DurableAttempt]>) -> Option<Arc<DurableAttempt>> {
    attempts.into_iter().next().map(Arc::new)
}
