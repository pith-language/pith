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

use pith_core::{Type, Value};
use pith_diag::PithResult;
use pith_ids::ContentId;

use crate::codec::{field_of, record_type, record_value, text_field, text_list, value_content_id};
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
    record_type([
        (NAME, Type::Text),
        (SOURCE, source_type()),
        (INPUTS, Type::List(Box::new(Type::Blob))),
        (OPTIONS, Type::List(Box::new(Type::Text))),
    ])
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
        record_value([
            (NAME, Value::Text(self.name.clone())),
            (SOURCE, self.source.to_value()),
            (
                INPUTS,
                Value::List(self.inputs.iter().map(|id| Value::Blob(*id)).collect()),
            ),
            (
                OPTIONS,
                Value::List(
                    self.options
                        .iter()
                        .map(|option| Value::Text(option.clone()))
                        .collect(),
                ),
            ),
        ])
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
        let name = text_field(fields, NAME)?;
        let source = match field_of(fields, SOURCE) {
            Some(payload) => SourceBinding::from_value(payload)?,
            None => return Err(diag(format!("the record carried no {SOURCE} binding"))),
        };
        let mut inputs = Vec::new();
        if let Some(Value::List(elements)) = field_of(fields, INPUTS) {
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
        let options = match field_of(fields, OPTIONS) {
            Some(payload) => text_list(payload, OPTIONS)?,
            None => Vec::new(),
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
        value_content_id(DESCRIPTION_DOMAIN, &self.to_value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pith_core::RecordField;

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
