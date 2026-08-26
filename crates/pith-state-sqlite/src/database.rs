use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use pith_diag::{Diag, EngineCode, Span};
use pith_engine::state::validate::{AttemptLookup, TerminalAttemptState, validate_publication};
use pith_engine::state::{
    DurableActionProvenance, DurableAttempt, DurableAttemptId, DurableAttemptStatus,
    DurableComputation, DurableDiagnostic, DurableObservationProvenance, DurableProvenance,
    EngineStateError, EngineStateVersions, SchemaVersion, SemanticEncodingVersion, StoppedAttempt,
};

use crate::rows::{
    Failure, attempt_computation, load_attempt, pending_attempt_rows, write_terminal_state,
};
use crate::schema::{CREATE_SCHEMA, CURRENT_VERSIONS, engine_state_versions};
use crate::store::SqliteStateError;

const MAX_QUARANTINE_CANDIDATES: u32 = 1024;

pub(super) fn open(path: &Path) -> Result<SqliteConnection, SqliteStateError> {
    let mut connection = establish(path)?;
    if compatibility(&mut connection)? == Compatibility::Incompatible {
        drop(connection);
        quarantine(path)?;
        connection = establish(path)?;
    }
    initialize(&mut connection)?;
    recover_pending(&mut connection)?;
    Ok(connection)
}

pub(super) fn open_in_memory() -> Result<SqliteConnection, SqliteStateError> {
    let mut connection = SqliteConnection::establish(":memory:")?;
    configure(&mut connection)?;
    initialize(&mut connection)?;
    recover_pending(&mut connection)?;
    Ok(connection)
}

/// Open the database at `path` for reading only.
///
/// The connection is sqlite `mode=ro`, so nothing in this process can write
/// the database file even by mistake: no schema is built, an incompatible
/// database is refused rather than moved aside (moving it aside is a write),
/// and pending attempts are left pending (marking them failed is the writable
/// open's recovery, and a reader is not the owner).
///
/// One caveat the WAL mode forces: reading a WAL database needs the shared
/// memory file, which sqlite may create beside the database even for a
/// read-only connection. The database itself is never written; the directory
/// may gain a `-shm` side file.
pub(super) fn open_read_only(path: &Path) -> Result<SqliteConnection, SqliteStateError> {
    let mut connection = establish_read_only(path)?;
    verify_readable(&mut connection, path)?;
    Ok(connection)
}

fn establish_read_only(path: &Path) -> Result<SqliteConnection, SqliteStateError> {
    let mut connection = SqliteConnection::establish(&read_only_url(path)?)?;
    // Belt and braces: the connection is already `mode=ro`, and this stops a
    // future read helper that happens to write from succeeding on a database
    // opened through some other route.
    connection.batch_execute("pragma query_only = true;")?;
    Ok(connection)
}

/// The `file:` URI that opens `path` read-only. SQLite URI paths percent-
/// decode what they are given, so a path containing `?`, `#`, or a space must
/// arrive encoded or the URI is misread.
fn read_only_url(path: &Path) -> Result<String, SqliteStateError> {
    let text = path
        .to_str()
        .ok_or_else(|| SqliteStateError::UnusablePath {
            path: path.to_path_buf(),
        })?;
    let mut url = String::from("file:");
    url.push_str(&percent_encoded(text));
    url.push_str("?mode=ro");
    Ok(url)
}

fn percent_encoded(text: &str) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                encoded.push(byte as char);
            }
            _ => {
                let _ = write!(encoded, "%{byte:02X}");
            }
        }
    }
    encoded
}

fn verify_readable(connection: &mut SqliteConnection, path: &Path) -> Result<(), SqliteStateError> {
    match compatibility(connection)? {
        Compatibility::Current => Ok(()),
        Compatibility::Fresh => Err(SqliteStateError::NothingToRead {
            path: path.to_path_buf(),
        }),
        Compatibility::Incompatible => Err(SqliteStateError::IncompatibleReadOnly {
            path: path.to_path_buf(),
        }),
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

fn configure(connection: &mut SqliteConnection) -> Result<(), SqliteStateError> {
    connection.batch_execute(
        "pragma auto_vacuum = incremental; \
         pragma journal_mode = wal; \
         pragma synchronous = full; \
         pragma foreign_keys = on;",
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

    let exists: bool = diesel::select(
        sql::<Bool>("exists (select 1 from sqlite_master where type = 'table' and name = ")
            .bind::<Text, _>(name)
            .sql(")"),
    )
    .get_result(connection)?;
    Ok(exists)
}

fn quarantine(path: &Path) -> Result<(), SqliteStateError> {
    for candidate in 0..MAX_QUARANTINE_CANDIDATES {
        let mut quarantined = path.as_os_str().to_os_string();
        quarantined.push(format!(".incompatible.{candidate}"));
        let quarantined = PathBuf::from(quarantined);
        if quarantined.exists() {
            continue;
        }
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
            let terminal_state = TerminalAttemptState::Cancelled(interrupted_attempt(&computation));
            {
                let lookup = RecoveryLookup(RefCell::new(connection));
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

fn interrupted_attempt(computation: &DurableComputation) -> StoppedAttempt {
    let provenance = match computation {
        DurableComputation::Pure(_) => DurableProvenance::Pure,
        DurableComputation::Action { .. } => {
            DurableProvenance::Action(DurableActionProvenance::NotExecuted)
        }
        DurableComputation::Observation { observer, .. } => {
            DurableProvenance::Observation(DurableObservationProvenance::NotObserved {
                observer: observer.clone(),
            })
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

struct RecoveryLookup<'connection>(RefCell<&'connection mut SqliteConnection>);

impl AttemptLookup for RecoveryLookup<'_> {
    fn lookup(
        &self,
        attempt: DurableAttemptId,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        let mut connection = self
            .0
            .try_borrow_mut()
            .map_err(|_| EngineStateError::Adapter {
                message: "the engine-state connection was already in use during recovery".into(),
            })?;
        let record = load_attempt(&mut connection, attempt).map_err(EngineStateError::from)?;
        Ok(record.map(Arc::new))
    }
}
