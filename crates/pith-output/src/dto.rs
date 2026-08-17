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
    List {
        elements: Box<[ValueRepr]>,
    },
    Record {
        fields: Box<[(Box<str>, ValueRepr)]>,
    },
    Sum {
        name: Box<str>,
        constructor: Box<str>,
        payload: Option<Box<ValueRepr>>,
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
    Nominal {
        name: Box<str>,
    },
    List {
        element: Box<TypeRepr>,
    },
    Record {
        fields: Box<[(Box<str>, TypeRepr)]>,
    },
    Sum {
        name: Box<str>,
        constructors: Box<[SumConstructorRepr]>,
    },
    /// The recursion cut inside a declaration's body (decision 0047). Rendered
    /// as a distinct kind rather than as the enclosing declaration repeated, so
    /// a reader sees a finite type.
    Cut,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct SumConstructorRepr {
    pub name: Box<str>,
    pub payload: Option<Box<TypeRepr>>,
}
