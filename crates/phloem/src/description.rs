//! The package description: a record value (decision 0039).
//!
//! A package version's description is a value — a record carrying the source
//! binding, the build inputs it prescribes, and the options it declares. The
//! description's own content identity is a digest of that value, and it is
//! not the package's identity; it is one revision of the description, on the
//! same terms a rule revision is one revision of a rule. Options are a
//! `List<Text>` of declared option names because `Map` has no constructor
//! yet (0026's closure rule, 0039's explicit refusal to reserve one); if the
//! constraints record shows a lookup that needs keys, that is the evidence
//! to bring.

use pith_core::{RecordField, Type, Value};
use pith_diag::PithResult;
use pith_ids::{ContentDigest, ContentId};

use crate::diag;
use crate::source::{SourceBinding, source_type};

const NAME: &str = "name";
const SOURCE: &str = "source";
const INPUTS: &str = "inputs";
const OPTIONS: &str = "options";

/// The digest domain for a description's content identity. NUL-terminated so
/// it is self-delimiting against the canonical bytes that follow, mirroring
/// the domain separation `pith-ids` applies to every digest kind it owns.
const DESCRIPTION_DOMAIN: &[u8] = b"phloem.description-v1\0";

/// The declared description type: a closed record over the source-binding
/// sum, a list of prescribed build inputs, and a list of declared options.
#[must_use]
pub fn description_type() -> Type {
    let record = Type::record([
        RecordField {
            name: NAME.into(),
            payload: Type::Text,
        },
        RecordField {
            name: SOURCE.into(),
            payload: source_type(),
        },
        RecordField {
            name: INPUTS.into(),
            payload: Type::List(Box::new(Type::Blob)),
        },
        RecordField {
            name: OPTIONS.into(),
            payload: Type::List(Box::new(Type::Text)),
        },
    ]);
    match record {
        Ok(record) => record,
        // The field names are distinct literals, so the duplicate-name
        // rejection is unreachable by construction.
        Err(error) => unreachable!("{error}"),
    }
}

/// A package version's description, as declared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Description {
    pub name: Box<str>,
    pub source: SourceBinding,
    /// The build inputs the description prescribes, as content identities.
    /// Which interface consumes them is the request's business, on the terms
    /// 0039 sets: a description names build interfaces and carries inputs,
    /// it never wraps "build" as a sub-concept of "package".
    pub inputs: Box<[ContentId]>,
    /// The options the description declares, by name.
    pub options: Box<[Box<str>]>,
}

impl Description {
    /// The description as a value of the declared record type.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let record = Value::record([
            RecordField {
                name: NAME.into(),
                payload: Value::Text(self.name.clone()),
            },
            RecordField {
                name: SOURCE.into(),
                payload: self.source.to_value(),
            },
            RecordField {
                name: INPUTS.into(),
                payload: Value::List(self.inputs.iter().map(|id| Value::Blob(*id)).collect()),
            },
            RecordField {
                name: OPTIONS.into(),
                payload: Value::List(
                    self.options
                        .iter()
                        .map(|option| Value::Text(option.clone()))
                        .collect(),
                ),
            },
        ]);
        match record {
            Ok(record) => record,
            // The field names are distinct literals, so the duplicate-name
            // rejection is unreachable by construction.
            Err(error) => unreachable!("{error}"),
        }
    }

    /// Read a description from a value, checking inhabitation with
    /// `is_type` rather than comparing against `value_type`: an empty
    /// options list inhabits `List<Text>` under `is_type` while
    /// `value_type` must name `List<Unit>` (0026's asymmetry, inherited by
    /// every record whose fields can be empty).
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming the declared type and the
    /// value found when the value is not a description.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        if !value.is_type(&description_type()) {
            return Err(diag(format!(
                "expected a value of the package description type, found {}",
                value.describe()
            )));
        }
        let Value::Record(fields) = value else {
            return Err(diag(format!(
                "expected a value of the package description type, found {}",
                value.describe()
            )));
        };
        let mut name = None;
        let mut source = None;
        let mut inputs = Vec::new();
        let mut options = Vec::new();
        for field in fields.iter() {
            match field.name.as_ref() {
                NAME => name = Some(&field.payload),
                SOURCE => source = Some(&field.payload),
                INPUTS => {
                    let Value::List(elements) = &field.payload else {
                        return Err(diag(format!(
                            "the {INPUTS} field carried {} rather than a list",
                            field.payload.describe()
                        )));
                    };
                    for element in elements.iter() {
                        let Value::Blob(id) = element else {
                            return Err(diag(format!(
                                "the {INPUTS} list carried {} rather than a blob",
                                element.describe()
                            )));
                        };
                        inputs.push(*id);
                    }
                }
                OPTIONS => {
                    let Value::List(elements) = &field.payload else {
                        return Err(diag(format!(
                            "the {OPTIONS} field carried {} rather than a list",
                            field.payload.describe()
                        )));
                    };
                    for element in elements.iter() {
                        let Value::Text(option) = element else {
                            return Err(diag(format!(
                                "the {OPTIONS} list carried {} rather than a text",
                                element.describe()
                            )));
                        };
                        options.push(option.clone());
                    }
                }
                _ => {}
            }
        }
        let name = match name {
            Some(Value::Text(name)) => name.clone(),
            _ => return Err(diag(format!("the record carried no {NAME} text"))),
        };
        let source = match source {
            Some(value) => SourceBinding::from_value(value)?,
            None => return Err(diag(format!("the record carried no {SOURCE} binding"))),
        };
        Ok(Self {
            name,
            source,
            inputs: inputs.into(),
            options: options.into(),
        })
    }

    /// The description's own content identity: a digest over its canonical
    /// encoding. This identifies one revision of the description; it is not
    /// the package's identity, and two revisions of one description name one
    /// package (0039).
    #[must_use]
    pub fn content_id(&self) -> ContentId {
        let canonical = self.to_value().encode_canonical();
        let mut domain_prefixed = DESCRIPTION_DOMAIN.to_vec();
        domain_prefixed.extend_from_slice(&canonical);
        ContentId::from_digest(ContentDigest::of_bytes(&domain_prefixed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn description() -> Description {
        Description {
            name: "zlib".into(),
            source: SourceBinding::Git {
                revision: "9f11b1d".into(),
                tree: "e3b0c44".into(),
            },
            inputs: Box::new([ContentId::of_blob(b"zlib.c"), ContentId::of_blob(b"zlib.h")]),
            options: Box::new(["shared".into()]),
        }
    }

    #[test]
    fn a_description_round_trips_through_the_canonical_codec() {
        let value = description().to_value();
        assert!(value.is_type(&description_type()));
        let encoded = value.encode_canonical();
        let decoded = Value::decode_canonical(&encoded).unwrap();
        assert_eq!(decoded, value);
        assert_eq!(Description::from_value(&decoded).unwrap(), description());
    }

    #[test]
    fn a_description_digests_stably_across_the_codec_boundary() {
        // The digest is taken over canonical bytes, so the same description
        // reaching it through a decode — the shape a second process reading
        // a stored description takes — produces the same content identity.
        let declared = description();
        let decoded = Value::decode_canonical(&declared.to_value().encode_canonical()).unwrap();
        let read_back = Description::from_value(&decoded).unwrap();
        assert_eq!(read_back.content_id(), declared.content_id());
    }

    #[test]
    fn an_empty_options_list_is_a_description() {
        // The empty list is where value_type and is_type disagree (0026), so
        // this is the description-level check that the asymmetry a record
        // inherits from its fields does not reject honest declarations.
        let mut sparse = description();
        sparse.options = Box::new([]);
        sparse.inputs = Box::new([]);
        let value = sparse.to_value();
        assert_ne!(value.value_type(), description_type());
        assert!(value.is_type(&description_type()));
        assert_eq!(Description::from_value(&value).unwrap(), sparse);
    }

    #[test]
    fn a_record_missing_a_field_is_not_a_description() {
        let missing = Value::record([RecordField {
            name: NAME.into(),
            payload: Value::Text("zlib".into()),
        }])
        .unwrap();
        assert!(!missing.is_type(&description_type()));
        assert!(Description::from_value(&missing).is_err());
    }
}
