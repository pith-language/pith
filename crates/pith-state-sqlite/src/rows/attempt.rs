//! The attempt row itself: status, result, reuse decision, and the queries
//! that find attempts by computation, by reusability, or by being unfinished.

use pith_engine::state::validate::TerminalAttemptState;
use pith_engine::state::{
    CompletedAttempt, DurableAttempt, DurableAttemptId, DurableAttemptState, DurableAttemptStatus,
    DurableComputation, DurableReuseDecision, DurableReuseReason, EncodedValue, FailedAttempt,
};

use crate::columns::{
    ReuseKind, StoredAccess, StoredAttemptId, StoredProvenanceKind, StoredReuseKind, StoredStatus,
};
use crate::schema::{attempts, reusable_index};

use super::computation::{intern_pure_computation, load_computation, load_pure_key};
use super::corrupt;
use super::dependency::{load_dependencies, write_dependencies};
use super::diagnostic::{load_diagnostics, write_diagnostics};
use super::provenance::{load_provenance, provenance_kind, report_columns, write_report_rows};
use diesel::prelude::*;
use diesel::sqlite::Sqlite;

use super::Failure;

#[derive(Queryable, Selectable)]
#[diesel(table_name = attempts)]
#[diesel(check_for_backend(Sqlite))]
pub struct AttemptRow {
    pub(crate) id: StoredAttemptId,
    pub(crate) computation: i64,
    pub(crate) status: StoredStatus,
    pub(crate) result: Option<Vec<u8>>,
    pub(crate) reuse: Option<StoredReuseKind>,
    pub(crate) reuse_attempt: Option<StoredAttemptId>,
    pub(crate) reuse_computation: Option<i64>,
    pub(crate) provenance: Option<StoredProvenanceKind>,
    pub(crate) executor: Option<String>,
    pub(crate) platform_operating_system: Option<String>,
    pub(crate) platform_architecture: Option<String>,
    pub(crate) access: Option<StoredAccess>,
}

pub fn insert_pending_attempt(
    connection: &mut SqliteConnection,
    computation: i64,
) -> Result<DurableAttemptId, Failure> {
    let id: StoredAttemptId = diesel::insert_into(attempts::table)
        .values((
            attempts::computation.eq(computation),
            attempts::status.eq(StoredStatus(DurableAttemptStatus::Pending)),
        ))
        .returning(attempts::id)
        .get_result(connection)?;
    Ok(id.0)
}

/// Rewrite a `Pending` attempt into its terminal state, with its dependency
/// set, result, provenance, and diagnostics.
pub fn write_terminal_state(
    connection: &mut SqliteConnection,
    attempt: DurableAttemptId,
    terminal_state: &TerminalAttemptState,
) -> Result<(), Failure> {
    let stored = StoredAttemptId(attempt);
    let provenance = terminal_state.provenance();
    let report = report_columns(provenance);
    let status = match terminal_state {
        TerminalAttemptState::Complete(_) => DurableAttemptStatus::Complete,
        TerminalAttemptState::Failed(_) => DurableAttemptStatus::Failed,
    };
    let (reuse, reuse_attempt, reuse_computation) = match terminal_state {
        TerminalAttemptState::Complete(completion) => stored_reuse(connection, &completion.reuse)?,
        TerminalAttemptState::Failed(_) => (None, None, None),
    };
    let result = match terminal_state {
        TerminalAttemptState::Complete(completion) => Some(completion.result.as_bytes().to_vec()),
        TerminalAttemptState::Failed(_) => None,
    };

    diesel::update(attempts::table.find(stored))
        .set((
            attempts::status.eq(StoredStatus(status)),
            attempts::result.eq(result),
            attempts::reuse.eq(reuse),
            attempts::reuse_attempt.eq(reuse_attempt),
            attempts::reuse_computation.eq(reuse_computation),
            attempts::provenance.eq(Some(StoredProvenanceKind(provenance_kind(provenance)))),
            attempts::executor.eq(report.as_ref().map(|report| report.executor.clone())),
            attempts::platform_operating_system.eq(report
                .as_ref()
                .map(|report| report.operating_system.clone())),
            attempts::platform_architecture
                .eq(report.as_ref().map(|report| report.architecture.clone())),
            attempts::access.eq(report.as_ref().map(|report| report.access)),
        ))
        .execute(connection)?;

    write_dependencies(connection, stored, terminal_state.dependencies())?;
    write_report_rows(connection, stored, provenance)?;
    if let TerminalAttemptState::Failed(failure) = terminal_state {
        write_diagnostics(connection, stored, &failure.diagnostics)?;
    }
    Ok(())
}

type StoredReuse = (
    Option<StoredReuseKind>,
    Option<StoredAttemptId>,
    Option<i64>,
);

fn stored_reuse(
    connection: &mut SqliteConnection,
    reuse: &DurableReuseDecision,
) -> Result<StoredReuse, Failure> {
    let reason = match reuse {
        DurableReuseDecision::Reusable => {
            return Ok((Some(StoredReuseKind(ReuseKind::Reusable)), None, None));
        }
        DurableReuseDecision::NotReusable(reason) => reason,
    };
    Ok(match reason {
        DurableReuseReason::ActionCachingDisabled => (
            Some(StoredReuseKind(ReuseKind::ActionCachingDisabled)),
            None,
            None,
        ),
        DurableReuseReason::DependencyPending { attempt } => (
            Some(StoredReuseKind(ReuseKind::DependencyPending)),
            Some(StoredAttemptId(*attempt)),
            None,
        ),
        DurableReuseReason::DependencyNotReusable { attempt } => (
            Some(StoredReuseKind(ReuseKind::DependencyNotReusable)),
            Some(StoredAttemptId(*attempt)),
            None,
        ),
        DurableReuseReason::DependencyMissing { computation } => (
            Some(StoredReuseKind(ReuseKind::DependencyMissing)),
            None,
            Some(intern_pure_computation(connection, *computation)?),
        ),
    })
}

fn load_reuse(
    connection: &mut SqliteConnection,
    row: &AttemptRow,
) -> Result<DurableReuseDecision, Failure> {
    let kind = row
        .reuse
        .ok_or_else(|| corrupt("a completed attempt has no reuse decision"))?;
    let missing = |field: &str| corrupt(format!("a reuse decision has no {field}"));
    let reason = match kind.0 {
        ReuseKind::Reusable => return Ok(DurableReuseDecision::Reusable),
        ReuseKind::ActionCachingDisabled => DurableReuseReason::ActionCachingDisabled,
        ReuseKind::DependencyPending => DurableReuseReason::DependencyPending {
            attempt: row.reuse_attempt.ok_or_else(|| missing("attempt"))?.0,
        },
        ReuseKind::DependencyNotReusable => DurableReuseReason::DependencyNotReusable {
            attempt: row.reuse_attempt.ok_or_else(|| missing("attempt"))?.0,
        },
        ReuseKind::DependencyMissing => DurableReuseReason::DependencyMissing {
            computation: load_pure_key(
                connection,
                row.reuse_computation
                    .ok_or_else(|| missing("computation"))?,
            )?,
        },
    };
    Ok(DurableReuseDecision::NotReusable(reason))
}

pub fn load_attempt(
    connection: &mut SqliteConnection,
    attempt: DurableAttemptId,
) -> Result<Option<DurableAttempt>, Failure> {
    let row: Option<AttemptRow> = attempts::table
        .find(StoredAttemptId(attempt))
        .select(AttemptRow::as_select())
        .first(connection)
        .optional()?;
    let Some(row) = row else {
        return Ok(None);
    };
    restore_attempt(connection, row).map(Some)
}

pub fn load_attempts(
    connection: &mut SqliteConnection,
    rows: Vec<AttemptRow>,
) -> Result<Box<[DurableAttempt]>, Failure> {
    let mut restored = Vec::with_capacity(rows.len());
    for row in rows {
        restored.push(restore_attempt(connection, row)?);
    }
    Ok(restored.into_boxed_slice())
}

pub fn attempts_for_computation(
    connection: &mut SqliteConnection,
    computation: i64,
) -> Result<Vec<AttemptRow>, Failure> {
    Ok(attempts::table
        .filter(attempts::computation.eq(computation))
        .order(attempts::id.asc())
        .select(AttemptRow::as_select())
        .load(connection)?)
}

pub fn pending_attempt_rows(connection: &mut SqliteConnection) -> Result<Vec<AttemptRow>, Failure> {
    Ok(attempts::table
        .filter(attempts::status.eq(StoredStatus(DurableAttemptStatus::Pending)))
        .order(attempts::id.asc())
        .select(AttemptRow::as_select())
        .load(connection)?)
}

pub fn reusable_attempt_row(
    connection: &mut SqliteConnection,
    computation: i64,
) -> Result<Option<AttemptRow>, Failure> {
    Ok(reusable_index::table
        .inner_join(
            attempts::table.on(attempts::id
                .nullable()
                .eq(reusable_index::attempt.nullable())),
        )
        .filter(reusable_index::computation.eq(computation))
        .select(AttemptRow::as_select())
        .first(connection)
        .optional()?)
}

pub fn publish_reusable(
    connection: &mut SqliteConnection,
    computation: i64,
    attempt: DurableAttemptId,
) -> Result<(), Failure> {
    diesel::insert_into(reusable_index::table)
        .values((
            reusable_index::computation.eq(computation),
            reusable_index::attempt.eq(StoredAttemptId(attempt)),
        ))
        .on_conflict(reusable_index::computation)
        .do_update()
        .set(reusable_index::attempt.eq(StoredAttemptId(attempt)))
        .execute(connection)?;
    Ok(())
}

fn restore_attempt(
    connection: &mut SqliteConnection,
    row: AttemptRow,
) -> Result<DurableAttempt, Failure> {
    let computation = load_computation(connection, row.computation)?;
    let state = match row.status.0 {
        DurableAttemptStatus::Pending => DurableAttemptState::Pending,
        DurableAttemptStatus::Complete => DurableAttemptState::Complete(CompletedAttempt {
            dependencies: load_dependencies(connection, row.id)?,
            result: EncodedValue::from_bytes(
                row.result
                    .clone()
                    .ok_or_else(|| corrupt("a completed attempt retains no result"))?,
            )
            .map_err(|error| corrupt(format!("a stored result is unreadable: {error}")))?,
            provenance: load_provenance(connection, &row)?,
            reuse: load_reuse(connection, &row)?,
        }),
        DurableAttemptStatus::Failed => DurableAttemptState::Failed(FailedAttempt {
            dependencies: load_dependencies(connection, row.id)?,
            diagnostics: load_diagnostics(connection, row.id)?,
            provenance: load_provenance(connection, &row)?,
        }),
    };
    Ok(DurableAttempt {
        id: row.id.0,
        computation,
        state,
    })
}

pub fn attempt_computation(
    connection: &mut SqliteConnection,
    attempt: DurableAttemptId,
) -> Result<Option<(i64, DurableComputation, DurableAttemptStatus)>, Failure> {
    let row: Option<AttemptRow> = attempts::table
        .find(StoredAttemptId(attempt))
        .select(AttemptRow::as_select())
        .first(connection)
        .optional()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let computation = load_computation(connection, row.computation)?;
    Ok(Some((row.computation, computation, row.status.0)))
}
