//! The one spelling of the value operations every phloem module needs:
//! build a record type, build a record value, build a sum value, pull a
//! typed field out of a record by name.
//!
//! Every declared shape here is a record or a sum built from field-name
//! constants, and every read walks a record's fields by those same
//! constants. One home for those operations keeps the round-trip halves
//! derivable from one list of names rather than re-spelled per module.

use pith_core::{RecordField, Type, Value};
use pith_diag::PithResult;
use pith_ids::ContentId;

use crate::diag;

/// Build a record type from `(name, payload)` pairs. Field names are
/// distinct constants at every call site, so the duplicate-name rejection
/// is unreachable by construction.
pub(crate) fn record_type<const N: usize>(fields: [(&str, Type); N]) -> Type {
    let record = Type::record(fields.map(|(name, payload)| RecordField {
        name: name.into(),
        payload,
    }));
    record.unwrap_or_else(|error| unreachable!("{error}"))
}

/// Build a record value from `(name, payload)` pairs, on the same terms as
/// [`record_type`].
pub(crate) fn record_value<const N: usize>(fields: [(&str, Value); N]) -> Value {
    let record = Value::record(fields.map(|(name, payload)| RecordField {
        name: name.into(),
        payload,
    }));
    record.unwrap_or_else(|error| unreachable!("{error}"))
}

/// Build a value of one constructor of a declared sum: the sum's name, the
/// selected constructor, and its payload.
pub(crate) fn sum_value(type_name: &str, constructor: &str, payload: Option<Value>) -> Value {
    Value::Sum {
        type_name: type_name.into(),
        constructor: constructor.into(),
        payload: payload.map(Box::new),
    }
}

/// A record's field payload by name, or `None` when the record has no such
/// field.
pub(crate) fn field_of<'a>(fields: &'a [RecordField<Value>], name: &str) -> Option<&'a Value> {
    fields
        .iter()
        .find(|field| field.name.as_ref() == name)
        .map(|field| &field.payload)
}

/// A payload as text, naming the field it was read from.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming what the field carried when the
/// payload is not a text.
pub(crate) fn text_of(value: &Value, field: &str) -> PithResult<Box<str>> {
    match value {
        Value::Text(text) => Ok(text.clone()),
        _ => Err(diag(format!(
            "the {field} field carried {} rather than a text",
            value.describe()
        ))),
    }
}

/// A payload as a list of texts, naming the field it was read from.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming what the field carried when the
/// payload is not a list of texts.
pub(crate) fn text_list(value: &Value, field: &str) -> PithResult<Vec<Box<str>>> {
    let Value::List(elements) = value else {
        return Err(diag(format!(
            "the {field} field carried {} rather than a list",
            value.describe()
        )));
    };
    let mut texts = Vec::with_capacity(elements.len());
    for element in elements.iter() {
        texts.push(text_of(element, field)?);
    }
    Ok(texts)
}

/// A record's text field by name.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming what the field carried when the
/// record has no such field or the field is not a text.
pub(crate) fn text_field(fields: &[RecordField<Value>], name: &str) -> PithResult<Box<str>> {
    match field_of(fields, name) {
        Some(payload) => text_of(payload, name),
        None => Err(diag(format!("the {name} field carried no text"))),
    }
}

/// A record's blob field by name.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming what the field carried when the
/// record has no such field or the field is not a blob.
pub(crate) fn blob_field(fields: &[RecordField<Value>], name: &str) -> PithResult<ContentId> {
    match field_of(fields, name) {
        Some(Value::Blob(id)) => Ok(*id),
        _ => Err(diag(format!("the {name} field carried no blob"))),
    }
}

/// A record's integer field by name, as a `u64`.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming what the field carried when the
/// record has no such field, the field is not an integer, or the integer is
/// negative.
pub(crate) fn int_field(fields: &[RecordField<Value>], name: &str) -> PithResult<u64> {
    match field_of(fields, name) {
        Some(Value::Int(n)) => u64::try_from(*n)
            .map_err(|_| crate::diag(format!("the {name} field carried a negative integer"))),
        _ => Err(diag(format!("the {name} field carried no integer"))),
    }
}
