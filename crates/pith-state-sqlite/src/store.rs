use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use pith_core::PureComputationKey;
use pith_diag::{Diag, EngineCode, Span};
use pith_engine::state::validate::{AttemptLookup, TerminalAttemptState, validate_publication};
use pith_engine::state::{
    CompletedAttempt, DurableActionProvenance, DurableAttempt, DurableAttemptId,
    DurableAttemptStatus, DurableComputation, DurableDiagnostic, DurableProvenance,
    EngineStateError, EngineStateStore, EngineStateVersions, InvalidationExplanation,
    SchemaVersion, SemanticEncodingVersion, StoppedAttempt,
};

use crate::rows::{
    Failure, attempt_computation, attempts_for_computation, find_pure_computation,
    insert_pending_attempt, intern_computation, load_attempt, load_attempts, pending_attempt_rows,
    publish_reusable, reusable_attempt_row, write_terminal_state,
};
use crate::schema::{CREATE_SCHEMA, CURRENT_VERSIONS, engine_state_versions};

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
        let path = path.as_ref();
        let mut connection = establish(path)?;
        // The version gate is read before any schema is applied, so a database
        // this build cannot interpret is not written to at all.
        if compatibility(&mut connection)? == Compatibility::Incompatible {
            drop(connection);
            quarantine(path)?;
            connection = establish(path)?;
        }
        initialize(&mut connection)?;
        recover_pending(&mut connection)?;
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
        let mut connection = SqliteConnection::establish(":memory:")?;
        configure(&mut connection)?;
        initialize(&mut connection)?;
        recover_pending(&mut connection)?;
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
                    let lookup = ConnectionLookup(RefCell::new(connection));
                    validate_publication(&lookup, attempt, &computation, &terminal_state)?;
                }
                let reusable = terminal_state.reusable_computation(&computation).is_some();
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

fn establish(path: &Path) -> Result<SqliteConnection, SqliteStateError> {
    let url = path
        .to_str()
        .ok_or_else(|| SqliteStateError::UnusablePath {
            path: path.to_path_buf(),
        })?;
    let mut connection = SqliteConnection::establish(url)?;
    configure(&mut connection)?;
    Ok(connection)
}

/// WAL gives concurrent readers alongside the single writer decision 0024
/// describes; FULL synchronous keeps a committed transaction durable across
/// power loss, which is what makes a published attempt a promise rather than a
/// hint.
fn configure(connection: &mut SqliteConnection) -> Result<(), SqliteStateError> {
    connection.batch_execute(
        "pragma journal_mode = wal; pragma synchronous = full; pragma foreign_keys = on;",
    )?;
    Ok(())
}

fn initialize(connection: &mut SqliteConnection) -> Result<(), SqliteStateError> {
    connection.batch_execute(CREATE_SCHEMA)?;
    diesel::replace_into(engine_state_versions::table)
        .values((
            engine_state_versions::id.eq(0),
            engine_state_versions::schema_version.eq(version(CURRENT_VERSIONS.schema.get())?),
            engine_state_versions::semantic_encoding_version
                .eq(version(CURRENT_VERSIONS.semantic_encoding.get())?),
        ))
        .execute(connection)?;
    Ok(())
}

fn version(value: u32) -> Result<i32, SqliteStateError> {
    i32::try_from(value).map_err(|_| SqliteStateError::UnrepresentableVersion { version: value })
}

#[derive(PartialEq, Eq)]
enum Compatibility {
    /// No prior database, or one that never recorded its versions and holds no
    /// attempts.
    Fresh,
    Current,
    Incompatible,
}

fn compatibility(connection: &mut SqliteConnection) -> Result<Compatibility, SqliteStateError> {
    if !table_exists(connection, "engine_state_versions")? {
        return Ok(Compatibility::Fresh);
    }
    let recorded: Option<(i32, i32)> = engine_state_versions::table
        .find(0)
        .select((
            engine_state_versions::schema_version,
            engine_state_versions::semantic_encoding_version,
        ))
        .first(connection)
        .optional()?;
    let Some((schema, semantic_encoding)) = recorded else {
        return Ok(Compatibility::Fresh);
    };
    // A negative version was never written by this adapter, so it is a database
    // this build cannot interpret rather than one to coerce into range.
    let (Ok(schema), Ok(semantic_encoding)) =
        (u32::try_from(schema), u32::try_from(semantic_encoding))
    else {
        return Ok(Compatibility::Incompatible);
    };
    let recorded = EngineStateVersions {
        schema: SchemaVersion::new(schema),
        semantic_encoding: SemanticEncodingVersion::new(semantic_encoding),
    };
    if recorded == CURRENT_VERSIONS {
        Ok(Compatibility::Current)
    } else {
        Ok(Compatibility::Incompatible)
    }
}

fn table_exists(connection: &mut SqliteConnection, name: &str) -> Result<bool, SqliteStateError> {
    use diesel::dsl::sql;
    use diesel::sql_types::{Bool, Text};

    // `sqlite_master` is sqlite's own catalogue, not part of this schema, so it
    // is the one place a statement is not built from a table definition.
    let exists: bool = diesel::select(
        sql::<Bool>("exists (select 1 from sqlite_master where type = 'table' and name = ")
            .bind::<Text, _>(name)
            .sql(")"),
    )
    .get_result(connection)?;
    Ok(exists)
}

/// Move an incompatible database aside without overwriting a previous one.
///
/// The suffix is a counter rather than a timestamp so the choice is
/// deterministic and does not read a clock.
fn quarantine(path: &Path) -> Result<(), SqliteStateError> {
    for candidate in 0u32..1024 {
        let mut quarantined = path.as_os_str().to_os_string();
        quarantined.push(format!(".incompatible.{candidate}"));
        let quarantined = PathBuf::from(quarantined);
        if quarantined.exists() {
            continue;
        }
        // Sidecar journals belong to the database they were written for; a
        // rebuilt database must not adopt them.
        for suffix in ["", "-wal", "-shm"] {
            let mut source = path.as_os_str().to_os_string();
            source.push(suffix);
            let source = PathBuf::from(source);
            if !source.exists() {
                continue;
            }
            let mut destination = quarantined.as_os_str().to_os_string();
            destination.push(suffix);
            std::fs::rename(&source, PathBuf::from(destination)).map_err(|error| {
                SqliteStateError::Filesystem {
                    path: source.clone(),
                    error,
                }
            })?;
        }
        return Ok(());
    }
    Err(SqliteStateError::NoFreeQuarantinePath {
        path: path.to_path_buf(),
    })
}

/// Mark every attempt still `Pending` as failed (decision 0024). An interrupted
/// owner never resumed them, so reopening the database does: each is written as
/// `Failed` through the same validated transaction a caller-driven failure uses,
/// with a diagnostic that names it as interrupted work.
fn recover_pending(connection: &mut SqliteConnection) -> Result<(), SqliteStateError> {
    let pending = match pending_attempt_rows(connection) {
        Ok(rows) => rows,
        Err(Failure::Database(error)) => return Err(SqliteStateError::Database(error)),
        Err(Failure::Engine(error)) => {
            return Err(SqliteStateError::Recovery {
                attempt: None,
                reason: error,
            });
        }
    };
    if pending.is_empty() {
        return Ok(());
    }
    for row in pending {
        let attempt = row.id.0;
        let result = connection.transaction::<(), Failure, _>(|connection| {
            let Some((_computation_id, computation, status)) =
                attempt_computation(connection, attempt)?
            else {
                return Err(EngineStateError::AttemptNotFound { attempt }.into());
            };
            if status != DurableAttemptStatus::Pending {
                return Ok(());
            }
            let terminal_state = TerminalAttemptState::Failed(interrupted_failure(&computation));
            {
                let lookup = ConnectionLookup(RefCell::new(connection));
                validate_publication(&lookup, attempt, &computation, &terminal_state)?;
            }
            write_terminal_state(connection, attempt, &terminal_state)?;
            Ok(())
        });
        match result {
            Ok(()) => Ok(()),
            Err(Failure::Database(error)) => Err(SqliteStateError::Database(error)),
            Err(Failure::Engine(reason)) => Err(SqliteStateError::Recovery {
                attempt: Some(attempt),
                reason,
            }),
        }?;
    }
    Ok(())
}

fn interrupted_failure(computation: &DurableComputation) -> StoppedAttempt {
    let provenance = match computation {
        DurableComputation::Pure(_) => DurableProvenance::Pure,
        DurableComputation::Action { .. } => {
            DurableProvenance::Action(DurableActionProvenance::NotExecuted)
        }
    };
    let diagnostic = DurableDiagnostic::from(&Diag::engine(
        EngineCode::InterruptedAttempt,
        Span::none(),
        "the attempt was left pending when its owner stopped",
    ));
    StoppedAttempt {
        dependencies: Box::new([]),
        diagnostics: [diagnostic].into(),
        provenance,
    }
}

/// Resolves dependency edges during validation from inside the publishing
/// transaction, so an edge is checked against the same snapshot it is written
/// into.
struct ConnectionLookup<'transaction>(RefCell<&'transaction mut SqliteConnection>);

impl AttemptLookup for ConnectionLookup<'_> {
    fn lookup(
        &self,
        attempt: DurableAttemptId,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        let mut connection = self
            .0
            .try_borrow_mut()
            .map_err(|_| EngineStateError::Adapter {
                message: "the engine-state connection was already in use during validation".into(),
            })?;
        let record = load_attempt(&mut connection, attempt).map_err(EngineStateError::from)?;
        Ok(record.map(Arc::new))
    }
}

/// Read-only dependency-edge resolver used by the invalidation chain walk.
/// Unlike [`ConnectionLookup`] this is not inside a publishing transaction; it
/// is the same `load_attempt` query run against the snapshot the read holds.
/// `RefCell` reborrows the connection the way `ConnectionLookup` does, because
/// [`AttemptLookup::lookup`] takes `&self` while `load_attempt` needs `&mut`.
struct LoadLookup<'connection>(RefCell<&'connection mut SqliteConnection>);

impl AttemptLookup for LoadLookup<'_> {
    fn lookup(
        &self,
        attempt: DurableAttemptId,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        let mut connection = self
            .0
            .try_borrow_mut()
            .map_err(|_| EngineStateError::Adapter {
                message: "the engine-state connection was already in use".into(),
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
            let lookup = LoadLookup(RefCell::new(connection));
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
