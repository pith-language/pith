use pith_core::{
    Coordinate, Int, Interface, NominalType, RecordField, SumConstructor, SumType, Type, Value,
};
use pith_ids::{ContentId, ModuleAbiDigest};

use crate::RuleCategory;

pub const FRONTEND_MODULE: &str = "frontend";

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
        }
    }
}

impl std::error::Error for FrontendInputError {}

impl FrontendSource {
    /// # Errors
    /// Returns [`FrontendInputError::DuplicateSourcePath`] for repeated paths.
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
    /// Returns [`FrontendInputError::DuplicateImportBinding`] for repeated bindings.
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValueDiagnostic {
    pub code: u32,
    pub message: Box<str>,
    pub source: ContentId,
    pub start: u32,
    pub end: u32,
}

fn record_type<const N: usize>(fields: [(&str, Type); N]) -> Type {
    let record = Type::record(fields.map(|(name, payload)| RecordField {
        name: name.into(),
        payload,
    }));
    record.unwrap_or_else(|error| unreachable!("{error}"))
}

fn record_value<const N: usize>(fields: [(&str, Value); N]) -> Value {
    let record = Value::record(fields.map(|(name, payload)| RecordField {
        name: name.into(),
        payload,
    }));
    record.unwrap_or_else(|error| unreachable!("{error}"))
}

fn frontend_nominal(name: &'static str, representation: Type) -> Type {
    Type::Nominal(Box::new(NominalType {
        coordinate: Coordinate::new(FRONTEND_MODULE, name),
        representation,
    }))
}

pub(crate) fn diagnostic_type() -> Type {
    record_type([
        ("code", Type::Int),
        ("message", Type::Text),
        ("source", Type::Blob),
        ("start", Type::Int),
        ("end", Type::Int),
    ])
}

pub(crate) fn diagnostic_value(diagnostic: &ValueDiagnostic) -> Value {
    record_value([
        ("code", Value::int(Int::from(diagnostic.code))),
        ("message", Value::Text(diagnostic.message.clone())),
        ("source", Value::Blob(diagnostic.source)),
        ("start", Value::int(Int::from(diagnostic.start))),
        ("end", Value::int(Int::from(diagnostic.end))),
    ])
}

fn tier_type() -> Type {
    Type::Sum(Box::new(SumType {
        coordinate: Coordinate::new(FRONTEND_MODULE, "Tier"),
        constructors: [
            SumConstructor {
                name: "Host".into(),
                payload: None,
            },
            SumConstructor {
                name: "Represented".into(),
                payload: None,
            },
        ]
        .into(),
    }))
}

fn host_tier_value() -> Value {
    Value::Sum {
        type_name: format!("{FRONTEND_MODULE}.Tier").into(),
        constructor: "Host".into(),
        payload: None,
    }
}

fn abi_digest_type() -> Type {
    frontend_nominal("ModuleAbiDigest", Type::Blob)
}

fn abi_digest_value(abi: ModuleAbiDigest) -> Value {
    Value::Nominal {
        name: format!("{FRONTEND_MODULE}.ModuleAbiDigest").into(),
        representation: Box::new(Value::Blob(ContentId::from_digest(abi.digest()))),
    }
}

fn rule_category_type() -> Type {
    Type::Sum(Box::new(SumType {
        coordinate: Coordinate::new(FRONTEND_MODULE, "RuleCategory"),
        constructors: [
            SumConstructor {
                name: "Action".into(),
                payload: None,
            },
            SumConstructor {
                name: "Pure".into(),
                payload: None,
            },
        ]
        .into(),
    }))
}

fn rule_category_value(category: RuleCategory) -> Value {
    Value::Sum {
        type_name: format!("{FRONTEND_MODULE}.RuleCategory").into(),
        constructor: match category {
            RuleCategory::Pure => "Pure",
            RuleCategory::Action => "Action",
        }
        .into(),
        payload: None,
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

pub(crate) fn module_interface_type() -> Type {
    frontend_nominal(
        "ModuleInterface",
        record_type([
            ("identity", Type::Text),
            ("tier", tier_type()),
            ("abi", abi_digest_type()),
            ("surface", Type::Bytes),
            ("diagnostics", Type::List(Box::new(diagnostic_type()))),
        ]),
    )
}

pub(crate) fn module_interface_value(
    module: &str,
    abi: ModuleAbiDigest,
    surface: &super::InterfaceSurface,
    diagnostics: &[ValueDiagnostic],
) -> Value {
    Value::Nominal {
        name: format!("{FRONTEND_MODULE}.ModuleInterface").into(),
        representation: Box::new(record_value([
            ("identity", Value::Text(module.into())),
            ("tier", host_tier_value()),
            ("abi", abi_digest_value(abi)),
            ("surface", Value::Bytes(surface.encode().into())),
            (
                "diagnostics",
                Value::List(diagnostics.iter().map(diagnostic_value).collect()),
            ),
        ])),
    }
}

fn rule_binding_type() -> Type {
    record_type([
        ("module", Type::Text),
        ("name", Type::Text),
        ("category", rule_category_type()),
        ("interface", Type::Bytes),
    ])
}

pub(crate) fn bodies_type() -> Type {
    frontend_nominal(
        "Bodies",
        record_type([
            ("rules", Type::List(Box::new(rule_binding_type()))),
            (
                "incomplete",
                Type::List(Box::new(record_type([
                    ("module", Type::Text),
                    ("name", Type::Text),
                    ("diagnostics", Type::List(Box::new(diagnostic_type()))),
                ]))),
            ),
            ("diagnostics", Type::List(Box::new(diagnostic_type()))),
        ]),
    )
}

pub(crate) struct ValueRuleBinding {
    pub module: Box<str>,
    pub name: Box<str>,
    pub category: RuleCategory,
    pub interface: Interface,
}

pub(crate) struct ValueIncompleteRule {
    pub module: Box<str>,
    pub name: Box<str>,
    pub diagnostics: Box<[ValueDiagnostic]>,
}

pub(crate) fn bodies_value(
    rules: &[ValueRuleBinding],
    incomplete_rules: &[ValueIncompleteRule],
    diagnostics: &[ValueDiagnostic],
) -> Value {
    Value::Nominal {
        name: format!("{FRONTEND_MODULE}.Bodies").into(),
        representation: Box::new(record_value([
            (
                "rules",
                Value::List(
                    rules
                        .iter()
                        .map(|rule| {
                            record_value([
                                ("module", Value::Text(rule.module.clone())),
                                ("name", Value::Text(rule.name.clone())),
                                ("category", rule_category_value(rule.category)),
                                (
                                    "interface",
                                    Value::Bytes(rule.interface.encode_canonical().into()),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "incomplete",
                Value::List(
                    incomplete_rules
                        .iter()
                        .map(|rule| {
                            record_value([
                                ("module", Value::Text(rule.module.clone())),
                                ("name", Value::Text(rule.name.clone())),
                                (
                                    "diagnostics",
                                    Value::List(
                                        rule.diagnostics.iter().map(diagnostic_value).collect(),
                                    ),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "diagnostics",
                Value::List(diagnostics.iter().map(diagnostic_value).collect()),
            ),
        ])),
    }
}

pub(crate) fn index_type() -> Type {
    frontend_nominal(
        "Index",
        record_type([
            (
                "entries",
                Type::List(Box::new(record_type([
                    ("category", rule_category_type()),
                    ("interface", Type::Bytes),
                    ("module", Type::Text),
                    ("name", Type::Text),
                ]))),
            ),
            ("diagnostics", Type::List(Box::new(diagnostic_type()))),
        ]),
    )
}

pub(crate) fn index_value(
    entries: &[(RuleCategory, Interface, Box<str>, Box<str>)],
    diagnostics: &[ValueDiagnostic],
) -> Value {
    Value::Nominal {
        name: format!("{FRONTEND_MODULE}.Index").into(),
        representation: Box::new(record_value([
            (
                "entries",
                Value::List(
                    entries
                        .iter()
                        .map(|(category, interface, module, name)| {
                            record_value([
                                ("category", rule_category_value(*category)),
                                (
                                    "interface",
                                    Value::Bytes(interface.encode_canonical().into()),
                                ),
                                ("module", Value::Text(module.clone())),
                                ("name", Value::Text(name.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "diagnostics",
                Value::List(diagnostics.iter().map(diagnostic_value).collect()),
            ),
        ])),
    }
}

fn field<'a>(fields: &'a [RecordField<Value>], name: &str) -> &'a Value {
    fields
        .iter()
        .find(|field| field.name.as_ref() == name)
        .map_or(&Value::Unit, |field| &field.payload)
}

fn text_field<'a>(fields: &'a [RecordField<Value>], name: &str) -> &'a str {
    match field(fields, name) {
        Value::Text(text) => text,
        _ => unreachable!("the engine validated the input against the source type"),
    }
}

fn blob_field(fields: &[RecordField<Value>], name: &str) -> ContentId {
    match field(fields, name) {
        Value::Blob(id) => *id,
        _ => unreachable!("the engine validated the input against the source type"),
    }
}

fn abi_digest_field(fields: &[RecordField<Value>], name: &str) -> ModuleAbiDigest {
    let Value::Nominal { representation, .. } = field(fields, name) else {
        unreachable!("the engine validated the ABI digest nominal");
    };
    let Value::Blob(abi) = representation.as_ref() else {
        unreachable!("the engine validated the ABI digest representation");
    };
    ModuleAbiDigest::from_digest(abi.digest())
}
