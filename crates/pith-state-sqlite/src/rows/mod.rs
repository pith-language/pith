//! Translation between durable records and the rows that hold them.
//!
//! A completed pure result and a declared action contract keep the canonical
//! bytes `pith-core` produced, because each carries a digest those exact bytes
//! must reproduce. Every other field of a record is a column or a row.

mod attempt;
mod computation;
mod dependency;
mod diagnostic;
mod provenance;

pub use attempt::{
    all_attempt_rows, attempt_computation, attempt_status_column, attempts_for_computation,
    insert_pending_attempt, latest_attempt_row, load_attempt, load_attempts, pending_attempt_rows,
    publish_reusable, reusable_action_attempt_row, reusable_attempt_row,
    reusable_index_attempt_rows, reusable_index_size, write_terminal_state,
};
pub use computation::{find_pure_computation, intern_computation};

use pith_diag::{ByteOffset, Span};
use pith_engine::state::EngineStateError;

/// Either the store rejected an operation or the database did.
pub enum Failure {
    Engine(EngineStateError),
    Database(diesel::result::Error),
}

impl From<diesel::result::Error> for Failure {
    fn from(error: diesel::result::Error) -> Self {
        Self::Database(error)
    }
}

impl From<EngineStateError> for Failure {
    fn from(error: EngineStateError) -> Self {
        Self::Engine(error)
    }
}

impl From<Failure> for EngineStateError {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Engine(error) => error,
            Failure::Database(error) => Self::Adapter {
                message: error.to_string().into(),
            },
        }
    }
}

/// A row that cannot be read back as the record it was written from. Decision
/// 0024: corruption is an adapter error, never a cache miss.
pub(super) fn corrupt(message: impl Into<Box<str>>) -> Failure {
    Failure::Engine(EngineStateError::Adapter {
        message: message.into(),
    })
}

pub(super) fn position(index: usize) -> Result<i32, Failure> {
    i32::try_from(index).map_err(|_| corrupt("an attempt has more rows than a position can index"))
}

pub(super) fn offset(value: i32) -> Result<ByteOffset, Failure> {
    Ok(ByteOffset(u32::try_from(value).map_err(|_| {
        corrupt("a stored span offset is negative")
    })?))
}

pub(super) fn span(start: i32, end: i32) -> Result<Span, Failure> {
    Ok(Span {
        start: offset(start)?,
        end: offset(end)?,
    })
}
