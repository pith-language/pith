//! DTO projections of pith-core's value and type IR. Lives here so pith-core
//! does not depend on serde.

#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "value_kind", rename_all = "snake_case")]
pub enum ValueRepr {
    Unit,
    Bool {
        b: bool,
    },
    Int {
        n: i64,
    },
    Text {
        s: Box<str>,
    },
    Bytes {
        len: u64,
    },
    Blob {
        digest: Box<str>,
    },
    Nominal {
        name: Box<str>,
        representation: Box<ValueRepr>,
    },
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "type_kind", rename_all = "snake_case")]
pub enum TypeRepr {
    Unit,
    Bool,
    Int,
    Text,
    Bytes,
    Blob,
    Nominal { name: Box<str> },
}
