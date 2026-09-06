//! The constraint and requirement codecs: what a clause declared, and what
//! a candidate requires of its own dependencies.

use pith_core::{Type, Value};
use pith_diag::PithResult;

use super::super::model::{ModuleConstraint, ModuleRequirement};
use super::{
    field_of, range_from_value, range_type, range_value, record_type, record_value, subject_of,
    text_of,
};

/// The constraint record type: `{domain, name, range, attribution}`.
#[must_use]
pub fn constraint_type() -> Type {
    record_type([
        ("domain", Type::Text),
        ("name", Type::Text),
        ("range", range_type()),
        ("attribution", Type::Text),
    ])
}

#[must_use]
pub fn constraint_value(constraint: &ModuleConstraint) -> Value {
    record_value([
        (
            "domain",
            Value::Text(constraint.subject.domain().as_str().into()),
        ),
        (
            "name",
            Value::Text(constraint.subject.name().as_str().into()),
        ),
        ("range", range_value(&constraint.range)),
        ("attribution", Value::Text(constraint.attribution.clone())),
    ])
}

/// Decodes a constraint from `value`.
///
/// # Errors
/// Returns a diagnostic when `value` is not a constraint record.
pub fn constraint_from_value(value: &Value) -> PithResult<ModuleConstraint> {
    Ok(ModuleConstraint {
        subject: subject_of(value)?,
        range: range_from_value(field_of(value, "range", "a constraint record")?)?,
        attribution: text_of(
            field_of(value, "attribution", "a constraint record")?,
            "attribution",
        )?,
    })
}

/// The declared type of a canonical constraint list.
#[must_use]
pub fn constraint_set_type() -> Type {
    Type::List(Box::new(constraint_type()))
}

/// A canonical constraint list value: sorted, without duplicates, so
/// construction order never reaches the computation key.
#[must_use]
pub fn constraint_set_value(constraints: &[ModuleConstraint]) -> Value {
    let mut values = constraints.iter().map(constraint_value).collect::<Vec<_>>();
    values.sort_by_key(|value| value.encode_canonical());
    values.dedup();
    Value::List(values.into())
}

/// The requirement record type: `{domain, name, range}`.
#[must_use]
pub fn requirement_type() -> Type {
    record_type([
        ("domain", Type::Text),
        ("name", Type::Text),
        ("range", range_type()),
    ])
}

#[must_use]
pub fn requirement_value(requirement: &ModuleRequirement) -> Value {
    record_value([
        (
            "domain",
            Value::Text(requirement.subject.domain().as_str().into()),
        ),
        (
            "name",
            Value::Text(requirement.subject.name().as_str().into()),
        ),
        ("range", range_value(&requirement.range)),
    ])
}
