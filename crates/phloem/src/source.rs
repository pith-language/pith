//! Typed source bindings for package candidates.
//!
//! Bindings represent registry archives, unresolved Git references,
//! materialized Git trees, and local paths.

use pith_core::{SumConstructor, Type, Value};
use pith_diag::PithResult;
use pith_ids::ContentId;

use crate::codec::{field_of, record_type, record_value, sum_value};
use crate::diag;

/// The declared sum's name.
pub const SOURCE: &str = "phloem.Source";

const ARCHIVE: &str = "Archive";
const GIT: &str = "Git";
const MATERIALIZED_TREE: &str = "GitTree";
const PATH: &str = "Path";

/// The record payload a git reference carries: the revision and forge tree
/// identifier it resolved to. Neither is the source binding until a fetch
/// materializes the tree and measures its content.
pub const GIT_REVISION: &str = "revision";
pub const GIT_TREE: &str = "tree";

/// The record payload a materialized git tree carries: the revision and tree
/// hash of the reference, beside the content identity measured from the
/// bytes the fetch actually read.
pub const TREE_CONTENT: &str = "content";

/// The record payload a local path carries: the path and the content
/// identity of what was found there when it was read.
pub const PATH_PATH: &str = "path";
pub const PATH_CONTENT: &str = "content";

/// The declared source-binding sum type: `Archive(Blob)`,
/// `Git({revision: Text, tree: Text})`,
/// `GitTree({revision: Text, tree: Text, content: Blob})`,
/// `Path({path: Text, content: Blob})`.
#[must_use]
pub fn source_type() -> Type {
    let git = record_type([(GIT_REVISION, Type::Text), (GIT_TREE, Type::Text)]);
    let git_tree = record_type([
        (GIT_REVISION, Type::Text),
        (GIT_TREE, Type::Text),
        (TREE_CONTENT, Type::Blob),
    ]);
    let path = record_type([(PATH_PATH, Type::Text), (PATH_CONTENT, Type::Blob)]);
    // Field and constructor names are distinct literals.
    let sum = Type::sum(
        SOURCE,
        [
            SumConstructor {
                name: ARCHIVE.into(),
                payload: Some(Type::Blob),
            },
            SumConstructor {
                name: GIT.into(),
                payload: Some(git),
            },
            SumConstructor {
                name: MATERIALIZED_TREE.into(),
                payload: Some(git_tree),
            },
            SumConstructor {
                name: PATH.into(),
                payload: Some(path),
            },
        ],
    );
    sum.unwrap_or_else(|error| unreachable!("{error}"))
}

/// One source binding, as declared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceBinding {
    /// A registry archive identified by its content.
    Archive { archive: ContentId },
    /// A Git revision and tree hash not yet materialized.
    Git { revision: Box<str>, tree: Box<str> },
    /// A materialized Git tree and its measured content identity.
    GitTree {
        revision: Box<str>,
        tree: Box<str>,
        content: ContentId,
    },
    /// A local path and the content identity of what was read there.
    Path { path: Box<str>, content: ContentId },
}

impl SourceBinding {
    /// The binding as a value of the declared sum.
    #[must_use]
    pub fn to_value(&self) -> Value {
        match self {
            Self::Archive { archive } => sum_value(SOURCE, ARCHIVE, Some(Value::Blob(*archive))),
            Self::Git { revision, tree } => sum_value(
                SOURCE,
                GIT,
                Some(record_value([
                    (GIT_REVISION, Value::Text(revision.clone())),
                    (GIT_TREE, Value::Text(tree.clone())),
                ])),
            ),
            Self::GitTree {
                revision,
                tree,
                content,
            } => sum_value(
                SOURCE,
                MATERIALIZED_TREE,
                Some(record_value([
                    (GIT_REVISION, Value::Text(revision.clone())),
                    (GIT_TREE, Value::Text(tree.clone())),
                    (TREE_CONTENT, Value::Blob(*content)),
                ])),
            ),
            Self::Path { path, content } => sum_value(
                SOURCE,
                PATH,
                Some(record_value([
                    (PATH_PATH, Value::Text(path.clone())),
                    (PATH_CONTENT, Value::Blob(*content)),
                ])),
            ),
        }
    }

    /// Decodes a source binding from `value`.
    ///
    /// # Errors
    /// Returns a diagnostic when `value` is not a source binding.
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
            (MATERIALIZED_TREE, Some(record)) => {
                let (revision, tree, content) = git_tree_fields(record)?;
                Ok(Self::GitTree {
                    revision,
                    tree,
                    content,
                })
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

/// Extract `(revision, tree)` from a git payload record that `is_type` has
/// already accepted against the declared payload type.
fn git_fields(record: &Value) -> PithResult<(Box<str>, Box<str>)> {
    let Value::Record(fields) = record else {
        return Err(diag(format!(
            "the {GIT} constructor carried {record:?} rather than a record"
        )));
    };
    match (field_of(fields, GIT_REVISION), field_of(fields, GIT_TREE)) {
        (Some(Value::Text(revision)), Some(Value::Text(tree))) => {
            Ok((revision.clone(), tree.clone()))
        }
        _ => Err(diag(format!(
            "the {GIT} payload did not carry {GIT_REVISION} and {GIT_TREE} texts"
        ))),
    }
}

/// Extract `(revision, tree, content)` from a materialized-tree payload
/// record that `is_type` has already accepted.
fn git_tree_fields(record: &Value) -> PithResult<(Box<str>, Box<str>, ContentId)> {
    let Value::Record(fields) = record else {
        return Err(diag(format!(
            "the {MATERIALIZED_TREE} constructor carried {record:?} rather than a record"
        )));
    };
    match (
        field_of(fields, GIT_REVISION),
        field_of(fields, GIT_TREE),
        field_of(fields, TREE_CONTENT),
    ) {
        (Some(Value::Text(revision)), Some(Value::Text(tree)), Some(Value::Blob(content))) => {
            Ok((revision.clone(), tree.clone(), *content))
        }
        _ => Err(diag(format!(
            "the {MATERIALIZED_TREE} payload did not carry {GIT_REVISION} and {GIT_TREE} texts \
             and a {TREE_CONTENT} blob"
        ))),
    }
}

fn path_fields(record: &Value) -> PithResult<(Box<str>, ContentId)> {
    let Value::Record(fields) = record else {
        return Err(diag(format!(
            "the {PATH} constructor carried {record:?} rather than a record"
        )));
    };
    match (field_of(fields, PATH_PATH), field_of(fields, PATH_CONTENT)) {
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

    fn git_tree() -> SourceBinding {
        SourceBinding::GitTree {
            revision: "9f11b1d".into(),
            tree: "e3b0c44".into(),
            content: ContentId::of_blob(b"tree-archive"),
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
        for binding in [archive(), git(), git_tree(), path()] {
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
