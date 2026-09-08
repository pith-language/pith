//! The answer codecs: the interface surface, the bodies, and the index the
//! graph rules produce.

use pith_core::{Interface, Type, Value};
use pith_ids::ModuleAbiDigest;

use super::super::InterfaceSurface;
use super::scalars::{
    abi_digest_type, abi_digest_value, diagnostic_type, diagnostic_value, host_tier_value,
    rule_category_type, rule_category_value, tier_type,
};
use super::{FRONTEND_MODULE, ValueDiagnostic, frontend_nominal, record_type, record_value};
use crate::RuleCategory;

/// The field names of `frontend.ModuleInterface`, named once so its type,
/// its value, and the reader that takes a surface back out cannot drift
/// apart.
mod module_interface {
    pub(super) const IDENTITY: &str = "identity";
    pub(super) const TIER: &str = "tier";
    pub(super) const ABI: &str = "abi";
    pub(super) const SURFACE: &str = "surface";
    pub(super) const DIAGNOSTICS: &str = "diagnostics";
}

pub(crate) fn module_interface_type() -> Type {
    frontend_nominal(
        "ModuleInterface",
        record_type([
            (module_interface::IDENTITY, Type::Text),
            (module_interface::TIER, tier_type()),
            (module_interface::ABI, abi_digest_type()),
            (module_interface::SURFACE, Type::Bytes),
            (
                module_interface::DIAGNOSTICS,
                Type::List(Box::new(diagnostic_type())),
            ),
        ]),
    )
}

pub(crate) fn module_interface_value(
    module: &str,
    abi: ModuleAbiDigest,
    surface: &InterfaceSurface,
    diagnostics: &[ValueDiagnostic],
) -> Value {
    Value::Nominal {
        name: format!("{FRONTEND_MODULE}.ModuleInterface").into(),
        representation: Box::new(record_value([
            (module_interface::IDENTITY, Value::Text(module.into())),
            (module_interface::TIER, host_tier_value()),
            (module_interface::ABI, abi_digest_value(abi)),
            (
                module_interface::SURFACE,
                Value::Bytes(surface.encode().into()),
            ),
            (
                module_interface::DIAGNOSTICS,
                Value::List(diagnostics.iter().map(diagnostic_value).collect()),
            ),
        ])),
    }
}

/// The encoded surface an `interface-of` evaluation returned, or `None`
/// when the value is not the nominal that rule is declared to produce.
pub(crate) fn read_interface_surface(value: &Value) -> Option<&[u8]> {
    let Value::Nominal { representation, .. } = value else {
        return None;
    };
    let Value::Record(fields) = representation.as_ref() else {
        return None;
    };
    let field = fields
        .iter()
        .find(|field| field.name.as_ref() == module_interface::SURFACE)?;
    match &field.payload {
        Value::Bytes(bytes) => Some(bytes),
        _ => None,
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
