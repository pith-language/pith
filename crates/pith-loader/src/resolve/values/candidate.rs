//! The candidate and universe codecs: what an acquired universe holds.

use pith_core::{Type, Value};
use pith_diag::PithResult;

use super::super::model::{ModuleCandidate, ModuleRequirement};
use super::constraint::{requirement_type, requirement_value};
use super::{
    decode_error, field_of, range_from_value, record_type, record_value, subject_of, text_of,
    version_of,
};

/// The candidate record type: `{domain, name, version, requires, origin}`.
#[must_use]
pub fn candidate_type() -> Type {
    record_type([
        ("domain", Type::Text),
        ("name", Type::Text),
        ("version", Type::Text),
        ("requires", Type::List(Box::new(requirement_type()))),
        ("origin", Type::Text),
    ])
}

#[must_use]
pub fn candidate_value(candidate: &ModuleCandidate) -> Value {
    record_value([
        (
            "domain",
            Value::Text(candidate.subject.domain().as_str().into()),
        ),
        (
            "name",
            Value::Text(candidate.subject.name().as_str().into()),
        ),
        (
            "version",
            Value::Text(candidate.version.canonical_spelling()),
        ),
        (
            "requires",
            Value::List(
                candidate
                    .requires
                    .iter()
                    .map(requirement_value)
                    .collect::<Vec<_>>()
                    .into(),
            ),
        ),
        ("origin", Value::Text(candidate.origin.clone())),
    ])
}

/// Decodes a candidate from `value`.
///
/// # Errors
/// Returns a diagnostic when `value` is not a candidate record.
pub fn candidate_from_value(value: &Value) -> PithResult<ModuleCandidate> {
    let version = version_of(field_of(value, "version", "a candidate record")?)?;
    let requires = match field_of(value, "requires", "a candidate record")? {
        Value::List(entries) => entries
            .iter()
            .map(|entry| {
                Ok(ModuleRequirement {
                    subject: subject_of(entry)?,
                    range: range_from_value(field_of(entry, "range", "a requirement record")?)?,
                })
            })
            .collect::<PithResult<Vec<_>>>(),
        other => Err(decode_error("a requirement list", other)),
    }?;
    Ok(ModuleCandidate {
        subject: subject_of(value)?,
        version,
        requires: requires.into(),
        origin: text_of(field_of(value, "origin", "a candidate record")?, "origin")?,
    })
}

/// The declared type of a candidate universe: the canonical candidate
/// list.
#[must_use]
pub fn universe_type() -> Type {
    Type::List(Box::new(candidate_type()))
}

#[must_use]
pub fn universe_value(candidates: &[ModuleCandidate]) -> Value {
    Value::List(
        candidates
            .iter()
            .map(candidate_value)
            .collect::<Vec<_>>()
            .into(),
    )
}

/// Decodes a candidate universe from `value`.
///
/// # Errors
/// Returns a diagnostic when `value` is not a candidate list.
pub fn universe_from_value(value: &Value) -> PithResult<Vec<ModuleCandidate>> {
    let Value::List(entries) = value else {
        return Err(decode_error("a candidate universe", value));
    };
    entries.iter().map(candidate_from_value).collect()
}
