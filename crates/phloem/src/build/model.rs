use pith_core::{Type, Value};
use pith_diag::PithResult;
use pith_ids::ContentId;

use crate::codec::{blob_field, field_of, record_type, record_value, text_field, text_list};
use crate::diag;

const PATH: &str = "path";
const CONTENT: &str = "content";
const SOURCES: &str = "sources";

/// The tree one archive unpacked into: the measured files, sorted by path
/// so the tree a build runs over is a function of the archive and not of
/// the order the reader happened to hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceTree {
    pub files: Box<[SourceFile]>,
}

/// One file of an unpacked tree: its path within the tree and the content
/// identity measured from its bytes when it was imported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    pub path: Box<str>,
    pub content: ContentId,
}

impl SourceTree {
    /// The content identity at `path`, or `None` when the tree does not
    /// hold it. Binary search over the sorted files.
    #[must_use]
    pub fn content_at(&self, path: &str) -> Option<ContentId> {
        self.files
            .binary_search_by(|file| file.path.as_ref().cmp(path))
            .ok()
            .and_then(|index| self.files.get(index).map(|file| file.content))
    }

    /// The tree as a value: `List<{path: Text, content: Blob}>`, sorted by
    /// path.
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

/// What a package declares as its build: the sources that compile, in link
/// order, over the tree its archive unpacked into. The procedure around
/// them — compile each through xylem's entry, link the objects through
/// xylem's link entry — is the library's, on the stdenv ground that a
/// fixed, inspectable procedure beats a script per package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageBuild {
    pub sources: Box<[Box<str>]>,
}

impl PackageBuild {
    /// The build as a value of the declared record type.
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

    /// Read a build from a value.
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming the declared type and the
    /// value found when the value is not a package build.
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

/// Read a tree from its value, in the sorted order the value spells.
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
