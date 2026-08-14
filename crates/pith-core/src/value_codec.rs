//! Canonical payload encoding for the currently implemented [`Type`] and
//! [`Value`] variants.

use crate::{
    Interface, RecordField, Type, Value,
    codec::CanonicalReader,
    manifest::{encode_bytes, encode_length, encode_str},
};

const ENCODING_VERSION: u8 = 1;

/// Discriminant tags for `Type`/`Value` variants. `Type` and `Value`
/// deliberately share numbering for their overlapping variants so a value and
/// its type encode under the same tag; `Nominal` is shared: a nominal type
/// encodes its name, a nominal value encodes its name and representation. These
/// tags are already part of the stable pure-computation digest format.
pub(crate) const TAG_UNIT: u8 = 0;
pub(crate) const TAG_BOOL: u8 = 1;
pub(crate) const TAG_INT: u8 = 2;
pub(crate) const TAG_TEXT: u8 = 3;
pub(crate) const TAG_BYTES: u8 = 4;
pub(crate) const TAG_BLOB: u8 = 5;
pub(crate) const TAG_NOMINAL: u8 = 6;
pub(crate) const TAG_LIST: u8 = 7;
pub(crate) const TAG_RECORD: u8 = 8;

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
        Type::List(element) => {
            encoded.push(TAG_LIST);
            encode_type_payload(encoded, element);
        }
        Type::Record(fields) => {
            encoded.push(TAG_RECORD);
            encode_record_fields(encoded, fields, encode_type_payload);
        }
    }
}

/// Records are the one shape `Type` and `Value` build the same way — named
/// fields, one payload each — so both grammars share the field machinery.
/// Fields are written in name order whatever order the in-memory slice holds,
/// because name order is the canonical order 0026 fixes; the decoder accepts
/// that order only, so two records that differ by construction order cannot
/// encode differently.
fn encode_record_fields<T>(
    encoded: &mut Vec<u8>,
    fields: &[RecordField<T>],
    encode_payload: fn(&mut Vec<u8>, &T),
) {
    let mut ordered: Vec<&RecordField<T>> = fields.iter().collect();
    ordered.sort_by(|left, right| left.name.cmp(&right.name));
    encode_length(encoded, ordered.len());
    for field in ordered {
        encode_str(encoded, &field.name);
        encode_payload(encoded, &field.payload);
    }
}

fn decode_record_fields<T>(
    decoder: &mut CanonicalReader,
    depth: u32,
    decode_payload: fn(&mut CanonicalReader, u32) -> Result<T, CanonicalDecodeError>,
) -> Result<Box<[RecordField<T>]>, CanonicalDecodeError> {
    if depth >= MAX_NOMINAL_NESTING {
        return Err(CanonicalDecodeError::NestingTooDeep {
            limit: MAX_NOMINAL_NESTING,
        });
    }
    let count = decoder.read_length()?;
    let mut fields: Vec<RecordField<T>> = Vec::with_capacity(count.min(64));
    for _ in 0..count {
        let name: Box<str> = decoder.read_text()?.into();
        if let Some(previous) = fields.last()
            && previous.name.as_ref() >= name.as_ref()
        {
            return Err(CanonicalDecodeError::RecordFieldsOutOfOrder {
                earlier: previous.name.clone(),
                later: name,
            });
        }
        fields.push(RecordField {
            name,
            payload: decode_payload(decoder, depth.saturating_add(1))?,
        });
    }
    Ok(fields.into_boxed_slice())
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
        Value::Nominal {
            name,
            representation,
        } => {
            encoded.push(TAG_NOMINAL);
            encode_str(encoded, name);
            encode_value_payload(encoded, representation);
        }
        Value::List(elements) => {
            encoded.push(TAG_LIST);
            encode_length(encoded, elements.len());
            for element in elements.iter() {
                encode_value_payload(encoded, element);
            }
        }
        Value::Record(fields) => {
            encoded.push(TAG_RECORD);
            encode_record_fields(encoded, fields, encode_value_payload);
        }
    }
}

/// Failure to decode a canonical [`Type`] or [`Value`] encoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanonicalDecodeError {
    UnsupportedVersion { version: u8 },
    UnknownTypeTag { tag: u8 },
    UnknownValueTag { tag: u8 },
    InvalidBoolean { byte: u8 },
    Truncated,
    TrailingBytes,
    InvalidUtf8,
    LengthOutOfRange { length: u64 },
    NestingTooDeep { limit: u32 },
    RecordFieldsOutOfOrder { earlier: Box<str>, later: Box<str> },
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
            Self::NestingTooDeep { limit } => write!(
                formatter,
                "canonical encoding nests values or types deeper than {limit}"
            ),
            Self::RecordFieldsOutOfOrder { earlier, later } => write!(
                formatter,
                "canonical record fields are not in strictly ascending name order: `{earlier}` then `{later}`"
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
        let mut decoder = CanonicalReader::new(encoded);
        decoder.read_version(ENCODING_VERSION)?;
        let value_type = decode_type_payload(&mut decoder)?;
        decoder.finish()?;
        Ok(value_type)
    }
}

pub(crate) fn decode_type_payload(
    decoder: &mut CanonicalReader,
) -> Result<Type, CanonicalDecodeError> {
    decode_type_at_depth(decoder, 0)
}

/// `List` made `Type` recursive, so decoding one type payload now nests the
/// same way a value payload does and is bounded by the same limit.
fn decode_type_at_depth(
    decoder: &mut CanonicalReader,
    depth: u32,
) -> Result<Type, CanonicalDecodeError> {
    Ok(match decoder.read_byte()? {
        TAG_UNIT => Type::Unit,
        TAG_BOOL => Type::Bool,
        TAG_INT => Type::Int,
        TAG_TEXT => Type::Text,
        TAG_BYTES => Type::Bytes,
        TAG_BLOB => Type::Blob,
        TAG_NOMINAL => Type::Nominal {
            name: decoder.read_text()?.into(),
        },
        TAG_LIST => {
            if depth >= MAX_NOMINAL_NESTING {
                return Err(CanonicalDecodeError::NestingTooDeep {
                    limit: MAX_NOMINAL_NESTING,
                });
            }
            Type::List(Box::new(decode_type_at_depth(
                decoder,
                depth.saturating_add(1),
            )?))
        }
        TAG_RECORD => Type::Record(decode_record_fields(decoder, depth, decode_type_at_depth)?),
        tag => return Err(CanonicalDecodeError::UnknownTypeTag { tag }),
    })
}

impl Interface {
    /// Encode this interface in the current version of Pith's canonical wire
    /// format.
    ///
    /// A durable action computation retains the interface its request was made
    /// against (decision 0033), because revalidating a consumer's action edge
    /// re-selects a rule from it.
    #[must_use]
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut encoded = vec![ENCODING_VERSION];
        encode_length(&mut encoded, self.inputs.len());
        for input in &self.inputs {
            encode_type_payload(&mut encoded, input);
        }
        encode_type_payload(&mut encoded, &self.output);
        encoded
    }

    /// Decode one interface from Pith's versioned canonical wire format.
    ///
    /// # Errors
    /// Returns an error for unsupported versions, unknown type tags, truncated
    /// or trailing data, invalid UTF-8, and lengths not representable on the
    /// current platform.
    pub fn decode_canonical(encoded: &[u8]) -> Result<Self, CanonicalDecodeError> {
        let mut decoder = CanonicalReader::new(encoded);
        decoder.read_version(ENCODING_VERSION)?;
        let inputs = decoder.read_sequence(decode_type_payload)?;
        let output = decode_type_payload(&mut decoder)?;
        decoder.finish()?;
        Ok(Self { inputs, output })
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
        let mut decoder = CanonicalReader::new(encoded);
        decoder.read_version(ENCODING_VERSION)?;
        let value = decode_value_payload(&mut decoder, 0)?;
        decoder.finish()?;
        Ok(value)
    }
}

/// How deeply the recursive constructors may nest before decoding refuses.
///
/// `Nominal` values, `List` values, `List` types, `Record` values, and
/// `Record` types recurse during decoding, so they are the things that can
/// make it recurse without bound. The decoder
/// reads bytes an adapter hands it, and those bytes are whatever the database
/// holds: a row of repeated `TAG_NOMINAL`, `TAG_LIST`, or `TAG_RECORD`
/// recurses once per byte and overflows the stack, which aborts the process
/// rather than returning the adapter error decision 0024 requires of corrupt
/// state. Bounding the depth turns that into an ordinary decode failure.
///
/// The limit is far above any real value. A nominal over a nominal is legal and
/// occasionally useful; a chain dozens deep is not something the calculus in
/// decision 0026 describes writing.
pub const MAX_NOMINAL_NESTING: u32 = 32;

fn decode_value_payload(
    decoder: &mut CanonicalReader,
    depth: u32,
) -> Result<Value, CanonicalDecodeError> {
    match decoder.read_byte()? {
        TAG_UNIT => Ok(Value::Unit),
        TAG_BOOL => Ok(Value::Bool(decoder.read_bool()?)),
        TAG_INT => Ok(Value::Int(decoder.read_int()?)),
        TAG_TEXT => Ok(Value::Text(decoder.read_text()?.into())),
        TAG_BYTES => Ok(Value::Bytes(decoder.read_bytes()?.into())),
        TAG_BLOB => Ok(Value::Blob(decoder.read_content_id()?)),
        TAG_NOMINAL => {
            if depth >= MAX_NOMINAL_NESTING {
                return Err(CanonicalDecodeError::NestingTooDeep {
                    limit: MAX_NOMINAL_NESTING,
                });
            }
            let name = decoder.read_text()?.into();
            let representation = Box::new(decode_value_payload(decoder, depth.saturating_add(1))?);
            Ok(Value::Nominal {
                name,
                representation,
            })
        }
        TAG_LIST => {
            if depth >= MAX_NOMINAL_NESTING {
                return Err(CanonicalDecodeError::NestingTooDeep {
                    limit: MAX_NOMINAL_NESTING,
                });
            }
            let elements = decoder
                .read_sequence(|decoder| decode_value_payload(decoder, depth.saturating_add(1)))?;
            Ok(Value::List(elements))
        }
        TAG_RECORD => Ok(Value::Record(decode_record_fields(
            decoder,
            depth,
            decode_value_payload,
        )?)),
        tag => Err(CanonicalDecodeError::UnknownValueTag { tag }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RecordField;
    use pith_ids::{ContentDigest, ContentId};
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
            Type::List(Box::new(Type::Nominal {
                name: "Machine".into(),
            })),
            Type::List(Box::new(Type::List(Box::new(Type::Int)))),
            Type::record([
                RecordField {
                    name: "name".into(),
                    payload: Type::Text,
                },
                RecordField {
                    name: "nested".into(),
                    payload: Type::List(Box::new(Type::Int)),
                },
            ])
            .unwrap(),
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
            Value::Nominal {
                name: "Machine".into(),
                representation: Box::new(Value::Text("id-7".into())),
            },
            Value::Nominal {
                name: "Nested".into(),
                representation: Box::new(Value::Nominal {
                    name: "Inner".into(),
                    representation: Box::new(Value::Int(1)),
                }),
            },
            Value::List(vec![].into_boxed_slice()),
            Value::List(vec![Value::Text("a".into()), Value::Text("b".into())].into_boxed_slice()),
            Value::List(
                vec![Value::List(vec![Value::Int(1)].into_boxed_slice())].into_boxed_slice(),
            ),
            Value::record([
                RecordField {
                    name: "name".into(),
                    payload: Value::Text("pith".into()),
                },
                RecordField {
                    name: "nested".into(),
                    payload: Value::List(vec![Value::Int(1)].into_boxed_slice()),
                },
            ])
            .unwrap(),
        ] {
            let encoded = value.encode_canonical();
            assert_eq!(Value::decode_canonical(&encoded), Ok(value.clone()));
        }
    }

    #[test]
    fn version_one_type_bytes_are_stable() {
        let fixtures: [(Type, &[u8]); 9] = [
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
            (Type::List(Box::new(Type::Int)), &[0x01, 0x07, 0x02]),
            // A two-field record, fields in canonical name order: the u64
            // count, then per field the u64-length-prefixed name and the
            // field's type payload.
            (
                Type::record([
                    RecordField {
                        name: "a".into(),
                        payload: Type::Int,
                    },
                    RecordField {
                        name: "b".into(),
                        payload: Type::Text,
                    },
                ])
                .unwrap(),
                &[
                    0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
                    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'a', 0x02, //
                    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'b', 0x03,
                ],
            ),
        ];

        for (value_type, expected) in fixtures {
            assert_eq!(value_type.encode_canonical(), expected);
        }
    }

    #[test]
    fn version_one_value_bytes_are_stable() {
        let fixtures: [(Value, &[u8]); 11] = [
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
            (
                Value::Nominal {
                    name: "N".into(),
                    representation: Box::new(Value::Unit),
                },
                &[
                    0x01, 0x06, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'N', 0x00,
                ],
            ),
            // An empty list: the tag, the u64 element count, nothing else.
            (
                Value::List(vec![].into_boxed_slice()),
                &[0x01, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            ),
            // A list of two texts: the u64 count, then each element's own
            // payload.
            (
                Value::List(
                    vec![Value::Text("a".into()), Value::Text("b".into())].into_boxed_slice(),
                ),
                &[
                    0x01, 0x07, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
                    0x03, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'a', //
                    0x03, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'b',
                ],
            ),
            (
                Value::record([
                    RecordField {
                        name: "a".into(),
                        payload: Value::Int(7),
                    },
                    RecordField {
                        name: "b".into(),
                        payload: Value::Text("hi".into()),
                    },
                ])
                .unwrap(),
                &[
                    0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
                    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'a', //
                    0x02, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
                    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'b', //
                    0x03, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'h', b'i',
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
        assert_eq!(
            Type::decode_canonical(&[0x02, TAG_UNIT]),
            Err(error.clone())
        );
        assert_eq!(Value::decode_canonical(&[0x02, TAG_UNIT]), Err(error));
    }

    #[test]
    fn unknown_tags_are_rejected() {
        assert_eq!(
            Type::decode_canonical(&[ENCODING_VERSION, 0xff]),
            Err(CanonicalDecodeError::UnknownTypeTag { tag: 0xff })
        );
        assert_eq!(
            Value::decode_canonical(&[ENCODING_VERSION, 0xff]),
            Err(CanonicalDecodeError::UnknownValueTag { tag: 0xff })
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
            Value::Nominal {
                name: "N".into(),
                representation: Box::new(Value::Unit),
            },
            Value::List(vec![Value::Int(1)].into_boxed_slice()),
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
    fn deeply_nested_lists_and_list_types_fail_rather_than_overflowing() {
        // `List` recurses in both the value and the type grammar, so a hostile
        // row of list tags has the same unbounded-recursion shape a nominal
        // chain has. The bound turns it into a decode error.
        let mut value = Value::Unit;
        for _ in 0..(MAX_NOMINAL_NESTING.saturating_add(1)) {
            value = Value::List(vec![value].into_boxed_slice());
        }
        assert_eq!(
            Value::decode_canonical(&value.encode_canonical()),
            Err(CanonicalDecodeError::NestingTooDeep {
                limit: MAX_NOMINAL_NESTING
            })
        );

        let mut element = Type::Unit;
        for _ in 0..(MAX_NOMINAL_NESTING.saturating_add(1)) {
            element = Type::List(Box::new(element));
        }
        assert_eq!(
            Type::decode_canonical(&element.encode_canonical()),
            Err(CanonicalDecodeError::NestingTooDeep {
                limit: MAX_NOMINAL_NESTING
            })
        );
    }

    #[test]
    fn record_encoding_is_name_ordered_whatever_order_the_value_holds() {
        // 0026 fixes name order as the canonical order, so the encoder writes
        // it even for a slice built out of order, and the decoder refuses
        // anything else — which also refuses a duplicate name — so two records
        // that differ only by construction order cannot encode differently.
        let sorted = Value::record([
            RecordField {
                name: "a".into(),
                payload: Value::Int(1),
            },
            RecordField {
                name: "b".into(),
                payload: Value::Int(2),
            },
        ])
        .unwrap();
        let reversed = Value::Record(
            vec![
                RecordField {
                    name: "b".into(),
                    payload: Value::Int(2),
                },
                RecordField {
                    name: "a".into(),
                    payload: Value::Int(1),
                },
            ]
            .into_boxed_slice(),
        );
        assert_eq!(sorted.encode_canonical(), reversed.encode_canonical());
        assert_eq!(
            Value::decode_canonical(&sorted.encode_canonical()),
            Ok(sorted)
        );
    }

    #[test]
    fn out_of_order_and_duplicate_record_fields_are_rejected() {
        // Hand-built bytes, one pair of fields in descending name order, for
        // both grammars. Descending order is also what a duplicate name
        // produces, so one encoding covers both rejections.
        let fields_descending = |payload: u8| -> Vec<u8> {
            let mut encoded = Vec::new();
            encode_length(&mut encoded, 2);
            for name in ["b", "a"] {
                encode_str(&mut encoded, name);
                encoded.push(payload);
            }
            encoded
        };
        let mut type_bytes = vec![ENCODING_VERSION, TAG_RECORD];
        type_bytes.extend(fields_descending(TAG_INT));
        assert_eq!(
            Type::decode_canonical(&type_bytes),
            Err(CanonicalDecodeError::RecordFieldsOutOfOrder {
                earlier: "b".into(),
                later: "a".into(),
            })
        );
        let mut value_bytes = vec![ENCODING_VERSION, TAG_RECORD];
        value_bytes.extend(fields_descending(TAG_UNIT));
        assert_eq!(
            Value::decode_canonical(&value_bytes),
            Err(CanonicalDecodeError::RecordFieldsOutOfOrder {
                earlier: "b".into(),
                later: "a".into(),
            })
        );
    }

    #[test]
    fn deeply_nested_records_fail_rather_than_overflowing() {
        // A record's field payload recurses in both grammars, so a run of
        // record tags has the same unbounded-recursion shape a list chain has.
        let mut value = Value::Int(0);
        for _ in 0..(MAX_NOMINAL_NESTING.saturating_add(1)) {
            value = Value::record([RecordField {
                name: "f".into(),
                payload: value,
            }])
            .unwrap();
        }
        assert_eq!(
            Value::decode_canonical(&value.encode_canonical()),
            Err(CanonicalDecodeError::NestingTooDeep {
                limit: MAX_NOMINAL_NESTING
            })
        );

        let mut field_type = Type::Int;
        for _ in 0..(MAX_NOMINAL_NESTING.saturating_add(1)) {
            field_type = Type::record([RecordField {
                name: "f".into(),
                payload: field_type,
            }])
            .unwrap();
        }
        assert_eq!(
            Type::decode_canonical(&field_type.encode_canonical()),
            Err(CanonicalDecodeError::NestingTooDeep {
                limit: MAX_NOMINAL_NESTING
            })
        );
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
