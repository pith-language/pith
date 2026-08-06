//! Canonical payload encoding for the currently implemented [`Type`] and
//! [`Value`] variants.

use crate::{
    Type, Value,
    manifest::{encode_bytes, encode_str},
};

/// Discriminant tags for `Type`/`Value` variants. `Type` and `Value`
/// deliberately share numbering for their overlapping variants so a value and
/// its type encode under the same tag; `Nominal` is `Type`-only. These tags are
/// already part of the stable pure-computation digest format.
pub(crate) const TAG_UNIT: u8 = 0;
pub(crate) const TAG_BOOL: u8 = 1;
pub(crate) const TAG_INT: u8 = 2;
pub(crate) const TAG_TEXT: u8 = 3;
pub(crate) const TAG_BYTES: u8 = 4;
pub(crate) const TAG_BLOB: u8 = 5;
pub(crate) const TAG_NOMINAL: u8 = 6;

pub(crate) fn encode_type_payload(encoded: &mut Vec<u8>, value_type: &Type) {
    match value_type {
        Type::Unit => encoded.push(TAG_UNIT),
        Type::Bool => encoded.push(TAG_BOOL),
        Type::Int => encoded.push(TAG_INT),
        Type::Text => encoded.push(TAG_TEXT),
        Type::Bytes => encoded.push(TAG_BYTES),
        Type::Blob => encoded.push(TAG_BLOB),
        Type::Nominal { name } => {
            encoded.push(TAG_NOMINAL);
            encode_str(encoded, name);
        }
    }
}

pub(crate) fn encode_value_payload(encoded: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Unit => encoded.push(TAG_UNIT),
        Value::Bool(value) => {
            encoded.push(TAG_BOOL);
            encoded.push(u8::from(*value));
        }
        Value::Int(value) => {
            encoded.push(TAG_INT);
            encoded.extend_from_slice(&value.to_le_bytes());
        }
        Value::Text(value) => {
            encoded.push(TAG_TEXT);
            encode_str(encoded, value);
        }
        Value::Bytes(value) => {
            encoded.push(TAG_BYTES);
            encode_bytes(encoded, value);
        }
        Value::Blob(value) => {
            encoded.push(TAG_BLOB);
            encoded.extend_from_slice(value.digest().as_bytes());
        }
    }
}
