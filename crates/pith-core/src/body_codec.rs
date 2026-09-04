//! The canonical encoding of a represented rule body (decisions 0038, 0062).
//!
//! The body is a grammar beside `Value` and `Type` with its own tag namespace
//! and version gate, not a rider on `RECORD_ENCODING_VERSION`: bodies change
//! at their own rate, and 0023 asks for the version to ride a digest domain
//! so a semantic change is a domain bump rather than a silent basis change.
//! The discipline is the canonical codec's — length-prefixed, depth-bounded,
//! tag-numbered — with embedded values and types carried as length-prefixed
//! payloads of the existing encodings.

use crate::body::{BodyExpr, BodyRequest, MAX_BODY_DEPTH, MatchArm, RuleBody};
use crate::codec::CanonicalReader;
use crate::manifest::{encode_bytes, encode_length, encode_str};
use crate::rule::{Interface, encode_interface};
use crate::value::{RecordField, Type, Value};
use crate::value_codec::{CanonicalDecodeError, decode_type_payload, encode_type_payload};

/// Version of the represented-body encoding, pinned at 1 until the first
/// release (decision 0048). A grammar change under this gate is answered by
/// discarding and rebuilding, and a change to evaluator semantics — anything
/// that would move what a body means without moving these bytes — is a
/// `pith:body-ir` domain bump instead.
pub const BODY_ENCODING_VERSION: u8 = 1;

const TAG_LITERAL: u8 = 0;
const TAG_BOUND: u8 = 1;
const TAG_LET: u8 = 2;
const TAG_FAIL: u8 = 3;
const TAG_RECORD: u8 = 4;
const TAG_FIELD: u8 = 5;
const TAG_MAKE_SUM: u8 = 6;
const TAG_MATCH: u8 = 7;
const TAG_WRAP: u8 = 8;
const TAG_UNWRAP: u8 = 9;
const TAG_LIST: u8 = 10;
const TAG_CONS: u8 = 11;
const TAG_MATCH_LIST: u8 = 29;
const TAG_APPEND: u8 = 12;
const TAG_FOLD: u8 = 13;
const TAG_SORT_BY: u8 = 14;
const TAG_IF: u8 = 15;
const TAG_EQUAL: u8 = 16;
const TAG_INT_ADD: u8 = 17;
const TAG_INT_SUBTRACT: u8 = 18;
const TAG_INT_MULTIPLY: u8 = 19;
const TAG_DESCRIBE: u8 = 20;
const TAG_TEXT_CONCAT: u8 = 21;
const TAG_TEXT_OF_BYTES: u8 = 22;
const TAG_NEED: u8 = 23;
const TAG_NEED_ALL: u8 = 24;
const TAG_NEED_EACH: u8 = 25;
const TAG_NEED_BLOB: u8 = 26;
const TAG_NEED_ACTION: u8 = 27;
const TAG_NEED_OBSERVATION: u8 = 28;
const TAG_TEXT_BREAK: u8 = 30;
const TAG_TEXT_JOIN: u8 = 31;

impl RuleBody {
    /// Encode the body in the current version of the canonical body format.
    #[must_use]
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut encoded = vec![BODY_ENCODING_VERSION];
        encode_expression(&mut encoded, self.expression());
        encoded
    }

    /// Decode one body from the versioned canonical body format.
    ///
    /// # Errors
    /// Returns an error for unsupported versions, unknown tags, truncated or
    /// trailing data, and names out of canonical order.
    pub fn decode_canonical(encoded: &[u8]) -> Result<Self, CanonicalDecodeError> {
        let mut decoder = CanonicalReader::new(encoded);
        decoder.read_version(BODY_ENCODING_VERSION)?;
        let expression = decode_expression(&mut decoder, 0)?;
        decoder.finish()?;
        Ok(Self::new(expression))
    }
}

fn encode_expression(encoded: &mut Vec<u8>, expression: &BodyExpr) {
    match expression {
        BodyExpr::Literal(value) => {
            encoded.push(TAG_LITERAL);
            encode_bytes(encoded, &value.encode_canonical());
        }
        BodyExpr::Bound(index) => {
            encoded.push(TAG_BOUND);
            encode_length(encoded, *index);
        }
        BodyExpr::Let { bound, rest } => {
            encoded.push(TAG_LET);
            encode_expression(encoded, bound);
            encode_expression(encoded, rest);
        }
        BodyExpr::Fail { message } => {
            encoded.push(TAG_FAIL);
            encode_expression(encoded, message);
        }
        BodyExpr::Record { fields } => {
            encoded.push(TAG_RECORD);
            encode_length(encoded, fields.len());
            for field in fields.iter() {
                encode_str(encoded, &field.name);
                encode_expression(encoded, &field.payload);
            }
        }
        BodyExpr::Field { record, name } => {
            encoded.push(TAG_FIELD);
            encode_expression(encoded, record);
            encode_str(encoded, name);
        }
        BodyExpr::MakeSum {
            declared,
            constructor,
            payload,
        } => {
            encoded.push(TAG_MAKE_SUM);
            encode_type_payload(encoded, &Type::Sum(Box::new(declared.clone())));
            encode_str(encoded, constructor);
            match payload {
                Some(payload) => {
                    encoded.push(1);
                    encode_expression(encoded, payload);
                }
                None => encoded.push(0),
            }
        }
        BodyExpr::Match { scrutinee, arms } => {
            encoded.push(TAG_MATCH);
            encode_expression(encoded, scrutinee);
            encode_length(encoded, arms.len());
            for arm in arms.iter() {
                encode_str(encoded, &arm.constructor);
                encode_expression(encoded, &arm.body);
            }
        }
        BodyExpr::Wrap {
            declared,
            representation,
        } => {
            encoded.push(TAG_WRAP);
            encode_type_payload(encoded, &Type::Nominal(Box::new(declared.clone())));
            encode_expression(encoded, representation);
        }
        BodyExpr::Unwrap { nominal } => {
            encoded.push(TAG_UNWRAP);
            encode_expression(encoded, nominal);
        }
        BodyExpr::List { element, items } => {
            encoded.push(TAG_LIST);
            encode_type_payload(encoded, element);
            encode_length(encoded, items.len());
            for item in items.iter() {
                encode_expression(encoded, item);
            }
        }
        BodyExpr::Cons { head, tail } => {
            encoded.push(TAG_CONS);
            encode_expression(encoded, head);
            encode_expression(encoded, tail);
        }
        BodyExpr::MatchList { list, empty, cons } => {
            encoded.push(TAG_MATCH_LIST);
            encode_expression(encoded, list);
            encode_expression(encoded, empty);
            encode_expression(encoded, cons);
        }
        BodyExpr::Append { left, right } => {
            encoded.push(TAG_APPEND);
            encode_expression(encoded, left);
            encode_expression(encoded, right);
        }
        BodyExpr::Fold { source, init, step } => {
            encoded.push(TAG_FOLD);
            encode_expression(encoded, source);
            encode_expression(encoded, init);
            encode_expression(encoded, step);
        }
        BodyExpr::SortBy { list, key } => {
            encoded.push(TAG_SORT_BY);
            encode_expression(encoded, list);
            encode_expression(encoded, key);
        }
        BodyExpr::If {
            condition,
            then,
            otherwise,
        } => {
            encoded.push(TAG_IF);
            encode_expression(encoded, condition);
            encode_expression(encoded, then);
            encode_expression(encoded, otherwise);
        }
        BodyExpr::Equal { left, right } => binary(encoded, TAG_EQUAL, left, right),
        BodyExpr::IntAdd { left, right } => binary(encoded, TAG_INT_ADD, left, right),
        BodyExpr::IntSubtract { left, right } => binary(encoded, TAG_INT_SUBTRACT, left, right),
        BodyExpr::IntMultiply { left, right } => binary(encoded, TAG_INT_MULTIPLY, left, right),
        BodyExpr::TextConcat { left, right } => binary(encoded, TAG_TEXT_CONCAT, left, right),
        BodyExpr::Describe { value } => {
            encoded.push(TAG_DESCRIBE);
            encode_expression(encoded, value);
        }
        BodyExpr::TextOfBytes { bytes } => {
            encoded.push(TAG_TEXT_OF_BYTES);
            encode_expression(encoded, bytes);
        }
        BodyExpr::TextBreak { text, separator } => {
            binary(encoded, TAG_TEXT_BREAK, text, separator);
        }
        BodyExpr::TextJoin { list, separator } => {
            binary(encoded, TAG_TEXT_JOIN, list, separator);
        }
        BodyExpr::Need { request, resume } => {
            encoded.push(TAG_NEED);
            encode_request(encoded, request);
            encode_expression(encoded, resume);
        }
        BodyExpr::NeedAll { requests, resume } => {
            encoded.push(TAG_NEED_ALL);
            encode_length(encoded, requests.len());
            for request in requests.iter() {
                encode_request(encoded, request);
            }
            encode_expression(encoded, resume);
        }
        BodyExpr::NeedEach {
            source,
            request,
            resume,
        } => {
            encoded.push(TAG_NEED_EACH);
            encode_expression(encoded, source);
            encode_request(encoded, request);
            encode_expression(encoded, resume);
        }
        BodyExpr::NeedBlob { content, resume } => {
            encoded.push(TAG_NEED_BLOB);
            encode_expression(encoded, content);
            encode_expression(encoded, resume);
        }
        BodyExpr::NeedAction { request, resume } => {
            encoded.push(TAG_NEED_ACTION);
            encode_request(encoded, request);
            encode_expression(encoded, resume);
        }
        BodyExpr::NeedObservation { request, resume } => {
            encoded.push(TAG_NEED_OBSERVATION);
            encode_request(encoded, request);
            encode_expression(encoded, resume);
        }
    }
}

fn binary(encoded: &mut Vec<u8>, tag: u8, left: &BodyExpr, right: &BodyExpr) {
    encoded.push(tag);
    encode_expression(encoded, left);
    encode_expression(encoded, right);
}

fn encode_request(encoded: &mut Vec<u8>, request: &BodyRequest) {
    encode_interface(encoded, &request.interface);
    encode_length(encoded, request.inputs.len());
    for input in request.inputs.iter() {
        encode_expression(encoded, input);
    }
}

fn decode_expression(
    decoder: &mut CanonicalReader<'_>,
    depth: u32,
) -> Result<BodyExpr, CanonicalDecodeError> {
    if depth >= MAX_BODY_DEPTH {
        return Err(CanonicalDecodeError::NestingTooDeep {
            limit: MAX_BODY_DEPTH,
        });
    }
    let deeper = depth.saturating_add(1);
    match decoder.read_byte()? {
        TAG_LITERAL => {
            let payload = decoder.read_bytes()?;
            let value = Value::decode_canonical(payload)?;
            Ok(BodyExpr::Literal(value))
        }
        TAG_BOUND => Ok(BodyExpr::Bound(decoder.read_length()?)),
        TAG_LET => Ok(BodyExpr::Let {
            bound: Box::new(decode_expression(decoder, deeper)?),
            rest: Box::new(decode_expression(decoder, deeper)?),
        }),
        TAG_FAIL => Ok(BodyExpr::Fail {
            message: Box::new(decode_expression(decoder, deeper)?),
        }),
        TAG_RECORD => {
            let fields = decoder.read_sequence(|decoder| {
                let name: Box<str> = decoder.read_text()?.into();
                Ok(RecordField {
                    name,
                    payload: decode_expression(decoder, deeper)?,
                })
            })?;
            Ok(BodyExpr::Record { fields })
        }
        TAG_FIELD => Ok(BodyExpr::Field {
            record: Box::new(decode_expression(decoder, deeper)?),
            name: decoder.read_text()?.into(),
        }),
        TAG_MAKE_SUM => {
            let Type::Sum(declared) = decode_type_payload(decoder)? else {
                return Err(CanonicalDecodeError::TypeInBodyPosition);
            };
            let constructor: Box<str> = decoder.read_text()?.into();
            let payload = match decoder.read_bool()? {
                true => Some(Box::new(decode_expression(decoder, deeper)?)),
                false => None,
            };
            Ok(BodyExpr::MakeSum {
                declared: *declared,
                constructor,
                payload,
            })
        }
        TAG_MATCH => {
            let scrutinee = Box::new(decode_expression(decoder, deeper)?);
            let arms = decoder.read_sequence(|decoder| {
                let constructor: Box<str> = decoder.read_text()?.into();
                Ok(MatchArm {
                    constructor,
                    body: Box::new(decode_expression(decoder, deeper)?),
                })
            })?;
            Ok(BodyExpr::Match { scrutinee, arms })
        }
        TAG_WRAP => {
            let Type::Nominal(declared) = decode_type_payload(decoder)? else {
                return Err(CanonicalDecodeError::TypeInBodyPosition);
            };
            Ok(BodyExpr::Wrap {
                declared: *declared,
                representation: Box::new(decode_expression(decoder, deeper)?),
            })
        }
        TAG_UNWRAP => Ok(BodyExpr::Unwrap {
            nominal: Box::new(decode_expression(decoder, deeper)?),
        }),
        TAG_LIST => {
            let element = decode_type_payload(decoder)?;
            let items = decoder.read_sequence(|decoder| decode_expression(decoder, deeper))?;
            Ok(BodyExpr::List { element, items })
        }
        TAG_CONS => Ok(BodyExpr::Cons {
            head: Box::new(decode_expression(decoder, deeper)?),
            tail: Box::new(decode_expression(decoder, deeper)?),
        }),
        TAG_MATCH_LIST => Ok(BodyExpr::MatchList {
            list: Box::new(decode_expression(decoder, deeper)?),
            empty: Box::new(decode_expression(decoder, deeper)?),
            cons: Box::new(decode_expression(decoder, deeper)?),
        }),
        TAG_APPEND => Ok(BodyExpr::Append {
            left: Box::new(decode_expression(decoder, deeper)?),
            right: Box::new(decode_expression(decoder, deeper)?),
        }),
        TAG_FOLD => Ok(BodyExpr::Fold {
            source: Box::new(decode_expression(decoder, deeper)?),
            init: Box::new(decode_expression(decoder, deeper)?),
            step: Box::new(decode_expression(decoder, deeper)?),
        }),
        TAG_SORT_BY => Ok(BodyExpr::SortBy {
            list: Box::new(decode_expression(decoder, deeper)?),
            key: Box::new(decode_expression(decoder, deeper)?),
        }),
        TAG_IF => Ok(BodyExpr::If {
            condition: Box::new(decode_expression(decoder, deeper)?),
            then: Box::new(decode_expression(decoder, deeper)?),
            otherwise: Box::new(decode_expression(decoder, deeper)?),
        }),
        TAG_EQUAL => pair(decoder, deeper, |left, right| BodyExpr::Equal {
            left,
            right,
        }),
        TAG_INT_ADD => pair(decoder, deeper, |left, right| BodyExpr::IntAdd {
            left,
            right,
        }),
        TAG_INT_SUBTRACT => pair(decoder, deeper, |left, right| BodyExpr::IntSubtract {
            left,
            right,
        }),
        TAG_INT_MULTIPLY => pair(decoder, deeper, |left, right| BodyExpr::IntMultiply {
            left,
            right,
        }),
        TAG_TEXT_CONCAT => pair(decoder, deeper, |left, right| BodyExpr::TextConcat {
            left,
            right,
        }),
        TAG_DESCRIBE => Ok(BodyExpr::Describe {
            value: Box::new(decode_expression(decoder, deeper)?),
        }),
        TAG_TEXT_OF_BYTES => Ok(BodyExpr::TextOfBytes {
            bytes: Box::new(decode_expression(decoder, deeper)?),
        }),
        TAG_TEXT_BREAK => pair(decoder, deeper, |text, separator| BodyExpr::TextBreak {
            text,
            separator,
        }),
        TAG_TEXT_JOIN => pair(decoder, deeper, |list, separator| BodyExpr::TextJoin {
            list,
            separator,
        }),
        TAG_NEED => decode_need(decoder, deeper),
        TAG_NEED_ALL => decode_need_all(decoder, deeper),
        TAG_NEED_EACH => decode_need_each(decoder, deeper),
        TAG_NEED_BLOB => decode_need_blob(decoder, deeper),
        TAG_NEED_ACTION => decode_need_action(decoder, deeper),
        TAG_NEED_OBSERVATION => decode_need_observation(decoder, deeper),
        tag => Err(CanonicalDecodeError::UnknownBodyTag { tag }),
    }
}

/// The yield arms live outside `decode_expression`'s frame so a deep body
/// cannot overflow the stack through either half of the codec.
fn decode_need(
    decoder: &mut CanonicalReader<'_>,
    depth: u32,
) -> Result<BodyExpr, CanonicalDecodeError> {
    Ok(BodyExpr::Need {
        request: decode_request(decoder, depth)?,
        resume: Box::new(decode_expression(decoder, depth)?),
    })
}

fn decode_need_all(
    decoder: &mut CanonicalReader<'_>,
    depth: u32,
) -> Result<BodyExpr, CanonicalDecodeError> {
    let requests = decoder.read_sequence(|decoder| decode_request(decoder, depth))?;
    Ok(BodyExpr::NeedAll {
        requests,
        resume: Box::new(decode_expression(decoder, depth)?),
    })
}

fn decode_need_each(
    decoder: &mut CanonicalReader<'_>,
    depth: u32,
) -> Result<BodyExpr, CanonicalDecodeError> {
    Ok(BodyExpr::NeedEach {
        source: Box::new(decode_expression(decoder, depth)?),
        request: decode_request(decoder, depth)?,
        resume: Box::new(decode_expression(decoder, depth)?),
    })
}

fn decode_need_blob(
    decoder: &mut CanonicalReader<'_>,
    depth: u32,
) -> Result<BodyExpr, CanonicalDecodeError> {
    Ok(BodyExpr::NeedBlob {
        content: Box::new(decode_expression(decoder, depth)?),
        resume: Box::new(decode_expression(decoder, depth)?),
    })
}

fn decode_need_action(
    decoder: &mut CanonicalReader<'_>,
    depth: u32,
) -> Result<BodyExpr, CanonicalDecodeError> {
    Ok(BodyExpr::NeedAction {
        request: decode_request(decoder, depth)?,
        resume: Box::new(decode_expression(decoder, depth)?),
    })
}

fn decode_need_observation(
    decoder: &mut CanonicalReader<'_>,
    depth: u32,
) -> Result<BodyExpr, CanonicalDecodeError> {
    Ok(BodyExpr::NeedObservation {
        request: decode_request(decoder, depth)?,
        resume: Box::new(decode_expression(decoder, depth)?),
    })
}

fn pair(
    decoder: &mut CanonicalReader<'_>,
    depth: u32,
    make: impl Fn(Box<BodyExpr>, Box<BodyExpr>) -> BodyExpr,
) -> Result<BodyExpr, CanonicalDecodeError> {
    let left = Box::new(decode_expression(decoder, depth)?);
    let right = Box::new(decode_expression(decoder, depth)?);
    Ok(make(left, right))
}

fn decode_request(
    decoder: &mut CanonicalReader<'_>,
    depth: u32,
) -> Result<BodyRequest, CanonicalDecodeError> {
    let inputs = decoder.read_sequence(|decoder| decode_type_payload(decoder))?;
    let output = decode_type_payload(decoder)?;
    let request_inputs = decoder.read_sequence(|decoder| decode_expression(decoder, depth))?;
    Ok(BodyRequest {
        interface: Interface { inputs, output },
        inputs: request_inputs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::MatchArm;
    use crate::rule::Interface;
    use crate::value::SumConstructor;
    use pith_ids::BodyIrDigest;

    fn round_trips(body: &RuleBody) {
        let encoded = body.encode_canonical();
        let decoded = RuleBody::decode_canonical(&encoded)
            .unwrap_or_else(|error| unreachable!("a canonical body decodes: {error}"));
        assert_eq!(&decoded, body);
    }

    #[test]
    fn a_literal_body_round_trips_and_pins_the_golden_bytes() {
        let body = RuleBody::new(BodyExpr::Literal(Value::Bool(true)));
        assert_eq!(
            body.encode_canonical(),
            [
                BODY_ENCODING_VERSION,
                TAG_LITERAL,
                // the embedded value's own encoding, length-prefixed
                3,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                crate::ENCODING_VERSION,
                crate::value_codec::TAG_BOOL,
                1,
            ]
        );
        round_trips(&body);
    }

    #[test]
    fn a_request_body_round_trips_and_pins_the_golden_bytes() {
        let interface = Interface {
            inputs: Box::new([Type::Int]),
            output: Type::Text,
        };
        let body = RuleBody::new(BodyExpr::Need {
            request: BodyRequest {
                interface,
                inputs: Box::new([BodyExpr::Bound(0)]),
            },
            resume: Box::new(BodyExpr::Bound(0)),
        });
        assert_eq!(
            body.encode_canonical(),
            [
                BODY_ENCODING_VERSION,
                TAG_NEED,
                // the interface: one input, then the output
                1,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                crate::value_codec::TAG_INT,
                crate::value_codec::TAG_TEXT,
                // one request input, then the resume
                1,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                TAG_BOUND,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                TAG_BOUND,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ]
        );
        round_trips(&body);
    }

    #[test]
    fn every_constructor_round_trips() {
        let sum = {
            let mut table = crate::DeclarationTable::new("test");
            match table.sum(
                "Shape",
                [
                    SumConstructor {
                        name: "circle".into(),
                        payload: Some(Type::Int),
                    },
                    SumConstructor {
                        name: "square".into(),
                        payload: None,
                    },
                ],
            ) {
                Ok(Type::Sum(sum)) => *sum,
                _ => unreachable!("a fresh table admits one sum"),
            }
        };
        let nominal = {
            let mut table = crate::DeclarationTable::new("test");
            match table.nominal("Object", Type::Blob) {
                Ok(Type::Nominal(declared)) => *declared,
                _ => unreachable!("a fresh table admits one nominal"),
            }
        };
        let bodies = [
            RuleBody::new(BodyExpr::Literal(Value::int(7))),
            RuleBody::new(BodyExpr::Bound(3)),
            RuleBody::new(BodyExpr::Let {
                bound: Box::new(BodyExpr::Literal(Value::Unit)),
                rest: Box::new(BodyExpr::Bound(0)),
            }),
            RuleBody::new(BodyExpr::Fail {
                message: Box::new(BodyExpr::Literal(Value::Text("no".into()))),
            }),
            RuleBody::new(
                BodyExpr::record([crate::RecordField {
                    name: "path".into(),
                    payload: BodyExpr::Literal(Value::Text("a.c".into())),
                }])
                .unwrap(),
            ),
            RuleBody::new(BodyExpr::Field {
                record: Box::new(BodyExpr::Bound(0)),
                name: "path".into(),
            }),
            RuleBody::new(BodyExpr::MakeSum {
                declared: sum.clone(),
                constructor: "circle".into(),
                payload: Some(Box::new(BodyExpr::Literal(Value::int(3)))),
            }),
            RuleBody::new(BodyExpr::Match {
                scrutinee: Box::new(BodyExpr::Bound(0)),
                arms: Box::new([
                    MatchArm {
                        constructor: "circle".into(),
                        body: Box::new(BodyExpr::Bound(0)),
                    },
                    MatchArm {
                        constructor: "square".into(),
                        body: Box::new(BodyExpr::Literal(Value::int(0))),
                    },
                ]),
            }),
            RuleBody::new(BodyExpr::Wrap {
                declared: nominal.clone(),
                representation: Box::new(BodyExpr::Bound(0)),
            }),
            RuleBody::new(BodyExpr::Unwrap {
                nominal: Box::new(BodyExpr::Bound(0)),
            }),
            RuleBody::new(BodyExpr::List {
                element: Type::Text,
                items: Box::new([]),
            }),
            RuleBody::new(BodyExpr::Cons {
                head: Box::new(BodyExpr::Literal(Value::int(1))),
                tail: Box::new(BodyExpr::Bound(0)),
            }),
            RuleBody::new(BodyExpr::Append {
                left: Box::new(BodyExpr::Bound(0)),
                right: Box::new(BodyExpr::Bound(1)),
            }),
            RuleBody::new(BodyExpr::MatchList {
                list: Box::new(BodyExpr::Bound(0)),
                empty: Box::new(BodyExpr::Literal(Value::int(0))),
                cons: Box::new(BodyExpr::IntAdd {
                    left: Box::new(BodyExpr::Bound(0)),
                    right: Box::new(BodyExpr::Literal(Value::int(1))),
                }),
            }),
            RuleBody::new(BodyExpr::Fold {
                source: Box::new(BodyExpr::Bound(0)),
                init: Box::new(BodyExpr::Literal(Value::int(0))),
                step: Box::new(BodyExpr::IntAdd {
                    left: Box::new(BodyExpr::Bound(1)),
                    right: Box::new(BodyExpr::Bound(0)),
                }),
            }),
            RuleBody::new(BodyExpr::SortBy {
                list: Box::new(BodyExpr::Bound(0)),
                key: Box::new(BodyExpr::Bound(0)),
            }),
            RuleBody::new(BodyExpr::If {
                condition: Box::new(BodyExpr::Equal {
                    left: Box::new(BodyExpr::Bound(0)),
                    right: Box::new(BodyExpr::Bound(1)),
                }),
                then: Box::new(BodyExpr::Literal(Value::int(1))),
                otherwise: Box::new(BodyExpr::Literal(Value::int(0))),
            }),
            RuleBody::new(BodyExpr::IntSubtract {
                left: Box::new(BodyExpr::Bound(0)),
                right: Box::new(BodyExpr::Literal(Value::int(1))),
            }),
            RuleBody::new(BodyExpr::IntMultiply {
                left: Box::new(BodyExpr::Bound(0)),
                right: Box::new(BodyExpr::Literal(Value::int(2))),
            }),
            RuleBody::new(BodyExpr::TextConcat {
                left: Box::new(BodyExpr::Describe {
                    value: Box::new(BodyExpr::Bound(0)),
                }),
                right: Box::new(BodyExpr::Literal(Value::Text("\n".into()))),
            }),
            RuleBody::new(BodyExpr::TextOfBytes {
                bytes: Box::new(BodyExpr::Bound(0)),
            }),
            RuleBody::new(BodyExpr::TextBreak {
                text: Box::new(BodyExpr::Bound(0)),
                separator: Box::new(BodyExpr::Literal(Value::Text(",".into()))),
            }),
            RuleBody::new(BodyExpr::TextJoin {
                list: Box::new(BodyExpr::Bound(0)),
                separator: Box::new(BodyExpr::Literal(Value::Text(",".into()))),
            }),
            RuleBody::new(BodyExpr::NeedBlob {
                content: Box::new(BodyExpr::Bound(0)),
                resume: Box::new(BodyExpr::Bound(0)),
            }),
            RuleBody::new(BodyExpr::NeedObservation {
                request: BodyRequest {
                    interface: Interface {
                        inputs: Box::new([Type::Text]),
                        output: Type::Text,
                    },
                    inputs: Box::new([BodyExpr::Bound(0)]),
                },
                resume: Box::new(BodyExpr::Bound(0)),
            }),
            RuleBody::new(BodyExpr::NeedAction {
                request: BodyRequest {
                    interface: Interface {
                        inputs: Box::new([Type::Blob]),
                        output: Type::Blob,
                    },
                    inputs: Box::new([BodyExpr::Bound(0)]),
                },
                resume: Box::new(BodyExpr::Bound(0)),
            }),
            RuleBody::new(BodyExpr::NeedEach {
                source: Box::new(BodyExpr::Bound(0)),
                request: BodyRequest {
                    interface: Interface {
                        inputs: Box::new([Type::Blob]),
                        output: Type::Blob,
                    },
                    inputs: Box::new([BodyExpr::Bound(0)]),
                },
                resume: Box::new(BodyExpr::Bound(0)),
            }),
            RuleBody::new(BodyExpr::NeedAll {
                requests: Box::new([
                    BodyRequest {
                        interface: Interface {
                            inputs: Box::new([Type::Blob]),
                            output: Type::Blob,
                        },
                        inputs: Box::new([BodyExpr::Bound(0)]),
                    },
                    BodyRequest {
                        interface: Interface {
                            inputs: Box::new([Type::Text]),
                            output: Type::Text,
                        },
                        inputs: Box::new([BodyExpr::Bound(0)]),
                    },
                ]),
                resume: Box::new(BodyExpr::Bound(0)),
            }),
        ];
        for body in &bodies {
            round_trips(body);
        }
    }

    #[test]
    fn the_digest_is_a_function_of_the_canonical_bytes() {
        let body = RuleBody::new(BodyExpr::Let {
            bound: Box::new(BodyExpr::Literal(Value::int(1))),
            rest: Box::new(BodyExpr::Bound(0)),
        });
        let same = RuleBody::new(BodyExpr::Let {
            bound: Box::new(BodyExpr::Literal(Value::int(1))),
            rest: Box::new(BodyExpr::Bound(0)),
        });
        let changed = RuleBody::new(BodyExpr::Let {
            bound: Box::new(BodyExpr::Literal(Value::int(2))),
            rest: Box::new(BodyExpr::Bound(0)),
        });

        assert_eq!(body.digest(), same.digest());
        assert_ne!(body.digest(), changed.digest());
        assert_eq!(
            body.digest(),
            BodyIrDigest::of_manifest(&body.encode_canonical())
        );
        let decoded = RuleBody::decode_canonical(&body.encode_canonical()).unwrap();
        assert_eq!(decoded.digest(), body.digest());
    }

    #[test]
    fn decode_refuses_foreign_versions_tags_and_trailing_bytes() {
        let body = RuleBody::new(BodyExpr::Literal(Value::Unit));
        let encoded = body.encode_canonical();

        let mut wrong_version = encoded.clone();
        if let Some(first) = wrong_version.first_mut() {
            *first = BODY_ENCODING_VERSION + 1;
        }
        assert_eq!(
            RuleBody::decode_canonical(&wrong_version).err(),
            Some(CanonicalDecodeError::UnsupportedVersion {
                version: BODY_ENCODING_VERSION + 1
            })
        );

        let mut unknown_tag = encoded.clone();
        if let Some(tag) = unknown_tag.get_mut(1) {
            *tag = 32;
        }
        assert_eq!(
            RuleBody::decode_canonical(&unknown_tag).err(),
            Some(CanonicalDecodeError::UnknownBodyTag { tag: 32 })
        );

        let mut trailing = encoded.clone();
        trailing.push(0);
        assert_eq!(
            RuleBody::decode_canonical(&trailing).err(),
            Some(CanonicalDecodeError::TrailingBytes)
        );
        let last = encoded.len().saturating_sub(1);
        let (truncated, _) = encoded.split_at(last);
        assert_eq!(
            RuleBody::decode_canonical(truncated).err(),
            Some(CanonicalDecodeError::Truncated)
        );
    }

    #[test]
    fn a_body_nesting_past_the_bound_is_refused_on_decode() {
        let mut expression = BodyExpr::Literal(Value::Unit);
        for _ in 0..MAX_BODY_DEPTH {
            expression = BodyExpr::Let {
                bound: Box::new(expression),
                rest: Box::new(BodyExpr::Bound(0)),
            };
        }
        let body = RuleBody::new(expression);
        assert_eq!(
            RuleBody::decode_canonical(&body.encode_canonical()).err(),
            Some(CanonicalDecodeError::NestingTooDeep {
                limit: MAX_BODY_DEPTH
            })
        );
    }
}
