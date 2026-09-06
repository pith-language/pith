//! The frontend tier's value layer: the canonical types a graph request
//! names, and the codecs between them and the tier's own vocabulary.
//!
//! The record and nominal construction helpers live here once; the codec
//! modules below share them so one field-name spelling cannot drift between
//! a type and its codec.

mod answers;
mod inputs;
mod scalars;

pub(crate) use answers::{
    ValueIncompleteRule, ValueRuleBinding, bodies_type, bodies_value, index_type, index_value,
    module_interface_type, module_interface_value, read_interface_surface,
};
pub use inputs::{FrontendImport, FrontendImportEnv, FrontendInputError, FrontendSource};
pub(crate) use inputs::{SourceEntry, import_env_type, read_import_env, read_source, source_type};
pub(crate) use scalars::ValueDiagnostic;

use pith_core::{Coordinate, NominalType, RecordField, Type, Value};
use pith_ids::{ContentId, ModuleAbiDigest};

/// The module coordinate every frontend type is named under.
pub const FRONTEND_MODULE: &str = "frontend";

pub(super) fn record_type<const N: usize>(fields: [(&str, Type); N]) -> Type {
    let record = Type::record(fields.map(|(name, payload)| RecordField {
        name: name.into(),
        payload,
    }));
    record.unwrap_or_else(|error| unreachable!("{error}"))
}

pub(super) fn record_value<const N: usize>(fields: [(&str, Value); N]) -> Value {
    let record = Value::record(fields.map(|(name, payload)| RecordField {
        name: name.into(),
        payload,
    }));
    record.unwrap_or_else(|error| unreachable!("{error}"))
}

pub(super) fn frontend_nominal(name: &'static str, representation: Type) -> Type {
    Type::Nominal(Box::new(NominalType {
        coordinate: Coordinate::new(FRONTEND_MODULE, name),
        representation,
    }))
}

/// The field a record carries, or `Unit` when it carries none.
pub(super) fn field<'a>(fields: &'a [RecordField<Value>], name: &str) -> &'a Value {
    fields
        .iter()
        .find(|field| field.name.as_ref() == name)
        .map_or(&Value::Unit, |field| &field.payload)
}

pub(super) fn text_field<'a>(fields: &'a [RecordField<Value>], name: &str) -> &'a str {
    match field(fields, name) {
        Value::Text(text) => text,
        _ => unreachable!("the engine validated the input against the source type"),
    }
}

pub(super) fn blob_field(fields: &[RecordField<Value>], name: &str) -> ContentId {
    match field(fields, name) {
        Value::Blob(id) => *id,
        _ => unreachable!("the engine validated the input against the source type"),
    }
}

pub(super) fn abi_digest_field(fields: &[RecordField<Value>], name: &str) -> ModuleAbiDigest {
    let Value::Nominal { representation, .. } = field(fields, name) else {
        unreachable!("the engine validated the ABI digest nominal");
    };
    let Value::Blob(abi) = representation.as_ref() else {
        unreachable!("the engine validated the ABI digest representation");
    };
    ModuleAbiDigest::from_digest(abi.digest())
}
