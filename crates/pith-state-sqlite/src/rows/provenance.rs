//! Executor reports, the outputs they captured, and the capabilities they used.

use pith_core::{CapabilityRequirement, Content, OutputKind};
use pith_engine::state::{
    DurableActionProvenance, DurableCapturedExecutionReport, DurableCapturedOutput,
    DurableProvenance,
};
use pith_engine::{ExecutionPlatform, ExecutionReport, ProducedOutput};
use pith_ids::ContentId;

use crate::columns::{
    ProvenanceKind, StoredAccess, StoredAttemptId, StoredContentId, StoredOutputKind,
};
use crate::schema::{produced_outputs, report_capabilities};

use super::attempt::AttemptRow;
use super::{corrupt, position};
use diesel::prelude::*;
use diesel::sqlite::Sqlite;

use super::Failure;

#[derive(Insertable, Queryable, Selectable)]
#[diesel(table_name = report_capabilities)]
#[diesel(check_for_backend(Sqlite))]
struct CapabilityRow {
    attempt: StoredAttemptId,
    position: i32,
    name: String,
    scope: String,
}

#[derive(Insertable, Queryable, Selectable)]
#[diesel(table_name = produced_outputs)]
#[diesel(check_for_backend(Sqlite))]
struct OutputRow {
    attempt: StoredAttemptId,
    position: i32,
    path: String,
    kind: StoredOutputKind,
    content: Option<StoredContentId>,
}

/// The executor data an attempt's provenance carries, in the one shape the
/// columns hold. A captured output has no imported content identity.
struct StoredReport<'provenance> {
    executor: String,
    operating_system: String,
    architecture: String,
    access: pith_engine::AccessVerification,
    capabilities_used: &'provenance [CapabilityRequirement],
    outputs: Vec<(String, OutputKind, Option<ContentId>)>,
}

/// The executor columns of an attempt row, absent when nothing executed.
pub(super) struct ReportColumns {
    pub(super) executor: String,
    pub(super) operating_system: String,
    pub(super) architecture: String,
    pub(super) access: StoredAccess,
}

/// The columns an attempt row carries for `provenance`.
pub(super) fn report_columns(provenance: &DurableProvenance) -> Option<ReportColumns> {
    executor_report(provenance).map(|report| ReportColumns {
        executor: report.executor,
        operating_system: report.operating_system,
        architecture: report.architecture,
        access: StoredAccess(report.access),
    })
}

/// The capability and output rows an executor report expands into.
pub(super) fn write_report_rows(
    connection: &mut SqliteConnection,
    attempt: StoredAttemptId,
    provenance: &DurableProvenance,
) -> Result<(), Failure> {
    let Some(report) = executor_report(provenance) else {
        return Ok(());
    };
    write_capabilities(connection, attempt, report.capabilities_used)?;
    write_outputs(connection, attempt, &report.outputs)
}

fn executor_report(provenance: &DurableProvenance) -> Option<StoredReport<'_>> {
    let DurableProvenance::Action(action) = provenance else {
        return None;
    };
    match action {
        DurableActionProvenance::NotExecuted => None,
        DurableActionProvenance::Captured { executor_report } => Some(StoredReport {
            executor: executor_report.executor.to_string(),
            operating_system: executor_report.platform.operating_system.to_string(),
            architecture: executor_report.platform.architecture.to_string(),
            access: executor_report.access,
            capabilities_used: &executor_report.capabilities_used,
            outputs: executor_report
                .outputs
                .iter()
                .map(|output| (output.path.to_string(), output.kind, None))
                .collect(),
        }),
        DurableActionProvenance::Imported { imported_report } => Some(StoredReport {
            executor: imported_report.executor.to_string(),
            operating_system: imported_report.platform.operating_system.to_string(),
            architecture: imported_report.platform.architecture.to_string(),
            access: imported_report.access,
            capabilities_used: &imported_report.capabilities_used,
            outputs: imported_report
                .outputs
                .iter()
                .map(|output| {
                    let (Content::Blob(content) | Content::Tree(content)) = output.content;
                    (
                        output.path.to_string(),
                        output.content.kind(),
                        Some(content),
                    )
                })
                .collect(),
        }),
    }
}

pub(super) const fn provenance_kind(provenance: &DurableProvenance) -> ProvenanceKind {
    match provenance {
        DurableProvenance::Pure => ProvenanceKind::Pure,
        DurableProvenance::Action(DurableActionProvenance::NotExecuted) => {
            ProvenanceKind::ActionNotExecuted
        }
        DurableProvenance::Action(DurableActionProvenance::Captured { .. }) => {
            ProvenanceKind::ActionCaptured
        }
        DurableProvenance::Action(DurableActionProvenance::Imported { .. }) => {
            ProvenanceKind::ActionImported
        }
    }
}

fn write_capabilities(
    connection: &mut SqliteConnection,
    attempt: StoredAttemptId,
    capabilities: &[CapabilityRequirement],
) -> Result<(), Failure> {
    let mut rows = Vec::with_capacity(capabilities.len());
    for (index, capability) in capabilities.iter().enumerate() {
        rows.push(CapabilityRow {
            attempt,
            position: position(index)?,
            name: capability.name.to_string(),
            scope: capability.scope.to_string(),
        });
    }
    diesel::insert_into(report_capabilities::table)
        .values(rows)
        .execute(connection)?;
    Ok(())
}

fn load_capabilities(
    connection: &mut SqliteConnection,
    attempt: StoredAttemptId,
) -> Result<Box<[CapabilityRequirement]>, Failure> {
    let rows: Vec<CapabilityRow> = report_capabilities::table
        .filter(report_capabilities::attempt.eq(attempt))
        .order(report_capabilities::position.asc())
        .select(CapabilityRow::as_select())
        .load(connection)?;
    Ok(rows
        .into_iter()
        .map(|row| CapabilityRequirement {
            name: row.name.into(),
            scope: row.scope.into(),
        })
        .collect())
}

fn write_outputs(
    connection: &mut SqliteConnection,
    attempt: StoredAttemptId,
    outputs: &[(String, OutputKind, Option<ContentId>)],
) -> Result<(), Failure> {
    let mut rows = Vec::with_capacity(outputs.len());
    for (index, (path, kind, content)) in outputs.iter().enumerate() {
        rows.push(OutputRow {
            attempt,
            position: position(index)?,
            path: path.clone(),
            kind: StoredOutputKind(*kind),
            content: content.map(StoredContentId),
        });
    }
    diesel::insert_into(produced_outputs::table)
        .values(rows)
        .execute(connection)?;
    Ok(())
}

fn load_outputs(
    connection: &mut SqliteConnection,
    attempt: StoredAttemptId,
) -> Result<Vec<OutputRow>, Failure> {
    Ok(produced_outputs::table
        .filter(produced_outputs::attempt.eq(attempt))
        .order(produced_outputs::position.asc())
        .select(OutputRow::as_select())
        .load(connection)?)
}

pub(super) fn load_provenance(
    connection: &mut SqliteConnection,
    row: &AttemptRow,
) -> Result<DurableProvenance, Failure> {
    let kind = row
        .provenance
        .ok_or_else(|| corrupt("a terminal attempt has no provenance"))?;
    let action = match kind.0 {
        ProvenanceKind::Pure => return Ok(DurableProvenance::Pure),
        ProvenanceKind::ActionNotExecuted => DurableActionProvenance::NotExecuted,
        ProvenanceKind::ActionCaptured => DurableActionProvenance::Captured {
            executor_report: DurableCapturedExecutionReport {
                executor: load_executor(row)?,
                platform: load_platform(row)?,
                access: load_access(row)?,
                outputs: load_outputs(connection, row.id)?
                    .into_iter()
                    .map(|output| DurableCapturedOutput {
                        path: output.path.into(),
                        kind: output.kind.0,
                    })
                    .collect(),
                capabilities_used: load_capabilities(connection, row.id)?,
            },
        },
        ProvenanceKind::ActionImported => {
            let mut outputs = Vec::new();
            for output in load_outputs(connection, row.id)? {
                let content = output
                    .content
                    .ok_or_else(|| corrupt("an imported output has no content identity"))?;
                outputs.push(ProducedOutput {
                    path: output.path.into(),
                    content: match output.kind.0 {
                        OutputKind::Blob => Content::Blob(content.0),
                        OutputKind::Tree => Content::Tree(content.0),
                    },
                });
            }
            DurableActionProvenance::Imported {
                imported_report: ExecutionReport {
                    executor: load_executor(row)?,
                    platform: load_platform(row)?,
                    access: load_access(row)?,
                    outputs: outputs.into_boxed_slice(),
                    capabilities_used: load_capabilities(connection, row.id)?,
                },
            }
        }
    };
    Ok(DurableProvenance::Action(action))
}

fn load_executor(row: &AttemptRow) -> Result<Box<str>, Failure> {
    Ok(row
        .executor
        .clone()
        .ok_or_else(|| corrupt("an executor report names no executor"))?
        .into())
}

fn load_platform(row: &AttemptRow) -> Result<ExecutionPlatform, Failure> {
    let (Some(operating_system), Some(architecture)) = (
        row.platform_operating_system.clone(),
        row.platform_architecture.clone(),
    ) else {
        return Err(corrupt("an executor report retains no platform"));
    };
    Ok(ExecutionPlatform {
        operating_system: operating_system.into(),
        architecture: architecture.into(),
    })
}

fn load_access(row: &AttemptRow) -> Result<pith_engine::AccessVerification, Failure> {
    Ok(row
        .access
        .ok_or_else(|| corrupt("an executor report retains no access verification"))?
        .0)
}
