//! Inspection of one engine-state database: what it holds, and whether every
//! record it holds reads back.

use std::io;
use std::path::Path;

use pith_engine::state::{AttemptStatistics, EngineStateReader};
use pith_output::dto::{AttemptCounts, StateCheck, StateInfo};
use pith_state_sqlite::ReadOnlySqliteEngineStateStore;

use crate::error::QueryError;
use crate::roots::Roots;
use crate::session::{ReadOnly, Session, read_failure};

/// The adapter every session opens. The DTO carries it because a repair
/// question is addressed to the holder, not to the query layer.
const ADAPTER: &str = "sqlite";

impl Session<ReadOnly> {
    /// The versions and counts `pith state info` reports.
    ///
    /// A machine that has recorded nothing yet still gets a report: the
    /// versions are what a writable open would create, and the counts are
    /// zero. A database that exists but cannot be read is an error, not an
    /// empty store.
    ///
    /// # Errors
    /// [`QueryError`] when a present state database cannot be opened or
    /// counted.
    pub fn state_info(&self) -> Result<StateInfo, QueryError> {
        match self.state_if_present()? {
            Some(state) => {
                let versions = state.versions();
                let statistics = state.attempt_statistics().map_err(read_failure)?;
                Ok(StateInfo {
                    adapter: ADAPTER.into(),
                    schema_version: versions.schema.get(),
                    semantic_encoding_version: versions.semantic_encoding.get(),
                    attempts: counts(statistics),
                    reusable_index: statistics.reusable_index,
                })
            }
            None => Ok(empty_info()),
        }
    }

    /// Decode every durable record, which is the integrity scan `pith state
    /// check` runs. The read fails on the first record that does not decode,
    /// so an `Ok` answer means the whole database read back.
    ///
    /// # Errors
    /// [`QueryError`] when a present state database cannot be opened or a
    /// record fails its decode validation.
    pub fn state_check(&self) -> Result<StateCheck, QueryError> {
        match self.state_if_present()? {
            Some(state) => {
                let records = state.all_attempts().map_err(read_failure)?.len();
                Ok(StateCheck {
                    records: u64::try_from(records).unwrap_or(u64::MAX),
                })
            }
            None => Ok(StateCheck { records: 0 }),
        }
    }

    /// The read-only state database when one exists, and `None` when the path
    /// holds nothing at all. Presence is decided by the filesystem rather
    /// than by the open error, because an existing file that fails to open is
    /// a reportable failure and not an empty store.
    ///
    /// # Errors
    /// [`QueryError`] when a present database cannot be opened read-only.
    pub(crate) fn state_if_present(
        &self,
    ) -> Result<Option<&ReadOnlySqliteEngineStateStore>, QueryError> {
        if !state_is_present(self.roots())? {
            return Ok(None);
        }
        self.state().map(Some)
    }
}

fn state_is_present(roots: &Roots) -> Result<bool, QueryError> {
    let path = roots.state();
    if path_exists(path)? {
        return Ok(true);
    }
    let mut wal = path.as_os_str().to_os_string();
    wal.push("-wal");
    path_exists(Path::new(&wal))
}

fn path_exists(path: &Path) -> Result<bool, QueryError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(QueryError::user(format!(
            "cannot inspect `{}`: {error}",
            path.display()
        ))),
    }
}

fn empty_info() -> StateInfo {
    let versions = pith_state_sqlite::SqliteEngineStateStore::current_versions();
    StateInfo {
        adapter: ADAPTER.into(),
        schema_version: versions.schema.get(),
        semantic_encoding_version: versions.semantic_encoding.get(),
        attempts: AttemptCounts::default(),
        reusable_index: 0,
    }
}

fn counts(statistics: AttemptStatistics) -> AttemptCounts {
    AttemptCounts {
        total: statistics.attempts,
        pending: statistics.pending,
        complete: statistics.complete,
        failed: statistics.failed,
        cancelled: statistics.cancelled,
    }
}
