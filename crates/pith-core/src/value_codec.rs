//! Canonical payload encoding for the currently implemented [`Type`] and
//! [`Value`] variants.

use std::mem::size_of;

use pith_ids::{ContentDigest, ContentId, DIGEST_LEN};

use crate::{
    Type, Value,
    manifest::{encode_bytes, encode_str},
};

const ENCODING_VERSION: u8 = 1;

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

/// Failure to decode a canonical [`Type`] or [`Value`] encoding.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CanonicalDecodeError {
    UnsupportedVersion { version: u8 },
    UnknownTypeTag { tag: u8 },
    UnknownValueTag { tag: u8 },
    InvalidBoolean { byte: u8 },
    Truncated,
    TrailingBytes,
    InvalidUtf8,
    LengthOutOfRange { length: u64 },
}

impl std::fmt::Display for CanonicalDecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion { version } => {
                write!(
                    formatter,
                    "unsupported canonical encoding version {version}"
                )
            }
            Self::UnknownTypeTag { tag } => {
                write!(formatter, "unknown canonical type tag {tag}")
            }
            Self::UnknownValueTag { tag } => {
                write!(formatter, "unknown canonical value tag {tag}")
            }
            Self::InvalidBoolean { byte } => {
                write!(formatter, "invalid canonical boolean byte {byte}")
            }
            Self::Truncated => formatter.write_str("canonical encoding ended unexpectedly"),
            Self::TrailingBytes => formatter.write_str("canonical encoding has trailing bytes"),
            Self::InvalidUtf8 => formatter.write_str("canonical encoding contains invalid UTF-8"),
            Self::LengthOutOfRange { length } => write!(
                formatter,
                "canonical encoding length {length} is not representable"
            ),
        }
    }
}

impl std::error::Error for CanonicalDecodeError {}

impl Type {
    /// Encode this type in the current version of Pith's canonical wire format.
    #[must_use]
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut encoded = vec![ENCODING_VERSION];
        encode_type_payload(&mut encoded, self);
        encoded
    }

    /// Decode one type from Pith's versioned canonical wire format.
    ///
    /// # Errors
    /// Returns an error for unsupported versions, unknown type tags, truncated
    /// or trailing data, invalid UTF-8, and lengths not representable on the
    /// current platform.
    pub fn decode_canonical(encoded: &[u8]) -> Result<Self, CanonicalDecodeError> {
        let mut decoder = Decoder::new(encoded);
        decoder.read_version()?;
        let value_type = match decoder.read_byte()? {
            TAG_UNIT => Self::Unit,
            TAG_BOOL => Self::Bool,
            TAG_INT => Self::Int,
            TAG_TEXT => Self::Text,
            TAG_BYTES => Self::Bytes,
            TAG_BLOB => Self::Blob,
            TAG_NOMINAL => Self::Nominal {
                name: decoder.read_text()?.into(),
            },
            tag => return Err(CanonicalDecodeError::UnknownTypeTag { tag }),
        };
        decoder.finish()?;
        Ok(value_type)
    }
}

impl Value {
    /// Encode this value in the current version of Pith's canonical wire format.
    #[must_use]
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut encoded = vec![ENCODING_VERSION];
        encode_value_payload(&mut encoded, self);
        encoded
    }

    /// Decode one value from Pith's versioned canonical wire format.
    ///
    /// # Errors
    /// Returns an error for unsupported versions, unknown value tags, malformed
    /// booleans, truncated or trailing data, invalid UTF-8, and lengths not
    /// representable on the current platform.
    pub fn decode_canonical(encoded: &[u8]) -> Result<Self, CanonicalDecodeError> {
        let mut decoder = Decoder::new(encoded);
        decoder.read_version()?;
        let value = match decoder.read_byte()? {
            TAG_UNIT => Self::Unit,
            TAG_BOOL => match decoder.read_byte()? {
                0 => Self::Bool(false),
                1 => Self::Bool(true),
                byte => return Err(CanonicalDecodeError::InvalidBoolean { byte }),
            },
            TAG_INT => Self::Int(decoder.read_int()?),
            TAG_TEXT => Self::Text(decoder.read_text()?.into()),
            TAG_BYTES => Self::Bytes(decoder.read_bytes()?.into()),
            TAG_BLOB => Self::Blob(decoder.read_content_id()?),
            tag => return Err(CanonicalDecodeError::UnknownValueTag { tag }),
        };
        decoder.finish()?;
        Ok(value)
    }
}

struct Decoder<'encoded> {
    remaining: &'encoded [u8],
}

impl<'encoded> Decoder<'encoded> {
    fn new(encoded: &'encoded [u8]) -> Self {
        Self { remaining: encoded }
    }

    fn read_version(&mut self) -> Result<(), CanonicalDecodeError> {
        let version = self.read_byte()?;
        if version != ENCODING_VERSION {
            return Err(CanonicalDecodeError::UnsupportedVersion { version });
        }
        Ok(())
    }

    fn read_byte(&mut self) -> Result<u8, CanonicalDecodeError> {
        let Some((byte, remaining)) = self.remaining.split_first() else {
            return Err(CanonicalDecodeError::Truncated);
        };
        self.remaining = remaining;
        Ok(*byte)
    }

    fn read_int(&mut self) -> Result<i64, CanonicalDecodeError> {
        let bytes = self.take(size_of::<i64>())?;
        let mut encoded = [0; size_of::<i64>()];
        encoded.copy_from_slice(bytes);
        Ok(i64::from_le_bytes(encoded))
    }

    fn read_length(&mut self) -> Result<usize, CanonicalDecodeError> {
        let bytes = self.take(size_of::<u64>())?;
        let mut encoded = [0; size_of::<u64>()];
        encoded.copy_from_slice(bytes);
        let length = u64::from_le_bytes(encoded);
        usize::try_from(length).map_err(|_| CanonicalDecodeError::LengthOutOfRange { length })
    }

    fn read_bytes(&mut self) -> Result<&'encoded [u8], CanonicalDecodeError> {
        let length = self.read_length()?;
        self.take(length)
    }

    fn read_text(&mut self) -> Result<&'encoded str, CanonicalDecodeError> {
        std::str::from_utf8(self.read_bytes()?).map_err(|_| CanonicalDecodeError::InvalidUtf8)
    }

    fn read_content_id(&mut self) -> Result<ContentId, CanonicalDecodeError> {
        let bytes = self.take(DIGEST_LEN)?;
        let mut digest = [0; DIGEST_LEN];
        digest.copy_from_slice(bytes);
        Ok(ContentId::from_digest(ContentDigest::from_bytes(digest)))
    }

    fn take(&mut self, length: usize) -> Result<&'encoded [u8], CanonicalDecodeError> {
        if self.remaining.len() < length {
            return Err(CanonicalDecodeError::Truncated);
        }
        let (value, remaining) = self.remaining.split_at(length);
        self.remaining = remaining;
        Ok(value)
    }

    fn finish(self) -> Result<(), CanonicalDecodeError> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(CanonicalDecodeError::TrailingBytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_content_id() -> ContentId {
        ContentId::from_digest(ContentDigest::from_bytes([
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ]))
    }

    #[test]
    fn every_type_round_trips() {
        for value_type in [
            Type::Unit,
            Type::Bool,
            Type::Int,
            Type::Text,
            Type::Bytes,
            Type::Blob,
            Type::Nominal {
                name: "Machine".into(),
            },
        ] {
            let encoded = value_type.encode_canonical();
            assert_eq!(Type::decode_canonical(&encoded), Ok(value_type));
        }
    }

    #[test]
    fn every_value_round_trips() {
        for value in [
            Value::Unit,
            Value::Bool(false),
            Value::Bool(true),
            Value::Int(i64::MIN),
            Value::Int(i64::MAX),
            Value::Text("Pith \u{03bb}".into()),
            Value::Bytes(vec![0x00, 0x80, 0xff].into_boxed_slice()),
            Value::Blob(fixture_content_id()),
        ] {
            let encoded = value.encode_canonical();
            assert_eq!(Value::decode_canonical(&encoded), Ok(value));
        }
    }

    #[test]
    fn version_one_type_bytes_are_stable() {
        let fixtures: [(Type, &[u8]); 7] = [
            (Type::Unit, &[0x01, 0x00]),
            (Type::Bool, &[0x01, 0x01]),
            (Type::Int, &[0x01, 0x02]),
            (Type::Text, &[0x01, 0x03]),
            (Type::Bytes, &[0x01, 0x04]),
            (Type::Blob, &[0x01, 0x05]),
            (
                Type::Nominal { name: "N".into() },
                &[
                    0x01, 0x06, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'N',
                ],
            ),
        ];

        for (value_type, expected) in fixtures {
            assert_eq!(value_type.encode_canonical(), expected);
        }
    }

    #[test]
    fn version_one_value_bytes_are_stable() {
        let fixtures: [(Value, &[u8]); 7] = [
            (Value::Unit, &[0x01, 0x00]),
            (Value::Bool(false), &[0x01, 0x01, 0x00]),
            (Value::Bool(true), &[0x01, 0x01, 0x01]),
            (
                Value::Int(-2),
                &[0x01, 0x02, 0xfe, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
            ),
            (
                Value::Text("hi".into()),
                &[
                    0x01, 0x03, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'h', b'i',
                ],
            ),
            (
                Value::Bytes(vec![0x00, 0xff].into_boxed_slice()),
                &[
                    0x01, 0x04, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff,
                ],
            ),
            (
                Value::Blob(fixture_content_id()),
                &[
                    0x01, 0x05, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a,
                    0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
                    0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
                ],
            ),
        ];

        for (value, expected) in fixtures {
            assert_eq!(value.encode_canonical(), expected);
        }
    }

    #[test]
    fn unsupported_versions_are_rejected() {
        let error = CanonicalDecodeError::UnsupportedVersion { version: 2 };
        assert_eq!(Type::decode_canonical(&[0x02, TAG_UNIT]), Err(error));
        assert_eq!(Value::decode_canonical(&[0x02, TAG_UNIT]), Err(error));
    }

    #[test]
    fn unknown_tags_are_rejected() {
        assert_eq!(
            Type::decode_canonical(&[ENCODING_VERSION, 0xff]),
            Err(CanonicalDecodeError::UnknownTypeTag { tag: 0xff })
        );
        assert_eq!(
            Value::decode_canonical(&[ENCODING_VERSION, TAG_NOMINAL]),
            Err(CanonicalDecodeError::UnknownValueTag { tag: TAG_NOMINAL })
        );
    }

    #[test]
    fn truncated_encodings_are_rejected() {
        assert_eq!(
            Type::decode_canonical(&[]),
            Err(CanonicalDecodeError::Truncated)
        );
        assert_eq!(
            Value::decode_canonical(&[ENCODING_VERSION]),
            Err(CanonicalDecodeError::Truncated)
        );

        let mut encoded_type = Type::Nominal { name: "N".into() }.encode_canonical();
        let _ = encoded_type.pop();
        assert_eq!(
            Type::decode_canonical(&encoded_type),
            Err(CanonicalDecodeError::Truncated)
        );

        for value in [
            Value::Bool(true),
            Value::Int(1),
            Value::Text("x".into()),
            Value::Bytes(vec![1].into_boxed_slice()),
            Value::Blob(fixture_content_id()),
        ] {
            let mut encoded = value.encode_canonical();
            let _ = encoded.pop();
            assert_eq!(
                Value::decode_canonical(&encoded),
                Err(CanonicalDecodeError::Truncated)
            );
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        assert_eq!(
            Type::decode_canonical(&[ENCODING_VERSION, TAG_UNIT, 0x00]),
            Err(CanonicalDecodeError::TrailingBytes)
        );
        assert_eq!(
            Value::decode_canonical(&[ENCODING_VERSION, TAG_UNIT, 0x00]),
            Err(CanonicalDecodeError::TrailingBytes)
        );
    }

    #[test]
    fn invalid_utf8_is_rejected() {
        let encoded_type = [
            ENCODING_VERSION,
            TAG_NOMINAL,
            0x01,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0xff,
        ];
        let encoded_value = [
            ENCODING_VERSION,
            TAG_TEXT,
            0x01,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0xff,
        ];
        assert_eq!(
            Type::decode_canonical(&encoded_type),
            Err(CanonicalDecodeError::InvalidUtf8)
        );
        assert_eq!(
            Value::decode_canonical(&encoded_value),
            Err(CanonicalDecodeError::InvalidUtf8)
        );
    }

    #[test]
    fn non_canonical_boolean_bytes_are_rejected() {
        assert_eq!(
            Value::decode_canonical(&[ENCODING_VERSION, TAG_BOOL, 0x02]),
            Err(CanonicalDecodeError::InvalidBoolean { byte: 0x02 })
        );
    }
}
