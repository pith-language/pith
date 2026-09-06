//! The scalar codecs the graph's records share: diagnostics, the tier and
//! rule-category sums, and the ABI digest nominal.

use pith_core::{Int, SumConstructor, SumType, Type, Value};
use pith_ids::{ContentId, ModuleAbiDigest};

use crate::RuleCategory;

use super::{FRONTEND_MODULE, frontend_nominal, record_type, record_value};

/// One diagnostic as the graph's values carry it: the code, the message,
/// the source it names, and the span inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValueDiagnostic {
    pub code: u32,
    pub message: Box<str>,
    pub source: ContentId,
    pub start: u32,
    pub end: u32,
}

pub(super) fn diagnostic_type() -> Type {
    record_type([
        ("code", Type::Int),
        ("message", Type::Text),
        ("source", Type::Blob),
        ("start", Type::Int),
        ("end", Type::Int),
    ])
}

pub(super) fn diagnostic_value(diagnostic: &ValueDiagnostic) -> Value {
    record_value([
        ("code", Value::int(Int::from(diagnostic.code))),
        ("message", Value::Text(diagnostic.message.clone())),
        ("source", Value::Blob(diagnostic.source)),
        ("start", Value::int(Int::from(diagnostic.start))),
        ("end", Value::int(Int::from(diagnostic.end))),
    ])
}

pub(super) fn tier_type() -> Type {
    Type::Sum(Box::new(SumType {
        coordinate: pith_core::Coordinate::new(FRONTEND_MODULE, "Tier"),
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

pub(super) fn host_tier_value() -> Value {
    Value::Sum {
        type_name: format!("{FRONTEND_MODULE}.Tier").into(),
        constructor: "Host".into(),
        payload: None,
    }
}

pub(super) fn abi_digest_type() -> Type {
    frontend_nominal("ModuleAbiDigest", Type::Blob)
}

pub(super) fn abi_digest_value(abi: ModuleAbiDigest) -> Value {
    Value::Nominal {
        name: format!("{FRONTEND_MODULE}.ModuleAbiDigest").into(),
        representation: Box::new(Value::Blob(ContentId::from_digest(abi.digest()))),
    }
}

pub(super) fn rule_category_type() -> Type {
    Type::Sum(Box::new(SumType {
        coordinate: pith_core::Coordinate::new(FRONTEND_MODULE, "RuleCategory"),
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

pub(super) fn rule_category_value(category: RuleCategory) -> Value {
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
