use pith_core::{Type, Value};
use pith_diag::PithResult;
use pith_ids::ContentId;

use crate::codec::{blob_field, field_of, record_type, record_value, text_field, text_list};
use crate::diag;

const PATH: &str = "path";
const CONTENT: &str = "content";
const SOURCES: &str = "sources";

/// Measured source files in path order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceTree {
    pub files: Box<[SourceFile]>,
}

/// A source path and its measured content identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    pub path: Box<str>,
    pub content: ContentId,
}

impl SourceTree {
    /// Returns the content identity at `path`.
    #[must_use]
    pub fn content_at(&self, path: &str) -> Option<ContentId> {
        self.files
            .binary_search_by(|file| file.path.as_ref().cmp(path))
            .ok()
            .and_then(|index| self.files.get(index).map(|file| file.content))
    }

    /// Encodes the tree as a path-sorted list of records.
    #[must_use]
    pub fn to_value(&self) -> Value {
        Value::List(
            self.files
                .iter()
                .map(|file| {
                    record_value([
                        (PATH, Value::Text(file.path.clone())),
                        (CONTENT, Value::Blob(file.content)),
                    ])
                })
                .collect(),
        )
    }
}

/// The type of a source tree: a list of path-and-content records.
#[must_use]
pub fn tree_type() -> Type {
    Type::List(Box::new(record_type([
        (PATH, Type::Text),
        (CONTENT, Type::Blob),
    ])))
}

/// Source paths to compile, in link order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageBuild {
    pub sources: Box<[Box<str>]>,
}

impl PackageBuild {
    /// Encodes the build as its declared record type.
    #[must_use]
    pub fn to_value(&self) -> Value {
        record_value([(
            SOURCES,
            Value::List(
                self.sources
                    .iter()
                    .map(|source| Value::Text(source.clone()))
                    .collect(),
            ),
        )])
    }

    /// Decodes a package build from `value`.
    ///
    /// # Errors
    /// Returns a diagnostic when `value` is not a package build.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        let build = record_type([(SOURCES, Type::List(Box::new(Type::Text)))]);
        if !value.is_type(&build) {
            return Err(diag(format!(
                "expected a value of the package build type, found {}",
                value.describe()
            )));
        }
        let sources = match value {
            Value::Record(fields) => match field_of(fields, SOURCES) {
                Some(payload) => text_list(payload, SOURCES)?,
                None => return Err(diag(format!("the record carried no {SOURCES} list"))),
            },
            _ => unreachable!("is_type accepted a non-record"),
        };
        Ok(Self {
            sources: sources.into(),
        })
    }
}

/// The type of a declared package build.
#[must_use]
pub fn build_type() -> Type {
    record_type([(SOURCES, Type::List(Box::new(Type::Text)))])
}

/// Decodes a source tree without changing its file order.
pub(super) fn tree_from_value(value: &Value) -> PithResult<SourceTree> {
    if !value.is_type(&tree_type()) {
        return Err(diag(format!(
            "expected a value of the source tree type, found {}",
            value.describe()
        )));
    }
    let Value::List(files) = value else {
        return Err(diag("the source tree was not a list of files"));
    };
    let mut imported = Vec::with_capacity(files.len());
    for file in files.iter() {
        let Value::Record(fields) = file else {
            return Err(diag(format!(
                "a tree entry was {}, not a path-and-content record",
                file.describe()
            )));
        };
        let path = text_field(fields, PATH)?;
        let content = blob_field(fields, CONTENT)?;
        imported.push(SourceFile { path, content });
    }
    Ok(SourceTree {
        files: imported.into(),
    })
}
