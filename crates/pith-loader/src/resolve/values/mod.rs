//! The module resolver's value layer: the types a resolve request carries
//! and the codecs between them and the resolver's own vocabulary.
//!
//! Types are structural and named by coordinate — `modules.Range`,
//! `modules.Constraint`, `modules.Resolution` — so registering the
//! resolver loads no module surface first: its bootstrap declarations are
//! this module, and nothing resolves the module system's own source to
//! reach them.
//!
//! The record, nominal, and sum construction helpers live here once; the
//! codec modules below share them so one field-name spelling cannot drift
//! between a type and its codec.

mod answer;
mod candidate;
mod constraint;
mod preference;
mod range;

pub use answer::{resolution_type, resolution_value, selections_from_value};
pub use candidate::{universe_from_value, universe_type, universe_value};
pub use constraint::{constraint_from_value, constraint_set_type, constraint_set_value};
pub use preference::{preference_from_value, preference_type, preference_value};
pub use range::{range_from_value, range_type, range_value};

use pith_core::{Coordinate, Int, NominalType, RecordField, SumConstructor, SumType, Type, Value};
use pith_diag::{DiagnosticSink, PithResult, Severity, Span};
use pith_hir::{FrontendCode, ManifestVersion, ModuleSubject};

/// The module coordinate every resolver type is named under.
pub const MODULES_MODULE: &str = "modules";

/// A length as a count value.
pub(super) fn count_of(len: usize) -> Int {
    Int::from(u64::try_from(len).unwrap_or(u64::MAX))
}

pub(super) fn named(name: &str) -> Box<str> {
    format!("{MODULES_MODULE}.{name}").into()
}

pub(super) fn record_type<const N: usize>(fields: [(&str, Type); N]) -> Type {
    let record = Type::record(fields.map(|(name, payload)| RecordField {
        name: name.into(),
        payload,
    }));
    record.unwrap_or_else(|error| unreachable!("{error}"))
}

pub(super) fn record_value<const N: usize>(fields: [(&str, Value); N]) -> Value {
    let record = Value::record(fields.map(|(name, payload)| RecordField {
        name: name.into(),
        payload,
    }));
    record.unwrap_or_else(|error| unreachable!("{error}"))
}

pub(super) fn nominal_type(name: &str, representation: Type) -> Type {
    Type::Nominal(Box::new(NominalType {
        coordinate: Coordinate::new(MODULES_MODULE, name),
        representation,
    }))
}

pub(super) fn nominal_value(name: &str, representation: Value) -> Value {
    Value::Nominal {
        name: named(name),
        representation: Box::new(representation),
    }
}

pub(super) fn sum_type(name: &str, constructors: impl Into<Box<[SumConstructor]>>) -> Type {
    Type::Sum(Box::new(SumType {
        coordinate: Coordinate::new(MODULES_MODULE, name),
        constructors: constructors.into(),
    }))
}

pub(super) fn sum_value(sum: &str, constructor: &str, payload: Option<Value>) -> Value {
    Value::Sum {
        type_name: named(sum),
        constructor: constructor.into(),
        payload: payload.map(Box::new),
    }
}

/// Why a resolver value could not be decoded: a defect in the request's
/// construction, named with the type that was expected.
pub(super) fn decode_error(expected: &str, found: &Value) -> DiagnosticSink {
    let mut sink = DiagnosticSink::new();
    sink.push(pith_diag::Diag::new(
        Severity::Error,
        FrontendCode::MalformedResolution.stable(),
        Span::none(),
        format!("expected {expected}, found {}", found.describe()),
    ));
    sink
}

/// Reads a sum constructor's payload, refusing a missing one.
pub(super) fn payload_of<'value>(
    payload: &'value Option<Box<Value>>,
    constructor: &str,
    whole: &Value,
) -> PithResult<&'value Value> {
    payload
        .as_deref()
        .ok_or_else(|| decode_error(&format!("the `{constructor}` constructor's payload"), whole))
}

/// Reads a record field, refusing its absence.
pub(super) fn field_of<'value>(
    value: &'value Value,
    field: &str,
    expected: &str,
) -> PithResult<&'value Value> {
    let Value::Record(fields) = value else {
        return Err(decode_error(expected, value));
    };
    fields
        .iter()
        .find(|candidate| candidate.name.as_ref() == field)
        .map(|candidate| &candidate.payload)
        .ok_or_else(|| decode_error(expected, value))
}

pub(super) fn text_of(value: &Value, field: &str) -> PithResult<Box<str>> {
    match value {
        Value::Text(text) => Ok(text.clone()),
        other => Err(decode_error(&format!("text in `{field}`"), other)),
    }
}

/// A subject from its `domain`/`name` record fields.
pub(super) fn subject_of(value: &Value) -> PithResult<ModuleSubject> {
    let spelling = format!(
        "{}/{}",
        text_of(field_of(value, "domain", "a constraint record")?, "domain")?,
        text_of(field_of(value, "name", "a constraint record")?, "name")?,
    );
    ModuleSubject::parse(&spelling).map_err(|error| {
        let mut sink = DiagnosticSink::new();
        sink.push(pith_diag::Diag::new(
            Severity::Error,
            FrontendCode::InvalidSubject.stable(),
            Span::none(),
            error.to_string(),
        ));
        sink
    })
}

/// A version from its canonical dotted spelling.
pub(super) fn version_of(value: &Value) -> PithResult<ManifestVersion> {
    let spelling = text_of(value, "version")?;
    ManifestVersion::parse(&spelling).map_err(|_| decode_error("a version spelling", value))
}
