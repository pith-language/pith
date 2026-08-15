use pith_core::{Type, Value};
use pith_diag::PithResult;
use pith_ids::ContentId;

use crate::codec::{blob_field, field_of, record_type, record_value, text_field, text_list};
use crate::diag;

/// One offered header: the include spelling and the content it names, which
/// is also the staged path.
pub type HeaderPair = (Box<str>, ContentId);

/// A canonically sorted set of offered headers.
pub type HeaderSet = Box<[HeaderPair]>;

const PATH: &str = "path";
const CONTENT: &str = "content";
const SOURCES: &str = "sources";
const INCLUDES: &str = "includes";
const OBJECTS: &str = "objects";
const HEADERS: &str = "headers";
const TREE: &str = "tree";
const BUILD: &str = "build";

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

/// Source paths to compile, in link order, and header paths the package
/// offers — to its own sources and to whatever depends on it — where the
/// tree path is also the include spelling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageBuild {
    pub sources: Box<[Box<str>]>,
    pub includes: Box<[Box<str>]>,
}

impl PackageBuild {
    /// Encodes the build as its declared record type.
    #[must_use]
    pub fn to_value(&self) -> Value {
        record_value([
            (
                SOURCES,
                Value::List(
                    self.sources
                        .iter()
                        .map(|source| Value::Text(source.clone()))
                        .collect(),
                ),
            ),
            (
                INCLUDES,
                Value::List(
                    self.includes
                        .iter()
                        .map(|include| Value::Text(include.clone()))
                        .collect(),
                ),
            ),
        ])
    }

    /// Decodes a package build from `value`.
    ///
    /// # Errors
    /// Returns a diagnostic when `value` is not a package build.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        if !value.is_type(&build_type()) {
            return Err(diag(format!(
                "expected a value of the package build type, found {}",
                value.describe()
            )));
        }
        let Value::Record(fields) = value else {
            return Err(diag(format!(
                "expected a value of the package build type, found {}",
                value.describe()
            )));
        };
        let sources = match field_of(fields, SOURCES) {
            Some(payload) => text_list(payload, SOURCES)?,
            None => return Err(diag(format!("the record carried no {SOURCES} list"))),
        };
        let includes = match field_of(fields, INCLUDES) {
            Some(payload) => text_list(payload, INCLUDES)?,
            None => return Err(diag(format!("the record carried no {INCLUDES} list"))),
        };
        Ok(Self {
            sources: sources.into(),
            includes: includes.into(),
        })
    }
}

/// The type of a declared package build.
#[must_use]
pub fn build_type() -> Type {
    record_type([
        (SOURCES, Type::List(Box::new(Type::Text))),
        (INCLUDES, Type::List(Box::new(Type::Text))),
    ])
}

/// A library package's artifact: the objects a dependent links, and the
/// headers its compiles see. The headers ride beside the objects so a
/// dependency is one value — what a dependent needs of a package is both
/// halves, and a package that published objects without its headers would be
/// unlinkable from source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Library {
    /// Object content identities in link order.
    pub objects: Box<[ContentId]>,
    /// Include spellings with their content, canonically sorted by path.
    pub headers: HeaderSet,
}

/// The declared library type: `{objects: List<xylem.Object>, headers: List<
/// {path, content}>}`, over the same header shape a compile request provides.
#[must_use]
pub fn library_type() -> Type {
    record_type([
        (
            OBJECTS,
            Type::List(Box::new(Type::Nominal {
                name: xylem::types::OBJECT.into(),
            })),
        ),
        (HEADERS, xylem::types::provided_headers_type()),
    ])
}

impl Library {
    /// Encodes the library as its declared record type.
    #[must_use]
    pub fn to_value(&self) -> Value {
        record_value([
            (
                OBJECTS,
                Value::List(
                    self.objects
                        .iter()
                        .map(|object| xylem::types::object(*object))
                        .collect(),
                ),
            ),
            (
                HEADERS,
                xylem::types::provided_headers(
                    self.headers
                        .iter()
                        .map(|(path, content)| (path.clone(), *content)),
                ),
            ),
        ])
    }

    /// Decodes a library from `value`.
    ///
    /// # Errors
    /// Returns a diagnostic when `value` is not a library record.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        if !value.is_type(&library_type()) {
            return Err(diag(format!(
                "expected a value of the library type, found {}",
                value.describe()
            )));
        }
        let Value::Record(fields) = value else {
            return Err(diag(format!(
                "expected a value of the library type, found {}",
                value.describe()
            )));
        };
        let objects = match field_of(fields, OBJECTS) {
            Some(Value::List(entries)) => entries
                .iter()
                .map(object_of)
                .collect::<PithResult<Vec<_>>>()?,
            _ => return Err(diag(format!("the record carried no {OBJECTS} list"))),
        };
        let headers = match field_of(fields, HEADERS) {
            Some(Value::List(entries)) => entries
                .iter()
                .map(header_of)
                .collect::<PithResult<Vec<_>>>()?,
            _ => return Err(diag(format!("the record carried no {HEADERS} list"))),
        };
        Ok(Self {
            objects: objects.into(),
            headers: headers.into(),
        })
    }
}

fn object_of(entry: &Value) -> PithResult<ContentId> {
    match entry {
        Value::Nominal {
            name,
            representation,
        } if name.as_ref() == xylem::types::OBJECT => match representation.as_ref() {
            Value::Blob(id) => Ok(*id),
            other => Err(diag(format!(
                "an object value carried {other:?} rather than a blob"
            ))),
        },
        other => Err(diag(format!(
            "expected a {} value in the objects list, found {}",
            xylem::types::OBJECT,
            other.describe()
        ))),
    }
}

fn header_of(entry: &Value) -> PithResult<(Box<str>, ContentId)> {
    let Value::Record(fields) = entry else {
        return Err(diag(format!(
            "a library header was {}, not a path-and-content record",
            entry.describe()
        )));
    };
    let path = text_field(fields, xylem::types::HEADER_PATH)?;
    let content = blob_field(fields, xylem::types::HEADER_CONTENT)?;
    Ok((path, content))
}

/// One dependency a build declares: the dependency's measured tree and its
/// build declaration, from which the dependency's library builds in-graph.
/// The pair rather than the built library is the input because the edge is
/// the graph's to order and cache — the dependent's request names what the
/// dependency is, not what a caller already built from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub tree: SourceTree,
    pub build: PackageBuild,
}

/// The declared dependency type: `{tree, build}`.
#[must_use]
pub fn dependency_type() -> Type {
    record_type([(TREE, tree_type()), (BUILD, build_type())])
}

impl Dependency {
    /// Encodes the dependency as its declared record type.
    #[must_use]
    pub fn to_value(&self) -> Value {
        record_value([(TREE, self.tree.to_value()), (BUILD, self.build.to_value())])
    }

    /// Decodes a dependency from `value`.
    ///
    /// # Errors
    /// Returns a diagnostic when `value` is not a dependency record.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        if !value.is_type(&dependency_type()) {
            return Err(diag(format!(
                "expected a value of the dependency type, found {}",
                value.describe()
            )));
        }
        let Value::Record(fields) = value else {
            return Err(diag(format!(
                "expected a value of the dependency type, found {}",
                value.describe()
            )));
        };
        let tree = match field_of(fields, TREE) {
            Some(payload) => tree_from_value(payload)?,
            None => return Err(diag(format!("the record carried no {TREE}"))),
        };
        let build = match field_of(fields, BUILD) {
            Some(payload) => PackageBuild::from_value(payload)?,
            None => return Err(diag(format!("the record carried no {BUILD}"))),
        };
        Ok(Self { tree, build })
    }
}

/// The declared dependency-list type, in link order.
#[must_use]
pub fn dependency_list_type() -> Type {
    Type::List(Box::new(dependency_type()))
}

/// A dependency list as one value, in the order the build wants the
/// dependencies' objects linked.
#[must_use]
pub fn dependency_list_value(dependencies: &[Dependency]) -> Value {
    Value::List(dependencies.iter().map(Dependency::to_value).collect())
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
