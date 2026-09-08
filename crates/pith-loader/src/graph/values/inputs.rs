//! The input codecs: a module's source set and its import environment, as
//! the graph rules are asked about them.

use pith_core::{Type, Value};
use pith_ids::{ContentId, ModuleAbiDigest};

use super::scalars::{abi_digest_type, abi_digest_value};
use super::{abi_digest_field, blob_field, field, record_type, record_value, text_field};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceEntry {
    pub(crate) path: Box<str>,
    pub(crate) content: ContentId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontendSource {
    pub(crate) module: Box<str>,
    pub(crate) files: Box<[SourceEntry]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontendImport {
    pub(crate) binding: Box<str>,
    pub(crate) module: Box<str>,
    pub(crate) abi: ModuleAbiDigest,
    pub(crate) surface: ContentId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrontendImportEnv {
    pub(crate) entries: Box<[FrontendImport]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrontendInputError {
    DuplicateSourcePath { path: Box<str> },
    DuplicateImportBinding { binding: Box<str> },
    ConflictingImportAbis { subject: Box<str> },
}

impl std::fmt::Display for FrontendInputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateSourcePath { path } => {
                write!(formatter, "source path `{path}` appears more than once")
            }
            Self::DuplicateImportBinding { binding } => {
                write!(
                    formatter,
                    "import binding `{binding}` appears more than once"
                )
            }
            Self::ConflictingImportAbis { subject } => {
                write!(
                    formatter,
                    "the subject {subject} is imported with two different ABI digests"
                )
            }
        }
    }
}

impl std::error::Error for FrontendInputError {}

impl FrontendSource {
    /// # Errors
    /// [`FrontendInputError::DuplicateSourcePath`] for repeated paths.
    pub fn new(
        module: impl Into<Box<str>>,
        files: impl IntoIterator<Item = (Box<str>, ContentId)>,
    ) -> Result<Self, FrontendInputError> {
        let mut files = files
            .into_iter()
            .map(|(path, content)| SourceEntry { path, content })
            .collect::<Vec<_>>();
        files.sort_by(|left, right| left.path.cmp(&right.path));
        for [earlier, later] in files.array_windows() {
            if earlier.path == later.path {
                return Err(FrontendInputError::DuplicateSourcePath {
                    path: earlier.path.clone(),
                });
            }
        }
        Ok(Self {
            module: module.into(),
            files: files.into(),
        })
    }

    pub(crate) fn into_value(self) -> Value {
        record_value([
            ("module", Value::Text(self.module)),
            (
                "files",
                Value::List(
                    self.files
                        .into_iter()
                        .map(|file| {
                            record_value([
                                ("path", Value::Text(file.path)),
                                ("content", Value::Blob(file.content)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }
}

impl FrontendImport {
    #[must_use]
    pub fn new(
        binding: impl Into<Box<str>>,
        module: impl Into<Box<str>>,
        abi: ModuleAbiDigest,
        surface: ContentId,
    ) -> Self {
        Self {
            binding: binding.into(),
            module: module.into(),
            abi,
            surface,
        }
    }
}

impl FrontendImportEnv {
    /// # Errors
    /// [`FrontendInputError`] for repeated bindings, or when one subject
    /// arrives with two different ABI digests: the environment binds a
    /// subject to one ABI, whichever bindings reach it.
    pub fn new(
        entries: impl IntoIterator<Item = FrontendImport>,
    ) -> Result<Self, FrontendInputError> {
        let mut entries = entries.into_iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.binding.cmp(&right.binding));
        for [earlier, later] in entries.array_windows() {
            if earlier.binding == later.binding {
                return Err(FrontendInputError::DuplicateImportBinding {
                    binding: earlier.binding.clone(),
                });
            }
        }
        let mut by_subject = entries.clone();
        by_subject.sort_by(|left, right| left.module.cmp(&right.module));
        for [earlier, later] in by_subject.array_windows() {
            if earlier.module == later.module && earlier.abi != later.abi {
                return Err(FrontendInputError::ConflictingImportAbis {
                    subject: earlier.module.clone(),
                });
            }
        }
        Ok(Self {
            entries: entries.into(),
        })
    }

    pub(crate) fn into_value(self) -> Value {
        Value::List(
            self.entries
                .into_iter()
                .map(|entry| {
                    record_value([
                        ("binding", Value::Text(entry.binding)),
                        ("module", Value::Text(entry.module)),
                        ("abi", abi_digest_value(entry.abi)),
                        ("surface", Value::Blob(entry.surface)),
                    ])
                })
                .collect(),
        )
    }
}

pub(crate) fn source_type() -> Type {
    record_type([
        ("module", Type::Text),
        (
            "files",
            Type::List(Box::new(record_type([
                ("path", Type::Text),
                ("content", Type::Blob),
            ]))),
        ),
    ])
}

pub(crate) fn import_env_type() -> Type {
    Type::List(Box::new(record_type([
        ("binding", Type::Text),
        ("module", Type::Text),
        ("abi", abi_digest_type()),
        ("surface", Type::Blob),
    ])))
}

pub(crate) fn read_source(value: &Value) -> FrontendSource {
    let Value::Record(fields) = value else {
        unreachable!("the engine validated the input against the source type");
    };
    let module = text_field(fields, "module");
    let Value::List(files) = field(fields, "files") else {
        unreachable!("the engine validated the input against the source type");
    };
    FrontendSource {
        module: module.into(),
        files: files
            .iter()
            .map(|file| {
                let Value::Record(fields) = file else {
                    unreachable!("the engine validated the input against the source type");
                };
                SourceEntry {
                    path: text_field(fields, "path").into(),
                    content: blob_field(fields, "content"),
                }
            })
            .collect(),
    }
}

pub(crate) fn read_import_env(value: &Value) -> FrontendImportEnv {
    let Value::List(entries) = value else {
        unreachable!("the engine validated the input against the import environment type");
    };
    FrontendImportEnv {
        entries: entries
            .iter()
            .map(|entry| {
                let Value::Record(fields) = entry else {
                    unreachable!(
                        "the engine validated the input against the import environment type"
                    );
                };
                FrontendImport {
                    binding: text_field(fields, "binding").into(),
                    module: text_field(fields, "module").into(),
                    abi: abi_digest_field(fields, "abi"),
                    surface: blob_field(fields, "surface"),
                }
            })
            .collect(),
    }
}
