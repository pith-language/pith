//! Shared constructors and readers for phloem value codecs.
//!
//! Record and sum codecs use the field-name constants defined here so their
//! encoders and decoders share one spelling.

use pith_core::{RecordField, Type, Value};
use pith_diag::PithResult;
use pith_ids::{ContentDigest, ContentId, DIGEST_LEN, DigestDomain};

use crate::diag;

pub(crate) const FIELD_ARCHITECTURE: &str = "architecture";
pub(crate) const FIELD_DOMAIN: &str = "domain";
pub(crate) const FIELD_FEATURES: &str = "features";
pub(crate) const FIELD_OPERATING_SYSTEM: &str = "operating-system";
pub(crate) const FIELD_PACKAGE: &str = "package";
pub(crate) const FIELD_TOOLCHAIN: &str = "toolchain";
pub(crate) const FIELD_VERSION: &str = "version";

/// Builds a record type from `(name, payload)` pairs. Call sites pass
/// distinct name constants, so the duplicate-name rejection never fires.
pub(crate) fn record_type<const N: usize>(fields: [(&str, Type); N]) -> Type {
    let record = Type::record(fields.map(|(name, payload)| RecordField {
        name: name.into(),
        payload,
    }));
    record.unwrap_or_else(|error| unreachable!("{error}"))
}

/// Builds a record value from `(name, payload)` pairs, on the same terms as
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

/// Values in canonical byte order, with duplicate values collapsed.
pub(crate) fn canonical_list(values: impl IntoIterator<Item = Value>) -> Value {
    let mut entries: Vec<(Vec<u8>, Value)> = values
        .into_iter()
        .map(|value| (value.encode_canonical(), value))
        .collect();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries.dedup_by(|left, right| left.0 == right.0);
    Value::List(entries.into_iter().map(|(_, value)| value).collect())
}

pub(crate) fn value_content_id(domain: DigestDomain, value: &Value) -> ContentId {
    ContentId::of_domain(domain, &value.encode_canonical())
}

/// Decodes a digest written as hexadecimal.
pub(crate) fn digest_from_hex(text: &str) -> Option<ContentDigest> {
    let mut bytes = [0; DIGEST_LEN];
    pith_ids::decode_hex_into(&mut bytes, text).then(|| ContentDigest::from_bytes(bytes))
}

/// A record's field payload by name, or `None` when absent.
pub(crate) fn field_of<'a>(fields: &'a [RecordField<Value>], name: &str) -> Option<&'a Value> {
    fields
        .iter()
        .find(|field| field.name.as_ref() == name)
        .map(|field| &field.payload)
}

/// A payload as text, naming the field it was read from.
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

/// A record's text field by name, naming the field in the diagnostic.
pub(crate) fn text_field(fields: &[RecordField<Value>], name: &str) -> PithResult<Box<str>> {
    match field_of(fields, name) {
        Some(payload) => text_of(payload, name),
        None => Err(diag(format!("the {name} field carried no text"))),
    }
}

/// A record's blob field by name, naming the field in the diagnostic.
pub(crate) fn blob_field(fields: &[RecordField<Value>], name: &str) -> PithResult<ContentId> {
    match field_of(fields, name) {
        Some(Value::Blob(id)) => Ok(*id),
        Some(found) => Err(diag(format!(
            "the {name} field carried {} rather than a blob",
            found.describe()
        ))),
        None => Err(diag(format!("the record carried no {name} field"))),
    }
}

/// Reads a record's integer field as a `u64`.
///
/// # Errors
/// When the field is missing, not an integer, or outside the `u64` range,
/// which the kernel's arbitrary-precision integers can exceed from either
/// end.
pub(crate) fn int_field(fields: &[RecordField<Value>], name: &str) -> PithResult<u64> {
    match field_of(fields, name) {
        Some(Value::Int(n)) => n
            .to_i64()
            .and_then(|value| u64::try_from(value).ok())
            .ok_or_else(|| {
                crate::diag(format!(
                    "the {name} field carried the integer {n}, which is not a count"
                ))
            }),
        Some(found) => Err(diag(format!(
            "the {name} field carried {} rather than an integer",
            found.describe()
        ))),
        None => Err(diag(format!("the record carried no {name} field"))),
    }
}
