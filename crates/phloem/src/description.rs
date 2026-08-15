//! Package descriptions and their value encoding.
//!
//! A description records a package name, source binding, and build
//! declaration. Its content identity is derived from the canonical record
//! value.

use pith_core::{Type, Value};
use pith_diag::PithResult;
use pith_ids::ContentId;

use crate::build::{PackageBuild, build_type};
use crate::codec::{field_of, record_type, record_value, text_field, value_content_id};
use crate::diag;
use crate::source::{SourceBinding, source_type};

const NAME: &str = "name";
const SOURCE: &str = "source";
const BUILD: &str = "build";

/// The digest domain for a description's content identity. NUL-terminated so
/// it is self-delimiting against the canonical bytes that follow, mirroring
/// the domain separation `pith-ids` applies to every digest kind it owns.
const DESCRIPTION_DOMAIN: &[u8] = b"phloem.description-v2\0";

/// The declared description type: a closed record over the source-binding
/// sum and the declared package build.
#[must_use]
pub fn description_type() -> Type {
    record_type([
        (NAME, Type::Text),
        (SOURCE, source_type()),
        (BUILD, build_type()),
    ])
}

/// A package version's description, as declared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Description {
    pub name: Box<str>,
    pub source: SourceBinding,
    /// Source paths to compile, in link order.
    pub build: PackageBuild,
}

impl Description {
    /// The description as a value of the declared record type.
    #[must_use]
    pub fn to_value(&self) -> Value {
        record_value([
            (NAME, Value::Text(self.name.clone())),
            (SOURCE, self.source.to_value()),
            (BUILD, self.build.to_value()),
        ])
    }

    /// Decodes a package description from `value`.
    ///
    /// # Errors
    /// Returns a diagnostic when `value` is not a package description.
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
        let build = match field_of(fields, BUILD) {
            Some(payload) => PackageBuild::from_value(payload)?,
            None => return Err(diag(format!("the record carried no {BUILD} declaration"))),
        };
        Ok(Self {
            name,
            source,
            build,
        })
    }

    /// Returns the content identity of the canonical description value.
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
            build: PackageBuild {
                sources: Box::new(["zlib-1.3/zlib.c".into(), "zlib-1.3/adler32.c".into()]),
                includes: Box::new([]),
            },
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
        let declared = description();
        let decoded = Value::decode_canonical(&declared.to_value().encode_canonical()).unwrap();
        let read_back = Description::from_value(&decoded).unwrap();
        assert_eq!(read_back.content_id(), declared.content_id());
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
