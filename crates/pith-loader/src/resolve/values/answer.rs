//! The answer codecs: a solve's selections, trail, derivation, and the
//! resolution sum.

use pith_core::{Int, SumConstructor, Type, Value};
use pith_diag::PithResult;
use pith_ids::ContentId;

use super::super::model::{Derivation, ModuleResolution, ModuleSelection, TrailEntry};
use super::{
    constraint_set_type, constraint_set_value, count_of, decode_error, field_of, payload_of,
    record_type, record_value, subject_of, sum_type, sum_value, version_of,
};

const SOLVED: &str = "Solved";
const UNSATISFIABLE: &str = "Unsatisfiable";
const UNDERDETERMINED: &str = "Underdetermined";
const BUDGET_EXHAUSTED: &str = "BudgetExhausted";

/// The selection record type: `{domain, name, version}`, the lock-entry
/// shape.
#[must_use]
pub fn selection_type() -> Type {
    record_type([
        ("domain", Type::Text),
        ("name", Type::Text),
        ("version", Type::Text),
    ])
}

#[must_use]
pub fn selection_value(selection: &ModuleSelection) -> Value {
    record_value([
        (
            "domain",
            Value::Text(selection.subject.domain().as_str().into()),
        ),
        (
            "name",
            Value::Text(selection.subject.name().as_str().into()),
        ),
        (
            "version",
            Value::Text(selection.version.canonical_spelling()),
        ),
    ])
}

/// The trail-entry record type: `{domain, name, considered, decided_by}`.
#[must_use]
pub fn trail_type() -> Type {
    record_type([
        ("domain", Type::Text),
        ("name", Type::Text),
        ("considered", Type::Int),
        ("decided_by", Type::Text),
    ])
}

#[must_use]
pub fn trail_value(entry: &TrailEntry) -> Value {
    record_value([
        (
            "domain",
            Value::Text(entry.subject.domain().as_str().into()),
        ),
        ("name", Value::Text(entry.subject.name().as_str().into())),
        ("considered", Value::int(count_of(entry.considered))),
        ("decided_by", Value::Text(entry.decided_by.clone())),
    ])
}

/// The derivation record type: `{domain, name, constraints, candidates}`.
#[must_use]
pub fn derivation_type() -> Type {
    record_type([
        ("domain", Type::Text),
        ("name", Type::Text),
        ("constraints", constraint_set_type()),
        ("candidates", Type::Int),
    ])
}

#[must_use]
pub fn derivation_value(derivation: &Derivation) -> Value {
    record_value([
        (
            "domain",
            Value::Text(derivation.subject.domain().as_str().into()),
        ),
        (
            "name",
            Value::Text(derivation.subject.name().as_str().into()),
        ),
        ("constraints", constraint_set_value(&derivation.constraints)),
        ("candidates", Value::int(count_of(derivation.candidates))),
    ])
}

/// The declared resolution sum.
#[must_use]
pub fn resolution_type() -> Type {
    sum_type(
        "Resolution",
        [
            SumConstructor {
                name: SOLVED.into(),
                payload: Some(record_type([
                    ("selections", Type::List(Box::new(selection_type()))),
                    ("trail", Type::List(Box::new(trail_type()))),
                    ("universe", Type::Text),
                ])),
            },
            SumConstructor {
                name: UNSATISFIABLE.into(),
                payload: Some(derivation_type()),
            },
            SumConstructor {
                name: UNDERDETERMINED.into(),
                payload: Some(record_type([
                    ("domain", Type::Text),
                    ("name", Type::Text),
                    ("tied", Type::List(Box::new(Type::Text))),
                ])),
            },
            SumConstructor {
                name: BUDGET_EXHAUSTED.into(),
                payload: Some(record_type([
                    ("budget", Type::Int),
                    ("decisions", Type::Int),
                ])),
            },
        ],
    )
}

#[must_use]
pub fn resolution_value(resolution: &ModuleResolution) -> Value {
    match resolution {
        ModuleResolution::Solved {
            selections,
            trail,
            universe,
        } => sum_value(
            "Resolution",
            SOLVED,
            Some(record_value([
                (
                    "selections",
                    Value::List(
                        selections
                            .iter()
                            .map(selection_value)
                            .collect::<Vec<_>>()
                            .into(),
                    ),
                ),
                (
                    "trail",
                    Value::List(trail.iter().map(trail_value).collect::<Vec<_>>().into()),
                ),
                ("universe", Value::Text(universe_identity(universe))),
            ])),
        ),
        ModuleResolution::Unsatisfiable { derivation } => sum_value(
            "Resolution",
            UNSATISFIABLE,
            Some(derivation_value(derivation)),
        ),
        ModuleResolution::Underdetermined { subject, tied } => sum_value(
            "Resolution",
            UNDERDETERMINED,
            Some(record_value([
                ("domain", Value::Text(subject.domain().as_str().into())),
                ("name", Value::Text(subject.name().as_str().into())),
                (
                    "tied",
                    Value::List(
                        tied.iter()
                            .map(|identity| Value::Text(identity.clone()))
                            .collect::<Vec<_>>()
                            .into(),
                    ),
                ),
            ])),
        ),
        ModuleResolution::BudgetExhausted { budget, decisions } => sum_value(
            "Resolution",
            BUDGET_EXHAUSTED,
            Some(record_value([
                ("budget", Value::int(Int::from(*budget))),
                ("decisions", Value::int(Int::from(*decisions))),
            ])),
        ),
    }
}

/// The universe identity a solved answer records, as the hex spelling of
/// its digest, so a lock can carry it and an invalidation explanation can
/// name it.
#[must_use]
pub fn universe_identity(universe: &ContentId) -> Box<str> {
    universe.digest().to_string().into()
}

/// The selections a solved answer carries.
///
/// # Errors
/// Returns a diagnostic when `value` is not a solved resolution.
pub fn selections_from_value(value: &Value) -> PithResult<Box<[ModuleSelection]>> {
    let Value::Sum {
        constructor,
        payload,
        ..
    } = value
    else {
        return Err(decode_error("a solved modules.Resolution", value));
    };
    if constructor.as_ref() != SOLVED {
        return Err(decode_error("a solved modules.Resolution", value));
    }
    let solved = payload_of(payload, SOLVED, value)?;
    match field_of(solved, "selections", "a solved payload")? {
        Value::List(entries) => entries
            .iter()
            .map(|entry| {
                Ok(ModuleSelection {
                    subject: subject_of(entry)?,
                    version: version_of(field_of(entry, "version", "a selection record")?)?,
                })
            })
            .collect(),
        other => Err(decode_error("a selection list", other)),
    }
}
