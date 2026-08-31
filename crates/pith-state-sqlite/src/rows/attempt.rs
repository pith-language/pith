//! The attempt row itself: status, result, reuse decision, and the queries
//! that find attempts by computation, by reusability, or by being unfinished.

use std::collections::BTreeMap;

use pith_core::ActionComputationKey;
use pith_engine::state::validate::TerminalAttemptState;
use pith_engine::state::{
    CompletedAttempt, DurableAttempt, DurableAttemptId, DurableAttemptState, DurableAttemptStatus,
    DurableComputation, DurableReuseDecision, DurableReuseReason, EncodedValue, StoppedAttempt,
};

use crate::columns::{
    ReuseKind, StoredAccess, StoredActionDigest, StoredAttemptId, StoredProvenanceKind,
    StoredReuseKind, StoredRevisionDigest, StoredRuleIdentity, StoredStatus,
};
use crate::schema::{attempts, computations, reusable_index};

use super::computation::{intern_pure_computation, load_computation, load_pure_key};
use super::corrupt;
use super::dependency::{load_dependencies, write_dependencies};
use super::diagnostic::{load_diagnostics, write_diagnostics};
use super::provenance::{
    load_provenance, load_required_capabilities, observation_columns, provenance_kind,
    report_columns, write_report_rows, write_required_capabilities,
};
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
    pub(crate) observer: Option<String>,
    pub(crate) observation_revision: Option<Vec<u8>>,
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
    let observation = observation_columns(provenance);
    let status = match terminal_state {
        TerminalAttemptState::Complete(_) => DurableAttemptStatus::Complete,
        TerminalAttemptState::Failed(_) => DurableAttemptStatus::Failed,
        TerminalAttemptState::Cancelled(_) => DurableAttemptStatus::Cancelled,
    };
    // Only a completed attempt carries a result and a reuse decision; the two
    // stopped states retain their edges, diagnostics, and provenance instead.
    let (reuse, reuse_attempt, reuse_computation) = match terminal_state {
        TerminalAttemptState::Complete(completion) => stored_reuse(connection, &completion.reuse)?,
        TerminalAttemptState::Failed(_) | TerminalAttemptState::Cancelled(_) => (None, None, None),
    };
    let result = match terminal_state {
        TerminalAttemptState::Complete(completion) => Some(completion.result.as_bytes().to_vec()),
        TerminalAttemptState::Failed(_) | TerminalAttemptState::Cancelled(_) => None,
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
            attempts::observer.eq(observation
                .as_ref()
                .map(|observation| observation.observer.clone())),
            attempts::observation_revision.eq(observation
                .as_ref()
                .and_then(|observation| observation.revision.clone())),
        ))
        .execute(connection)?;

    write_dependencies(connection, stored, terminal_state.dependencies())?;
    write_report_rows(connection, stored, provenance)?;
    if let TerminalAttemptState::Complete(completion) = terminal_state {
        write_required_capabilities(connection, stored, &completion.capabilities)?;
    }
    if let TerminalAttemptState::Failed(stopped) | TerminalAttemptState::Cancelled(stopped) =
        terminal_state
    {
        write_diagnostics(connection, stored, &stopped.diagnostics)?;
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
        DurableReuseReason::EffectfulDependency { attempt } => (
            Some(StoredReuseKind(ReuseKind::EffectfulDependency)),
            Some(StoredAttemptId(*attempt)),
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
        ReuseKind::EffectfulDependency => DurableReuseReason::EffectfulDependency {
            attempt: row.reuse_attempt.ok_or_else(|| missing("attempt"))?.0,
        },
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

/// The newest attempt row under one computation, whatever its status. The
/// read the whole history would answer with its last record, served from one
/// indexed row instead.
pub fn latest_attempt_row(
    connection: &mut SqliteConnection,
    computation: i64,
) -> Result<Option<AttemptRow>, Failure> {
    Ok(attempts::table
        .filter(attempts::computation.eq(computation))
        .order(attempts::id.desc())
        .select(AttemptRow::as_select())
        .first(connection)
        .optional()?)
}

pub fn pending_attempt_rows(connection: &mut SqliteConnection) -> Result<Vec<AttemptRow>, Failure> {
    Ok(attempts::table
        .filter(attempts::status.eq(StoredStatus(DurableAttemptStatus::Pending)))
        .order(attempts::id.asc())
        .select(AttemptRow::as_select())
        .load(connection)?)
}

pub fn all_attempt_rows(connection: &mut SqliteConnection) -> Result<Vec<AttemptRow>, Failure> {
    Ok(attempts::table
        .order(attempts::id.asc())
        .select(AttemptRow::as_select())
        .load(connection)?)
}

/// The columns that key one reusable-index entry. A pure key interns to one
/// computation row, so pure entries are one row per key already; an action
/// computation row is never shared, so one action key can hold an entry per
/// attempt, and the latest published of them is the one reads serve.
#[derive(Clone, Ord, PartialOrd, Eq, PartialEq)]
struct ReusableKey {
    rule_identity: Vec<u8>,
    rule_revision: Vec<u8>,
    /// The pure digest for a pure entry, the action digest for an action one.
    digest: Vec<u8>,
}

struct IndexedAttempt {
    key: ReusableKey,
    published: i64,
    attempt: StoredAttemptId,
}

fn indexed_attempts(connection: &mut SqliteConnection) -> Result<Vec<IndexedAttempt>, Failure> {
    let entries = reusable_index::table
        .select((
            reusable_index::computation,
            reusable_index::attempt,
            reusable_index::published,
        ))
        .load::<(i64, StoredAttemptId, i64)>(connection)?;
    let mut indexed = Vec::with_capacity(entries.len());
    for (computation, attempt, published) in entries {
        let (identity, revision, pure, action, observation) = computations::table
            .find(computation)
            .select((
                computations::rule_identity,
                computations::rule_revision,
                computations::pure_digest,
                computations::action_digest,
                computations::observation_digest,
            ))
            .first::<(
                Vec<u8>,
                Vec<u8>,
                Option<Vec<u8>>,
                Option<Vec<u8>>,
                Option<Vec<u8>>,
            )>(connection)
            .optional()?
            .ok_or_else(|| corrupt("the reusable index names a missing computation"))?;
        let digest = match (pure, action, observation) {
            (Some(digest), None, None) | (None, Some(digest), None) => digest,
            _ => {
                return Err(corrupt(
                    "the reusable index names a computation without exactly one reusable kind",
                ));
            }
        };
        let attempt_computation = attempts::table
            .find(attempt)
            .select(attempts::computation)
            .first::<i64>(connection)
            .optional()?
            .ok_or_else(|| corrupt("the reusable index names a missing attempt"))?;
        if attempt_computation != computation {
            return Err(corrupt(
                "the reusable index key and attempt name different computations",
            ));
        }
        indexed.push(IndexedAttempt {
            key: ReusableKey {
                rule_identity: identity,
                rule_revision: revision,
                digest,
            },
            published,
            attempt,
        });
    }
    Ok(indexed)
}

/// The newest-published entry per key, in creation order. Entries a later
/// publication superseded are absent: this is the root set, not the history.
fn newest_indexed_attempts(
    connection: &mut SqliteConnection,
) -> Result<Vec<StoredAttemptId>, Failure> {
    let mut newest: BTreeMap<ReusableKey, IndexedAttempt> = BTreeMap::new();
    for entry in indexed_attempts(connection)? {
        match newest.get(&entry.key) {
            Some(kept) if kept.published >= entry.published => {}
            _ => {
                let _ = newest.insert(entry.key.clone(), entry);
            }
        }
    }
    let mut attempts: Vec<StoredAttemptId> =
        newest.into_values().map(|entry| entry.attempt).collect();
    attempts.sort_unstable_by_key(|attempt| attempt.0);
    Ok(attempts)
}

pub fn reusable_index_attempt_rows(
    connection: &mut SqliteConnection,
) -> Result<Vec<AttemptRow>, Failure> {
    let attempts = newest_indexed_attempts(connection)?;
    let mut rows = Vec::with_capacity(attempts.len());
    for attempt in attempts {
        let row = attempts::table
            .find(attempt)
            .select(AttemptRow::as_select())
            .first(connection)
            .optional()?
            .ok_or_else(|| corrupt("the reusable index names a missing attempt"))?;
        rows.push(row);
    }
    Ok(rows)
}

pub fn reusable_index_size(connection: &mut SqliteConnection) -> Result<i64, Failure> {
    let attempts = newest_indexed_attempts(connection)?;
    i64::try_from(attempts.len())
        .map_err(|_| corrupt("the reusable index holds more entries than sqlite can count"))
}

/// The status column alone, in creation order. Enough to count a store
/// without decoding any record's payload.
pub fn attempt_status_column(
    connection: &mut SqliteConnection,
) -> Result<Vec<DurableAttemptStatus>, Failure> {
    let statuses = attempts::table
        .order(attempts::id.asc())
        .select(attempts::status)
        .load::<StoredStatus>(connection)?;
    Ok(statuses.iter().map(|status| status.0).collect())
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

/// The latest published reusable attempt for one action key.
///
/// An action computation row is never shared — it carries the authorization of
/// its own attempt — so one key can have many rows, each with its own index
/// entry. "Latest" is latest published, not newest attempt identifier: two
/// attempts of one key can complete in either order of their creation, and
/// the reference model serves whichever publication came last, so the
/// adapter orders by the publication sequence.
pub fn reusable_action_attempt_row(
    connection: &mut SqliteConnection,
    key: ActionComputationKey,
) -> Result<Option<AttemptRow>, Failure> {
    Ok(reusable_index::table
        .inner_join(
            attempts::table.on(attempts::id
                .nullable()
                .eq(reusable_index::attempt.nullable())),
        )
        .inner_join(
            computations::table.on(computations::id
                .nullable()
                .eq(reusable_index::computation.nullable())),
        )
        .filter(computations::rule_identity.eq(StoredRuleIdentity(key.rule_identity)))
        .filter(computations::rule_revision.eq(StoredRevisionDigest(key.rule_revision.digest())))
        .filter(computations::action_digest.eq(StoredActionDigest(key.digest)))
        .order(reusable_index::published.desc())
        .select(AttemptRow::as_select())
        .first(connection)
        .optional()?)
}

pub fn publish_reusable(
    connection: &mut SqliteConnection,
    computation: i64,
    attempt: DurableAttemptId,
) -> Result<(), Failure> {
    // Publication order uses `max + 1` so republishing a computation moves it
    // to the end. The process mutex serializes local writers; concurrent
    // writable processes are unsupported. Supporting them requires allocating
    // the sequence inside the insertion statement or enforcing a unique
    // sequence catching a collision.
    let published: i64 = reusable_index::table
        .select(diesel::dsl::max(reusable_index::published))
        .first::<Option<i64>>(connection)?
        .unwrap_or(0)
        .saturating_add(1);
    diesel::insert_into(reusable_index::table)
        .values((
            reusable_index::computation.eq(computation),
            reusable_index::attempt.eq(StoredAttemptId(attempt)),
            reusable_index::published.eq(published),
        ))
        .on_conflict(reusable_index::computation)
        .do_update()
        .set((
            reusable_index::attempt.eq(StoredAttemptId(attempt)),
            reusable_index::published.eq(published),
        ))
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
            capabilities: load_required_capabilities(connection, row.id)?,
        }),
        DurableAttemptStatus::Failed => {
            DurableAttemptState::Failed(restore_stopped(connection, &row)?)
        }
        DurableAttemptStatus::Cancelled => {
            DurableAttemptState::Cancelled(restore_stopped(connection, &row)?)
        }
    };
    Ok(DurableAttempt {
        id: row.id.0,
        computation,
        state,
    })
}

fn restore_stopped(
    connection: &mut SqliteConnection,
    row: &AttemptRow,
) -> Result<StoppedAttempt, Failure> {
    Ok(StoppedAttempt {
        dependencies: load_dependencies(connection, row.id)?,
        diagnostics: load_diagnostics(connection, row.id)?,
        provenance: load_provenance(connection, row)?,
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
