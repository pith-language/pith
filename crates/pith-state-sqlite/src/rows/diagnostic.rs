//! Retained diagnostics and their notes.

use pith_diag::StableCode;
use pith_engine::state::{DurableDiagnostic, DurableDiagnosticNote};

use crate::columns::{StoredAttemptId, StoredSeverity};
use crate::schema::{diagnostic_notes, diagnostics};

use super::{corrupt, position, span};
use diesel::prelude::*;
use diesel::sqlite::Sqlite;

use super::Failure;

#[derive(Insertable)]
#[diesel(table_name = diagnostics)]
struct NewDiagnostic {
    attempt: StoredAttemptId,
    position: i32,
    severity: StoredSeverity,
    code: i32,
    span_start: i32,
    span_end: i32,
    message: String,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = diagnostics)]
#[diesel(check_for_backend(Sqlite))]
struct DiagnosticRow {
    id: i64,
    severity: StoredSeverity,
    code: i32,
    span_start: i32,
    span_end: i32,
    message: String,
}

#[derive(Insertable, Queryable, Selectable)]
#[diesel(table_name = diagnostic_notes)]
#[diesel(check_for_backend(Sqlite))]
struct NoteRow {
    diagnostic: i64,
    position: i32,
    span_start: i32,
    span_end: i32,
    message: String,
}

pub(super) fn write_diagnostics(
    connection: &mut SqliteConnection,
    attempt: StoredAttemptId,
    diagnostics: &[DurableDiagnostic],
) -> Result<(), Failure> {
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        let id: i64 = diesel::insert_into(diagnostics::table)
            .values(NewDiagnostic {
                attempt,
                position: position(index)?,
                severity: StoredSeverity(diagnostic.severity),
                code: i32::try_from(diagnostic.code.0)
                    .map_err(|_| corrupt("a diagnostic code exceeds the storable range"))?,
                span_start: position(diagnostic.span.start.0 as usize)?,
                span_end: position(diagnostic.span.end.0 as usize)?,
                message: diagnostic.message.to_string(),
            })
            .returning(diagnostics::id)
            .get_result(connection)?;
        let mut notes = Vec::with_capacity(diagnostic.notes.len());
        for (note_index, note) in diagnostic.notes.iter().enumerate() {
            notes.push(NoteRow {
                diagnostic: id,
                position: position(note_index)?,
                span_start: position(note.span.start.0 as usize)?,
                span_end: position(note.span.end.0 as usize)?,
                message: note.message.to_string(),
            });
        }
        diesel::insert_into(diagnostic_notes::table)
            .values(notes)
            .execute(connection)?;
    }
    Ok(())
}

pub(super) fn load_diagnostics(
    connection: &mut SqliteConnection,
    attempt: StoredAttemptId,
) -> Result<Box<[DurableDiagnostic]>, Failure> {
    let rows: Vec<DiagnosticRow> = diagnostics::table
        .filter(diagnostics::attempt.eq(attempt))
        .order(diagnostics::position.asc())
        .select(DiagnosticRow::as_select())
        .load(connection)?;
    let mut restored = Vec::with_capacity(rows.len());
    for row in rows {
        let notes: Vec<NoteRow> = diagnostic_notes::table
            .filter(diagnostic_notes::diagnostic.eq(row.id))
            .order(diagnostic_notes::position.asc())
            .select(NoteRow::as_select())
            .load(connection)?;
        let mut restored_notes = Vec::with_capacity(notes.len());
        for note in notes {
            restored_notes.push(DurableDiagnosticNote {
                span: span(note.span_start, note.span_end)?,
                message: note.message.into(),
            });
        }
        restored.push(DurableDiagnostic {
            severity: row.severity.0,
            code: StableCode(
                u32::try_from(row.code)
                    .map_err(|_| corrupt("a stored diagnostic code is negative"))?,
            ),
            span: span(row.span_start, row.span_end)?,
            message: row.message.into(),
            notes: restored_notes.into_boxed_slice(),
        });
    }
    Ok(restored.into_boxed_slice())
}
