//! DTO projections of pith-core's value and type IR. Lives here so pith-core
//! does not depend on serde.

#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "value_kind", rename_all = "snake_case")]
pub enum ValueRepr {
    Unit,
    Bool { b: bool },
    Int { n: i64 },
    Text { s: Box<str> },
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "type_kind", rename_all = "snake_case")]
pub enum TypeRepr {
    Unit,
    Bool,
    Int,
    Text,
    Nominal { name: Box<str> },
}
