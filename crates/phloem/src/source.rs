//! The source binding: a declared sum with typed payloads (decision 0039).
//!
//! A source is a fixed set of constructors carrying different payloads — a
//! registry archive with its digest, a git revision with its tree hash, a
//! local path with its content identity. 0039 argues this is a sum and not a
//! record with a tag field: the tag spelling re-creates the flat-namespace
//! ambiguity 0026 rejected polymorphic variants for, at a smaller scale. The
//! payloads are typed: an archive carries its content identity, a git
//! revision carries a record of its revision and tree hash, a local path
//! carries a record of its path and content identity.

use pith_core::{RecordField, SumConstructor, Type, Value};
use pith_diag::PithResult;
use pith_ids::ContentId;

use crate::diag;

/// The declared sum's name.
pub const SOURCE: &str = "phloem.Source";

const ARCHIVE: &str = "Archive";
const GIT: &str = "Git";
const PATH: &str = "Path";

/// The record payload a git revision carries: the revision and the tree hash
/// it resolved to, the pair go.sum pins as its `h1:` line does.
pub const GIT_REVISION: &str = "revision";
pub const GIT_TREE: &str = "tree";

/// The record payload a local path carries: the path and the content
/// identity of what was found there when it was read.
pub const PATH_PATH: &str = "path";
pub const PATH_CONTENT: &str = "content";

/// The declared source-binding sum type: `Archive(Blob)`, `Git({revision:
/// Text, tree: Text})`, `Path({path: Text, content: Blob})`.
#[must_use]
pub fn source_type() -> Type {
    let git = Type::record([
        RecordField {
            name: GIT_REVISION.into(),
            payload: Type::Text,
        },
        RecordField {
            name: GIT_TREE.into(),
            payload: Type::Text,
        },
    ]);
    let path = Type::record([
        RecordField {
            name: PATH_PATH.into(),
            payload: Type::Text,
        },
        RecordField {
            name: PATH_CONTENT.into(),
            payload: Type::Blob,
        },
    ]);
    // The field and constructor names are distinct literals, so the
    // duplicate-name rejections are unreachable by construction.
    let sum = Type::sum(
        SOURCE,
        [
            SumConstructor {
                name: ARCHIVE.into(),
                payload: Some(Type::Blob),
            },
            SumConstructor {
                name: GIT.into(),
                payload: Some(git.unwrap_or_else(|error| unreachable!("{error}"))),
            },
            SumConstructor {
                name: PATH.into(),
                payload: Some(path.unwrap_or_else(|error| unreachable!("{error}"))),
            },
        ],
    );
    sum.unwrap_or_else(|error| unreachable!("{error}"))
}

/// One source binding, as declared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceBinding {
    /// A registry archive, identified by its content.
    Archive { archive: ContentId },
    /// A git revision and the tree hash it resolved to.
    Git { revision: Box<str>, tree: Box<str> },
    /// A local path and the content identity of what was read there.
    Path { path: Box<str>, content: ContentId },
}

impl SourceBinding {
    /// The binding as a value of the declared sum.
    #[must_use]
    pub fn to_value(&self) -> Value {
        match self {
            Self::Archive { archive } => Value::Sum {
                type_name: SOURCE.into(),
                constructor: ARCHIVE.into(),
                payload: Some(Box::new(Value::Blob(*archive))),
            },
            Self::Git { revision, tree } => Value::Sum {
                type_name: SOURCE.into(),
                constructor: GIT.into(),
                payload: Some(Box::new(git_value(revision, tree))),
            },
            Self::Path { path, content } => Value::Sum {
                type_name: SOURCE.into(),
                constructor: PATH.into(),
                payload: Some(Box::new(path_value(path, *content))),
            },
        }
    }

    /// Read a binding from a value, accepting every declared sum of this
    /// name that contains the selected constructor with a matching payload —
    /// `is_type`, not `value_type`, because a sum value cannot recover its
    /// sibling constructors (0026's asymmetry).
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming what was expected and what was
    /// found when the value is not a source binding.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        if !value.is_type(&source_type()) {
            return Err(diag(format!(
                "expected a value of {SOURCE}, found {}",
                value.describe()
            )));
        }
        let Value::Sum {
            constructor,
            payload,
            ..
        } = value
        else {
            return Err(diag(format!(
                "expected a value of {SOURCE}, found {}",
                value.describe()
            )));
        };
        match (constructor.as_ref(), payload.as_deref()) {
            (ARCHIVE, Some(Value::Blob(archive))) => Ok(Self::Archive { archive: *archive }),
            (GIT, Some(record)) => {
                let (revision, tree) = git_fields(record)?;
                Ok(Self::Git { revision, tree })
            }
            (PATH, Some(record)) => {
                let (path, content) = path_fields(record)?;
                Ok(Self::Path { path, content })
            }
            _ => Err(diag(format!(
                "the {constructor} constructor of {SOURCE} carried no payload"
            ))),
        }
    }
}

fn git_value(revision: &str, tree: &str) -> Value {
    match Value::record([
        RecordField {
            name: GIT_REVISION.into(),
            payload: Value::Text(revision.into()),
        },
        RecordField {
            name: GIT_TREE.into(),
            payload: Value::Text(tree.into()),
        },
    ]) {
        Ok(value) => value,
        Err(error) => unreachable!("{error}"),
    }
}

fn path_value(path: &str, content: ContentId) -> Value {
    match Value::record([
        RecordField {
            name: PATH_PATH.into(),
            payload: Value::Text(path.into()),
        },
        RecordField {
            name: PATH_CONTENT.into(),
            payload: Value::Blob(content),
        },
    ]) {
        Ok(value) => value,
        Err(error) => unreachable!("{error}"),
    }
}

/// Extract `(revision, tree)` from a git payload record that `is_type` has
/// already accepted against the declared payload type.
fn git_fields(record: &Value) -> PithResult<(Box<str>, Box<str>)> {
    let fields = match record {
        Value::Record(fields) => fields,
        other => {
            return Err(diag(format!(
                "the {GIT} constructor carried {other:?} rather than a record"
            )));
        }
    };
    let mut revision = None;
    let mut tree = None;
    for field in fields.iter() {
        match field.name.as_ref() {
            GIT_REVISION => revision = Some(&field.payload),
            GIT_TREE => tree = Some(&field.payload),
            _ => {}
        }
    }
    match (revision, tree) {
        (Some(Value::Text(revision)), Some(Value::Text(tree))) => {
            Ok((revision.clone(), tree.clone()))
        }
        _ => Err(diag(format!(
            "the {GIT} payload did not carry {GIT_REVISION} and {GIT_TREE} texts"
        ))),
    }
}

fn path_fields(record: &Value) -> PithResult<(Box<str>, ContentId)> {
    let fields = match record {
        Value::Record(fields) => fields,
        other => {
            return Err(diag(format!(
                "the {PATH} constructor carried {other:?} rather than a record"
            )));
        }
    };
    let mut path = None;
    let mut content = None;
    for field in fields.iter() {
        match field.name.as_ref() {
            PATH_PATH => path = Some(&field.payload),
            PATH_CONTENT => content = Some(&field.payload),
            _ => {}
        }
    }
    match (path, content) {
        (Some(Value::Text(path)), Some(Value::Blob(content))) => Ok((path.clone(), *content)),
        _ => Err(diag(format!(
            "the {PATH} payload did not carry {PATH_PATH} text and {PATH_CONTENT} blob"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive() -> SourceBinding {
        SourceBinding::Archive {
            archive: ContentId::of_blob(b"tarball"),
        }
    }

    fn git() -> SourceBinding {
        SourceBinding::Git {
            revision: "9f11b1d".into(),
            tree: "e3b0c44".into(),
        }
    }

    fn path() -> SourceBinding {
        SourceBinding::Path {
            path: "vendor/zlib".into(),
            content: ContentId::of_blob(b"zlib-source"),
        }
    }

    #[test]
    fn every_binding_round_trips_through_its_value() {
        for binding in [archive(), git(), path()] {
            let value = binding.to_value();
            assert!(
                value.is_type(&source_type()),
                "{} should inhabit the declared sum",
                value.describe()
            );
            assert_eq!(SourceBinding::from_value(&value).unwrap(), binding);
        }
    }

    #[test]
    fn a_value_of_the_wrong_sum_is_refused() {
        let wrong = Value::Sum {
            type_name: "elsewhere.Source".into(),
            constructor: ARCHIVE.into(),
            payload: Some(Box::new(Value::Blob(ContentId::of_blob(b"tarball")))),
        };
        let error = SourceBinding::from_value(&wrong);
        assert!(
            error.is_err(),
            "a foreign sum of the same constructor set is not this sum"
        );
    }

    #[test]
    fn a_payload_of_the_wrong_type_is_refused() {
        let wrong = Value::Sum {
            type_name: SOURCE.into(),
            constructor: ARCHIVE.into(),
            payload: Some(Box::new(Value::Text("not a digest".into()))),
        };
        let error = SourceBinding::from_value(&wrong);
        assert!(
            error.is_err(),
            "the archive constructor carries a blob, not text"
        );
    }
}
