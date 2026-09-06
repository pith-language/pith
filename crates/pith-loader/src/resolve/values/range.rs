//! The declared range sum: the five constructors a manifest writes, and
//! their codec.

use pith_core::{SumConstructor, Type, Value};
use pith_diag::PithResult;
use pith_hir::{VersionBound, VersionRange};

use super::{
    decode_error, field_of, payload_of, record_type, record_value, sum_type, sum_value, version_of,
};

const ANY: &str = "Any";
const EXACTLY: &str = "Exactly";
const AT_LEAST: &str = "AtLeast";
const AT_MOST: &str = "AtMost";
const BETWEEN: &str = "Between";

/// The range bound type: `{version: Text, inclusive: Bool}`.
fn bound_type() -> Type {
    record_type([("version", Type::Text), ("inclusive", Type::Bool)])
}

fn bound_value(bound: &VersionBound) -> Value {
    record_value([
        ("version", Value::Text(bound.version.canonical_spelling())),
        ("inclusive", Value::Bool(bound.inclusive)),
    ])
}

fn bound_from_value(value: &Value) -> PithResult<VersionBound> {
    let version = field_of(value, "version", "a range bound record")?;
    let inclusive = field_of(value, "inclusive", "a range bound record")?;
    let Value::Bool(inclusive) = inclusive else {
        return Err(decode_error("a boolean `inclusive` edge", value));
    };
    Ok(VersionBound {
        version: version_of(version)?,
        inclusive: *inclusive,
    })
}

/// The declared range sum: the five constructors the manifest writes.
#[must_use]
pub fn range_type() -> Type {
    sum_type(
        "Range",
        [
            SumConstructor {
                name: ANY.into(),
                payload: None,
            },
            SumConstructor {
                name: EXACTLY.into(),
                payload: Some(Type::Text),
            },
            SumConstructor {
                name: AT_LEAST.into(),
                payload: Some(bound_type()),
            },
            SumConstructor {
                name: AT_MOST.into(),
                payload: Some(bound_type()),
            },
            SumConstructor {
                name: BETWEEN.into(),
                payload: Some(record_type([
                    ("lower", bound_type()),
                    ("upper", bound_type()),
                ])),
            },
        ],
    )
}

#[must_use]
pub fn range_value(range: &VersionRange) -> Value {
    match range {
        VersionRange::Any => sum_value("Range", ANY, None),
        VersionRange::Exactly(version) => sum_value(
            "Range",
            EXACTLY,
            Some(Value::Text(version.canonical_spelling())),
        ),
        VersionRange::AtLeast(bound) => sum_value("Range", AT_LEAST, Some(bound_value(bound))),
        VersionRange::AtMost(bound) => sum_value("Range", AT_MOST, Some(bound_value(bound))),
        VersionRange::Between { lower, upper } => sum_value(
            "Range",
            BETWEEN,
            Some(record_value([
                ("lower", bound_value(lower)),
                ("upper", bound_value(upper)),
            ])),
        ),
    }
}

/// Decodes a range from `value`.
///
/// # Errors
/// Returns a diagnostic when `value` is not a declared range.
pub fn range_from_value(value: &Value) -> PithResult<VersionRange> {
    let Value::Sum {
        constructor,
        payload,
        ..
    } = value
    else {
        return Err(decode_error("a modules.Range", value));
    };
    match constructor.as_ref() {
        ANY => Ok(VersionRange::Any),
        EXACTLY => Ok(VersionRange::Exactly(version_of(payload_of(
            payload, EXACTLY, value,
        )?)?)),
        AT_LEAST => Ok(VersionRange::AtLeast(bound_from_value(payload_of(
            payload, AT_LEAST, value,
        )?)?)),
        AT_MOST => Ok(VersionRange::AtMost(bound_from_value(payload_of(
            payload, AT_MOST, value,
        )?)?)),
        BETWEEN => {
            let between = payload_of(payload, BETWEEN, value)?;
            let lower = bound_from_value(field_of(between, "lower", "a between payload")?)?;
            let upper = bound_from_value(field_of(between, "upper", "a between payload")?)?;
            Ok(VersionRange::Between { lower, upper })
        }
        _ => Err(decode_error("a modules.Range", value)),
    }
}
