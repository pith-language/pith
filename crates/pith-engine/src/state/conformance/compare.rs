//! Comparing the adapter against the reference model.
//!
//! Attempt identifiers are store-local, so records and errors are rewritten
//! into the model's identifier space before they are compared.

use std::sync::Arc;

use pith_core::{ActionComputationKey, PureComputationKey};

use crate::MemoryEngineStateStore;
use crate::state::{DurableAttempt, EngineStateError, EngineStateStore, InvalidationExplanation};

use super::run::Tracked;
use super::translate::{
    Translation, subject_to_model, translate_attempt, translate_error, translate_explanation,
};

#[derive(Clone, Debug)]
pub struct Divergence {
    pub step: usize,
    pub detail: DivergenceDetail,
}

#[derive(Clone, Debug)]
pub enum DivergenceDetail {
    Outcome {
        operation: &'static str,
        model: Result<(), String>,
        subject: Result<(), String>,
    },
    Read {
        query: &'static str,
        model: String,
        subject: String,
    },
    /// An adapter error is never a legitimate answer to a well-formed
    /// operation, whatever the model returned.
    AdapterError {
        operation: &'static str,
        message: String,
    },
}

impl std::fmt::Display for Divergence {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "at step {}: ", self.step)?;
        match &self.detail {
            DivergenceDetail::Outcome {
                operation,
                model,
                subject,
            } => write!(
                formatter,
                "{operation} diverged; the model returned {model:?} and the adapter returned \
                 {subject:?}"
            ),
            DivergenceDetail::Read {
                query,
                model,
                subject,
            } => write!(
                formatter,
                "{query} diverged;\n  model:   {model}\n  adapter: {subject}"
            ),
            DivergenceDetail::AdapterError { operation, message } => {
                write!(
                    formatter,
                    "{operation} failed inside the adapter: {message}"
                )
            }
        }
    }
}

impl std::error::Error for Divergence {}

pub(super) fn compare_outcome(
    index: usize,
    operation: &'static str,
    model: &Result<(), EngineStateError>,
    subject: &Result<(), EngineStateError>,
    tracked: &[Tracked],
) -> Result<(), Divergence> {
    if let Err(EngineStateError::Adapter { message }) = subject {
        return Err(Divergence {
            step: index,
            detail: DivergenceDetail::AdapterError {
                operation,
                message: message.to_string(),
            },
        });
    }
    let translation = subject_to_model(tracked);
    let translated = subject
        .as_ref()
        .map_err(|error| translate_error(error, &translation));
    if model.as_ref().err() == translated.as_ref().err() {
        return Ok(());
    }
    Err(Divergence {
        step: index,
        detail: DivergenceDetail::Outcome {
            operation,
            model: describe(model),
            subject: describe(subject),
        },
    })
}

pub(super) fn describe(outcome: &Result<(), EngineStateError>) -> Result<(), String> {
    outcome.as_ref().map(|_| ()).map_err(ToString::to_string)
}

pub(super) fn compare_reads(
    step: usize,
    model: &MemoryEngineStateStore,
    subject: &dyn EngineStateStore,
    tracked: &[Tracked],
) -> Result<(), Divergence> {
    let translation = subject_to_model(tracked);

    for entry in tracked {
        let expected = read(step, "attempt", || model.attempt(entry.model))?;
        let actual = read(step, "attempt", || subject.attempt(entry.subject))?;
        compare_records(
            step,
            "attempt",
            expected.as_deref(),
            actual.as_deref(),
            &translation,
        )?;
    }

    let mut keys: Vec<PureComputationKey> = tracked
        .iter()
        .filter_map(|entry| entry.computation.pure_key())
        .collect();
    keys.sort_by_key(|key| *key.digest.digest().as_bytes());
    keys.dedup();
    for key in keys {
        compare_sequences(
            step,
            "attempt_history",
            &read(step, "attempt_history", || model.attempt_history(key))?,
            &read(step, "attempt_history", || subject.attempt_history(key))?,
            &translation,
        )?;
        let expected = read(step, "latest_completed_reusable_attempt", || {
            model.latest_completed_reusable_attempt(key)
        })?;
        let actual = read(step, "latest_completed_reusable_attempt", || {
            subject.latest_completed_reusable_attempt(key)
        })?;
        compare_records(
            step,
            "latest_completed_reusable_attempt",
            expected.as_deref(),
            actual.as_deref(),
            &translation,
        )?;
        compare_explanations(
            step,
            "explain_invalidation",
            &read(step, "explain_invalidation", || {
                model.explain_invalidation(key)
            })?,
            &read(step, "explain_invalidation", || {
                subject.explain_invalidation(key)
            })?,
            &translation,
        )?;
    }

    let mut action_keys: Vec<ActionComputationKey> = tracked
        .iter()
        .filter_map(|entry| entry.computation.action_key())
        .collect();
    action_keys.sort_by_key(|key| *key.digest.digest().as_bytes());
    action_keys.dedup();
    for key in action_keys {
        let expected = read(step, "latest_completed_reusable_action_attempt", || {
            model.latest_completed_reusable_action_attempt(key)
        })?;
        let actual = read(step, "latest_completed_reusable_action_attempt", || {
            subject.latest_completed_reusable_action_attempt(key)
        })?;
        compare_records(
            step,
            "latest_completed_reusable_action_attempt",
            expected.as_deref(),
            actual.as_deref(),
            &translation,
        )?;
    }

    compare_sequences(
        step,
        "pending_attempts",
        &read(step, "pending_attempts", || model.pending_attempts())?,
        &read(step, "pending_attempts", || subject.pending_attempts())?,
        &translation,
    )
}

pub(super) fn read<T>(
    step: usize,
    query: &'static str,
    perform: impl FnOnce() -> Result<T, EngineStateError>,
) -> Result<T, Divergence> {
    perform().map_err(|error| Divergence {
        step,
        detail: DivergenceDetail::AdapterError {
            operation: query,
            message: error.to_string(),
        },
    })
}

pub(super) fn compare_sequences(
    step: usize,
    query: &'static str,
    model: &[Arc<DurableAttempt>],
    subject: &[Arc<DurableAttempt>],
    translation: &Translation,
) -> Result<(), Divergence> {
    let model: Vec<DurableAttempt> = model.iter().map(|attempt| (**attempt).clone()).collect();
    let subject: Vec<DurableAttempt> = subject
        .iter()
        .map(|attempt| translate_attempt(attempt, translation))
        .collect();
    if model == subject {
        return Ok(());
    }
    Err(Divergence {
        step,
        detail: DivergenceDetail::Read {
            query,
            model: format!("{model:?}"),
            subject: format!("{subject:?}"),
        },
    })
}

pub(super) fn compare_records(
    step: usize,
    query: &'static str,
    model: Option<&DurableAttempt>,
    subject: Option<&DurableAttempt>,
    translation: &Translation,
) -> Result<(), Divergence> {
    let translated = subject.map(|attempt| translate_attempt(attempt, translation));
    if model.cloned() == translated {
        return Ok(());
    }
    Err(Divergence {
        step,
        detail: DivergenceDetail::Read {
            query,
            model: format!("{model:?}"),
            subject: format!("{subject:?}"),
        },
    })
}

pub(super) fn compare_explanations(
    step: usize,
    query: &'static str,
    model: &Option<InvalidationExplanation>,
    subject: &Option<InvalidationExplanation>,
    translation: &Translation,
) -> Result<(), Divergence> {
    let translated = subject
        .as_ref()
        .map(|explanation| translate_explanation(explanation, translation));
    if model == &translated {
        return Ok(());
    }
    Err(Divergence {
        step,
        detail: DivergenceDetail::Read {
            query,
            model: format!("{model:?}"),
            subject: format!("{subject:?}"),
        },
    })
}
