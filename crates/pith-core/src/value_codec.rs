//! Canonical payload encoding for the currently implemented [`Type`] and
//! [`Value`] variants.

use crate::{
    Int, Interface, NominalType, RecordField, SumConstructor, SumType, Type, Value,
    codec::CanonicalReader,
    declaration::Coordinate,
    manifest::{encode_bytes, encode_length, encode_str},
};

/// Version of the stored `Type`/`Value` encoding.
///
/// Pinned at 1 until the first release (decision 0048), on the same terms as the
/// contract encoding and the record encoding: no released reader exists, so a
/// grammar change is answered by discarding and rebuilding rather than by a
/// migration. Four constructors have landed under that rule — `Nominal`, `List`,
/// `Record`, `Sum` — each taking a reserved tag, so no existing byte sequence
/// changed meaning and a foreign stream is still refused by the version byte.
pub const ENCODING_VERSION: u8 = 1;

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
pub(crate) const TAG_SUM: u8 = 9;
/// The recursion cut (decision 0047). A type-only tag: a value never carries a
/// cut, because a cut is a position inside a declaration's body and a value
/// carries its declaration's name instead.
pub(crate) const TAG_CUT: u8 = 10;

pub(crate) fn encode_type_payload(encoded: &mut Vec<u8>, value_type: &Type) {
    match value_type {
        Type::Unit => encoded.push(TAG_UNIT),
        Type::Bool => encoded.push(TAG_BOOL),
        Type::Int => encoded.push(TAG_INT),
        Type::Text => encoded.push(TAG_TEXT),
        Type::Bytes => encoded.push(TAG_BYTES),
        Type::Blob => encoded.push(TAG_BLOB),
        Type::Nominal(declared) => {
            encoded.push(TAG_NOMINAL);
            encode_str(encoded, &declared.coordinate.module);
            encode_str(encoded, &declared.coordinate.name);
            encode_type_payload(encoded, &declared.representation);
        }
        Type::Cut => encoded.push(TAG_CUT),
        Type::List(element) => {
            encoded.push(TAG_LIST);
            encode_type_payload(encoded, element);
        }
        Type::Record(fields) => {
            encoded.push(TAG_RECORD);
            encode_record_fields(encoded, fields, encode_type_payload);
        }
        Type::Sum(declared) => {
            let constructors = &declared.constructors;
            encoded.push(TAG_SUM);
            encode_str(encoded, &declared.coordinate.module);
            encode_str(encoded, &declared.coordinate.name);
            // Constructor order is canonical name order on the same terms a
            // record's fields are.
            let mut ordered: Vec<&SumConstructor> = constructors.iter().collect();
            ordered.sort_by(|left, right| left.name.cmp(&right.name));
            encode_length(encoded, ordered.len());
            for constructor in ordered {
                encode_str(encoded, &constructor.name);
                encode_optional(encoded, constructor.payload.as_ref(), encode_type_payload);
            }
        }
    }
}

/// A sign byte, then the magnitude big-endian with no leading zero, behind the
/// length prefix every variable-width payload in this encoding carries
/// (decision 0055).
///
/// Big-endian magnitudes are CBOR's choice for its bignums and Java's for
/// `BigInteger.toByteArray`, and the byte order is the only part of this a
/// reader has to agree on; the length prefix is what makes the payload
/// self-delimiting the way `Text` and `Bytes` already are.
fn encode_integer(encoded: &mut Vec<u8>, value: &Int) {
    encoded.push(u8::from(value.is_negative()));
    encode_bytes(encoded, &value.magnitude_bytes());
}

/// Refuses every spelling of a value except the one [`encode_integer`] writes:
/// a leading zero byte and a negative zero both name a value that already has an
/// encoding, and admitting them would put two byte strings under one integer and
/// two computation keys under one value.
fn decode_integer(decoder: &mut CanonicalReader) -> Result<Int, CanonicalDecodeError> {
    let negative = decoder.read_bool()?;
    let magnitude = decoder.read_bytes()?;
    Int::from_sign_and_magnitude(negative, magnitude)
        .ok_or(CanonicalDecodeError::NonCanonicalInteger)
}

/// A presence byte then the payload when present, for the optionally-typed
/// payloads a sum carries in both grammars.
fn encode_optional<T>(
    encoded: &mut Vec<u8>,
    payload: Option<&T>,
    encode_payload: fn(&mut Vec<u8>, &T),
) {
    match payload {
        Some(payload) => {
            encoded.push(1);
            encode_payload(encoded, payload);
        }
        None => encoded.push(0),
    }
}

fn decode_optional<T>(
    decoder: &mut CanonicalReader,
    depth: u32,
    decode_payload: fn(&mut CanonicalReader, u32) -> Result<T, CanonicalDecodeError>,
) -> Result<Option<T>, CanonicalDecodeError> {
    if decoder.read_bool()? {
        decode_payload(decoder, depth).map(Some)
    } else {
        Ok(None)
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
            return Err(CanonicalDecodeError::NamesOutOfOrder {
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
            encode_integer(encoded, value);
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
        Value::Sum {
            type_name,
            constructor,
            payload,
        } => {
            encoded.push(TAG_SUM);
            encode_str(encoded, type_name);
            encode_str(encoded, constructor);
            encode_optional(encoded, payload.as_deref(), encode_value_payload);
        }
    }
}

/// Failure to decode a canonical [`Type`] or [`Value`] encoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanonicalDecodeError {
    UnsupportedVersion {
        version: u8,
    },
    UnknownTypeTag {
        tag: u8,
    },
    UnknownValueTag {
        tag: u8,
    },
    UnknownBodyTag {
        tag: u8,
    },
    /// A type payload arrived in a body position reserved for one kind, such
    /// as a non-sum where `MakeSum` writes its declaration.
    TypeInBodyPosition,
    InvalidBoolean {
        byte: u8,
    },
    Truncated,
    TrailingBytes,
    InvalidUtf8,
    LengthOutOfRange {
        length: u64,
    },
    NestingTooDeep {
        limit: u32,
    },
    NamesOutOfOrder {
        earlier: Box<str>,
        later: Box<str>,
    },
    NonCanonicalInteger,
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
            Self::UnknownBodyTag { tag } => {
                write!(formatter, "unknown canonical body tag {tag}")
            }
            Self::TypeInBodyPosition => formatter.write_str(
                "a canonical body holds a type where a specific kind is \
                 required, such as a sum declaration",
            ),
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
            Self::NamesOutOfOrder { earlier, later } => write!(
                formatter,
                "canonical names are not in strictly ascending order: `{earlier}` then `{later}`"
            ),
            Self::NonCanonicalInteger => formatter.write_str(
                "canonical integer magnitude has a leading zero byte, or is a negative zero",
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
        TAG_NOMINAL => {
            if depth >= MAX_NOMINAL_NESTING {
                return Err(CanonicalDecodeError::NestingTooDeep {
                    limit: MAX_NOMINAL_NESTING,
                });
            }
            let module: Box<str> = decoder.read_text()?.into();
            let name: Box<str> = decoder.read_text()?.into();
            Type::Nominal(Box::new(NominalType {
                coordinate: Coordinate::new(module, name),
                representation: decode_type_at_depth(decoder, depth.saturating_add(1))?,
            }))
        }
        TAG_CUT => Type::Cut,
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
        TAG_SUM => {
            if depth >= MAX_NOMINAL_NESTING {
                return Err(CanonicalDecodeError::NestingTooDeep {
                    limit: MAX_NOMINAL_NESTING,
                });
            }
            let module: Box<str> = decoder.read_text()?.into();
            let name: Box<str> = decoder.read_text()?.into();
            let count = decoder.read_length()?;
            let mut constructors: Vec<SumConstructor> = Vec::with_capacity(count.min(64));
            for _ in 0..count {
                let constructor: Box<str> = decoder.read_text()?.into();
                if let Some(previous) = constructors.last()
                    && previous.name.as_ref() >= constructor.as_ref()
                {
                    return Err(CanonicalDecodeError::NamesOutOfOrder {
                        earlier: previous.name.clone(),
                        later: constructor,
                    });
                }
                constructors.push(SumConstructor {
                    name: constructor,
                    payload: decode_optional(
                        decoder,
                        depth.saturating_add(1),
                        decode_type_at_depth,
                    )?,
                });
            }
            Type::Sum(Box::new(SumType {
                coordinate: Coordinate::new(module, name),
                constructors: constructors.into(),
            }))
        }
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
/// `Nominal` values, `List` values, `List` types, `Record` values, `Record`
/// types, `Sum` values, and `Sum` types recurse during decoding, so they are
/// the things that can make it recurse without bound. The decoder
/// reads bytes an adapter hands it, and those bytes are whatever the database
/// holds: a row of repeated `TAG_NOMINAL`, `TAG_LIST`, `TAG_RECORD`, or
/// `TAG_SUM` recurses once per byte and overflows the stack, which aborts the
/// process rather than returning the adapter error decision 0024 requires of
/// corrupt state. Bounding the depth turns that into an ordinary decode
/// failure.
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
        TAG_INT => Ok(Value::Int(decode_integer(decoder)?)),
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
        TAG_SUM => {
            if depth >= MAX_NOMINAL_NESTING {
                return Err(CanonicalDecodeError::NestingTooDeep {
                    limit: MAX_NOMINAL_NESTING,
                });
            }
            let type_name = decoder.read_text()?.into();
            let constructor = decoder.read_text()?.into();
            let payload = decode_optional(decoder, depth.saturating_add(1), decode_value_payload)?;
            Ok(Value::Sum {
                type_name,
                constructor,
                payload: payload.map(Box::new),
            })
        }
        tag => Err(CanonicalDecodeError::UnknownValueTag { tag }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RecordField;
    use crate::value::{declared_nominal, declared_sum};
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
            declared_nominal("test", "Machine", Type::Blob),
            Type::List(Box::new(declared_nominal("test", "Machine", Type::Blob))),
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
            declared_sum(
                "test",
                "Source",
                [
                    SumConstructor {
                        name: "Git".into(),
                        payload: Some(Type::Text),
                    },
                    SumConstructor {
                        name: "Path".into(),
                        payload: None,
                    },
                ],
            ),
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
            Value::int(i64::MIN),
            Value::int(i64::MAX),
            // Past the range the type used to stop at, from both ends.
            Value::Int(Int::from(i64::MIN).multiplied(&Int::from(i64::MIN))),
            Value::Int(Int::from(i64::MIN).multiplied(&Int::from(i64::MAX))),
            Value::Text("Pith \u{03bb}".into()),
            Value::Bytes(vec![0x00, 0x80, 0xff].into_boxed_slice()),
            Value::Blob(fixture_content_id()),
            Value::Nominal {
                name: "test.Machine".into(),
                representation: Box::new(Value::Text("id-7".into())),
            },
            Value::Nominal {
                name: "test.Nested".into(),
                representation: Box::new(Value::Nominal {
                    name: "test.Inner".into(),
                    representation: Box::new(Value::int(1)),
                }),
            },
            Value::List(vec![].into_boxed_slice()),
            Value::List(vec![Value::Text("a".into()), Value::Text("b".into())].into_boxed_slice()),
            Value::List(
                vec![Value::List(vec![Value::int(1)].into_boxed_slice())].into_boxed_slice(),
            ),
            Value::record([
                RecordField {
                    name: "name".into(),
                    payload: Value::Text("pith".into()),
                },
                RecordField {
                    name: "nested".into(),
                    payload: Value::List(vec![Value::int(1)].into_boxed_slice()),
                },
            ])
            .unwrap(),
            Value::Sum {
                type_name: "test.Source".into(),
                constructor: "Git".into(),
                payload: Some(Box::new(Value::Text("main".into()))),
            },
            Value::Sum {
                type_name: "test.Source".into(),
                constructor: "Path".into(),
                payload: None,
            },
        ] {
            let encoded = value.encode_canonical();
            assert_eq!(Value::decode_canonical(&encoded), Ok(value.clone()));
        }
    }

    #[test]
    fn version_one_type_bytes_are_stable() {
        let fixtures: [(Type, &[u8]); 10] = [
            (Type::Unit, &[0x01, 0x00]),
            (Type::Bool, &[0x01, 0x01]),
            (Type::Int, &[0x01, 0x02]),
            (Type::Text, &[0x01, 0x03]),
            (Type::Bytes, &[0x01, 0x04]),
            (Type::Blob, &[0x01, 0x05]),
            (
                // A nominal type now carries its declaration: the module, the
                // declared name, then the representation's own payload
                // (decision 0047). The trailing 0x05 is TAG_BLOB.
                declared_nominal("test", "N", Type::Blob),
                &[
                    0x01, 0x06, //
                    0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b't', b'e', b's', b't', //
                    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'N', //
                    0x05,
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
            // A two-constructor sum, constructors in canonical name order:
            // the sum name, the u64 count, then per constructor the
            // length-prefixed name, a presence byte, and the payload type
            // when present.
            (
                declared_sum(
                    "test",
                    "S",
                    [
                        SumConstructor {
                            name: "b".into(),
                            payload: None,
                        },
                        SumConstructor {
                            name: "a".into(),
                            payload: Some(Type::Int),
                        },
                    ],
                ),
                &[
                    0x01, 0x09, //
                    0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b't', b'e', b's', b't', //
                    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'S', //
                    0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
                    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'a', 0x01, 0x02, //
                    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'b', 0x00,
                ],
            ),
        ];

        for (value_type, expected) in fixtures {
            assert_eq!(value_type.encode_canonical(), expected);
        }
    }

    #[test]
    fn version_one_value_bytes_are_stable() {
        let fixtures: [(Value, &[u8]); 15] = [
            (Value::Unit, &[0x01, 0x00]),
            (Value::Bool(false), &[0x01, 0x01, 0x00]),
            (Value::Bool(true), &[0x01, 0x01, 0x01]),
            // An integer carries a sign byte then its magnitude, big-endian
            // behind the same u64 length prefix `Text` and `Bytes` use, with no
            // leading zero byte (decision 0055). Zero is the empty magnitude,
            // which is the one place the sign byte and the length agree that
            // there is nothing to read.
            (
                Value::int(0),
                &[
                    0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                ],
            ),
            (
                Value::int(-2),
                &[
                    0x01, 0x02, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
                ],
            ),
            (
                Value::int(258),
                &[
                    0x01, 0x02, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02,
                ],
            ),
            (
                // Beyond the range the type used to hold: 2^64, whose magnitude
                // is nine bytes and which no `i64` fixture could have written.
                Value::Int(Int::from(4_294_967_296u64).multiplied(&Int::from(4_294_967_296u64))),
                &[
                    0x01, 0x02, 0x00, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
                    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                ],
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
                    name: "test.N".into(),
                    representation: Box::new(Value::Unit),
                },
                // A nominal value carries a name, not a declaration: the value
                // is data and the type is the side that can check
                // (decision 0047). The name is the coordinate's spelling.
                &[
                    0x01, 0x06, //
                    0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
                    b't', b'e', b's', b't', b'.', b'N', //
                    0x00,
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
                        payload: Value::int(7),
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
                    0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, //
                    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'b', //
                    0x03, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'h', b'i',
                ],
            ),
            // A sum value: the sum name, the constructor name, a presence
            // byte, then the payload when present.
            (
                Value::Sum {
                    type_name: "test.S".into(),
                    constructor: "a".into(),
                    payload: Some(Box::new(Value::Text("hi".into()))),
                },
                // A sum value names its declaration the way a nominal value
                // does, with the coordinate's spelling; the constructor set is
                // the type's business and is not repeated here.
                &[
                    0x01, 0x09, //
                    0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
                    b't', b'e', b's', b't', b'.', b'S', //
                    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'a', //
                    0x01, //
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

        let mut encoded_type = declared_nominal("test", "N", Type::Blob).encode_canonical();
        let _ = encoded_type.pop();
        assert_eq!(
            Type::decode_canonical(&encoded_type),
            Err(CanonicalDecodeError::Truncated)
        );

        for value in [
            Value::Bool(true),
            Value::int(1),
            Value::Text("x".into()),
            Value::Bytes(vec![1].into_boxed_slice()),
            Value::Blob(fixture_content_id()),
            Value::Nominal {
                name: "test.N".into(),
                representation: Box::new(Value::Unit),
            },
            Value::List(vec![Value::int(1)].into_boxed_slice()),
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
                payload: Value::int(1),
            },
            RecordField {
                name: "b".into(),
                payload: Value::int(2),
            },
        ])
        .unwrap();
        let reversed = Value::Record(
            vec![
                RecordField {
                    name: "b".into(),
                    payload: Value::int(2),
                },
                RecordField {
                    name: "a".into(),
                    payload: Value::int(1),
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
            Err(CanonicalDecodeError::NamesOutOfOrder {
                earlier: "b".into(),
                later: "a".into(),
            })
        );
        let mut value_bytes = vec![ENCODING_VERSION, TAG_RECORD];
        value_bytes.extend(fields_descending(TAG_UNIT));
        assert_eq!(
            Value::decode_canonical(&value_bytes),
            Err(CanonicalDecodeError::NamesOutOfOrder {
                earlier: "b".into(),
                later: "a".into(),
            })
        );
    }

    #[test]
    fn deeply_nested_records_fail_rather_than_overflowing() {
        // A record's field payload recurses in both grammars, so a run of
        // record tags has the same unbounded-recursion shape a list chain has.
        let mut value = Value::int(0);
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
    fn deeply_nested_sums_fail_rather_than_overflowing() {
        // A sum's payload recurses in both grammars, so a run of sum tags has
        // the same unbounded-recursion shape a list chain has.
        let mut value = Value::int(0);
        for _ in 0..(MAX_NOMINAL_NESTING.saturating_add(1)) {
            value = Value::Sum {
                type_name: "test.S".into(),
                constructor: "a".into(),
                payload: Some(Box::new(value)),
            };
        }
        assert_eq!(
            Value::decode_canonical(&value.encode_canonical()),
            Err(CanonicalDecodeError::NestingTooDeep {
                limit: MAX_NOMINAL_NESTING
            })
        );

        let mut payload_type = Type::Int;
        for _ in 0..(MAX_NOMINAL_NESTING.saturating_add(1)) {
            payload_type = declared_sum(
                "test",
                "S",
                [SumConstructor {
                    name: "a".into(),
                    payload: Some(payload_type),
                }],
            );
        }
        assert_eq!(
            Type::decode_canonical(&payload_type.encode_canonical()),
            Err(CanonicalDecodeError::NestingTooDeep {
                limit: MAX_NOMINAL_NESTING
            })
        );
    }

    #[test]
    fn out_of_order_sum_constructors_are_rejected() {
        // Descending constructor names, which is also what a duplicate name
        // produces; canonical order is the only one on the wire.
        let mut encoded = vec![ENCODING_VERSION, TAG_SUM];
        encode_str(&mut encoded, "test");
        encode_str(&mut encoded, "S");
        encode_length(&mut encoded, 2);
        for name in ["b", "a"] {
            encode_str(&mut encoded, name);
            encoded.push(0);
        }
        assert_eq!(
            Type::decode_canonical(&encoded),
            Err(CanonicalDecodeError::NamesOutOfOrder {
                earlier: "b".into(),
                later: "a".into(),
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
    fn non_canonical_integer_bytes_are_rejected() {
        // Two byte strings under one integer would be two computation keys
        // under one value, so the decoder refuses every spelling the encoder
        // does not write: a leading zero byte, and a negative zero.
        let integer = |negative: u8, magnitude: &[u8]| {
            let mut encoded = vec![ENCODING_VERSION, TAG_INT, negative];
            encode_bytes(&mut encoded, magnitude);
            encoded
        };
        for encoded in [
            integer(0, &[0x00]),
            integer(0, &[0x00, 0x01]),
            integer(1, &[0x00, 0x01]),
            integer(1, &[]),
        ] {
            assert_eq!(
                Value::decode_canonical(&encoded),
                Err(CanonicalDecodeError::NonCanonicalInteger)
            );
        }
        assert_eq!(Value::decode_canonical(&integer(0, &[])), Ok(Value::int(0)));
        assert_eq!(
            Value::decode_canonical(&integer(2, &[0x01])),
            Err(CanonicalDecodeError::InvalidBoolean { byte: 2 })
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

#[cfg(test)]
mod declaration_codec {
    use super::*;
    use crate::declaration::DeclarationTable;
    use pith_ids::ContentId;

    fn round_trip_type(declared: &Type) {
        assert_eq!(
            Type::decode_canonical(&declared.encode_canonical()).as_ref(),
            Ok(declared),
            "{declared} did not survive the codec"
        );
    }

    #[test]
    fn a_nominal_carrying_a_declaration_round_trips_with_its_representation() {
        // The encoding grew a body, so the decoder has to rebuild one — and a
        // decoded type has to check as well as a constructed one, which is the
        // whole reason decision 0047 puts the declaration in the type.
        let mut table = DeclarationTable::new("m");
        for (name, representation) in [
            ("OverBlob", Type::Blob),
            ("OverText", Type::Text),
            ("OverList", Type::List(Box::new(Type::Int))),
        ] {
            round_trip_type(&table.nominal(name, representation).unwrap());
        }
    }

    #[test]
    fn a_decoded_declaration_still_refuses_a_wrong_representation() {
        let mut table = DeclarationTable::new("m");
        let over_blob = table.nominal("Thing", Type::Blob).unwrap();
        let decoded = Type::decode_canonical(&over_blob.encode_canonical()).unwrap();

        let genuine = Value::Nominal {
            name: "m.Thing".into(),
            representation: Box::new(Value::Blob(ContentId::of_blob(b"x"))),
        };
        let wrong = Value::Nominal {
            name: "m.Thing".into(),
            representation: Box::new(Value::Text("x".into())),
        };
        assert!(genuine.is_type(&decoded));
        assert!(!wrong.is_type(&decoded));
    }

    #[test]
    fn a_recursive_declaration_round_trips_through_the_cut() {
        let mut table = DeclarationTable::new("m");
        let tree = table
            .nominal("Tree", Type::List(Box::new(Type::Cut)))
            .unwrap();
        round_trip_type(&tree);
        // And the decoded form still checks a nested value, so the cut survived
        // as a cut rather than as an expansion.
        let decoded = Type::decode_canonical(&tree.encode_canonical()).unwrap();
        let leaf = Value::Nominal {
            name: "m.Tree".into(),
            representation: Box::new(Value::List([].into())),
        };
        let nested = Value::Nominal {
            name: "m.Tree".into(),
            representation: Box::new(Value::List([leaf].into())),
        };
        assert!(nested.is_type(&decoded));
    }

    #[test]
    fn a_declared_sum_round_trips_with_its_constructor_set() {
        let mut table = DeclarationTable::new("m");
        let source = table
            .sum(
                "Source",
                [
                    SumConstructor {
                        name: "Archive".into(),
                        payload: Some(Type::Blob),
                    },
                    SumConstructor {
                        name: "Local".into(),
                        payload: None,
                    },
                ],
            )
            .unwrap();
        round_trip_type(&source);
    }

    #[test]
    fn a_bare_cut_round_trips() {
        round_trip_type(&Type::Cut);
        round_trip_type(&Type::List(Box::new(Type::Cut)));
    }

    #[test]
    fn a_value_stream_carrying_the_cut_tag_is_refused() {
        // `Cut` is a type-only construct: a value carries its declaration's name,
        // never a position inside a body. The value decoder must not accept the
        // tag rather than silently producing something.
        let encoded = vec![ENCODING_VERSION, TAG_CUT];
        assert_eq!(
            Value::decode_canonical(&encoded),
            Err(CanonicalDecodeError::UnknownValueTag { tag: TAG_CUT })
        );
    }

    #[test]
    fn a_deeply_nested_nominal_declaration_is_refused_rather_than_overflowing() {
        // The nominal decode path recurses now that it carries a representation,
        // so it needs the same depth bound `List` and `Sum` already had.
        let mut representation = Type::Unit;
        for _ in 0..(MAX_NOMINAL_NESTING.saturating_add(2)) {
            let mut table = DeclarationTable::new("m");
            representation = table.nominal("N", representation).unwrap();
        }
        assert_eq!(
            Type::decode_canonical(&representation.encode_canonical()),
            Err(CanonicalDecodeError::NestingTooDeep {
                limit: MAX_NOMINAL_NESTING
            })
        );
    }
}
