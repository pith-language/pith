use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use pith_core::{ActionComputationKey, PureComputationKey};
use pith_engine::state::validate::{AttemptLookup, TerminalAttemptState, validate_publication};
use pith_engine::state::{
    CompletedAttempt, DurableAttempt, DurableAttemptId, DurableAttemptStatus, DurableComputation,
    EngineStateError, EngineStateStore, EngineStateVersions, InvalidationExplanation,
    StoppedAttempt,
};

use crate::rows::{
    Failure, attempt_computation, attempts_for_computation, find_pure_computation,
    insert_pending_attempt, intern_computation, load_attempt, load_attempts, pending_attempt_rows,
    publish_reusable, reusable_action_attempt_row, reusable_attempt_row, write_terminal_state,
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

/// Durable engine state in a sqlite database (decisions 0024 and 0025).
///
/// A diesel `SqliteConnection` is `Send` but not `Sync`, while
/// [`EngineStateStore`] is both so the engine's evaluation future stays `Send`.
/// The connection is therefore serialized behind a mutex, which is the
/// arrangement decision 0024 describes: one process owns the writable database
/// and metadata access is serialized through the owner rather than locked
/// throughout the graph.
pub struct SqliteEngineStateStore {
    connection: Mutex<SqliteConnection>,
}

impl SqliteEngineStateStore {
    /// Open the engine-state database at `path`, creating it if absent.
    ///
    /// An existing database whose schema or record-encoding version differs
    /// from this build's is moved aside and rebuilt empty. Decision 0024 makes
    /// that the pre-release policy: reinterpreting records under a different
    /// version is forbidden, and losing a cache is cheaper than losing
    /// correctness. Content objects survive, because their identities are
    /// domain separated and include their own encoding version.
    ///
    /// # Errors
    /// Returns [`SqliteStateError`] when the database cannot be opened,
    /// initialized, or moved aside.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SqliteStateError> {
        let connection = crate::database::open(path.as_ref())?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    /// Open a private in-memory engine-state database. Exercises the same
    /// schema and transactions as [`Self::open`] without touching a filesystem.
    ///
    /// # Errors
    /// Returns [`SqliteStateError`] when the database cannot be initialized.
    pub fn open_in_memory() -> Result<Self, SqliteStateError> {
        let connection = crate::database::open_in_memory()?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
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
                let reusable = terminal_state.is_reusable();
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

impl EngineStateStore for SqliteEngineStateStore {
    fn versions(&self) -> EngineStateVersions {
        CURRENT_VERSIONS
    }

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

fn shared(attempts: Box<[DurableAttempt]>) -> Box<[Arc<DurableAttempt>]> {
    attempts.into_iter().map(Arc::new).collect()
}

fn first(attempts: Box<[DurableAttempt]>) -> Option<Arc<DurableAttempt>> {
    attempts.into_iter().next().map(Arc::new)
}
