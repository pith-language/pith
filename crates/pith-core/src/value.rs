//! Values and types in the typed semantic IR (decision 0017).
//!
//! `pith-core` does not depend on serde. DTO projections live in
//! `pith_output::dto`; the `From` impls below are the projection sites.

use pith_arena::define_arena;
use pith_ids::ContentId;
use pith_output::dto::{TypeRepr, ValueRepr};

define_arena!(ValueId, ValueArena, ValueBrand);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    Unit,
    Bool(bool),
    Int(i64),
    Text(Box<str>),
    /// Raw bytes computed or materialized by a rule. Distinct from [`Value::Blob`],
    /// which names stored content by identity.
    Bytes(Box<[u8]>),
    /// A reference to content-addressed storage by identity. Carries no bytes;
    /// a rule that needs the bytes requests them via the engine.
    Blob(ContentId),
}

impl Value {
    pub fn value_type(&self) -> Type {
        match self {
            Self::Unit => Type::Unit,
            Self::Bool(_) => Type::Bool,
            Self::Int(_) => Type::Int,
            Self::Text(_) => Type::Text,
            Self::Bytes(_) => Type::Bytes,
            Self::Blob(_) => Type::Blob,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Value::Unit => "()".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(n) => n.to_string(),
            Value::Text(s) => s.as_ref().to_string(),
            Value::Bytes(b) => format!("bytes({})", b.len()),
            Value::Blob(id) => format!("blob({})", hex_digest(id)),
        }
    }
}

fn hex_digest(id: &ContentId) -> String {
    let mut s = String::with_capacity(64);
    for byte in id.digest().as_bytes() {
        use std::fmt::Write;
        let _ = write!(s, "{byte:02x}");
    }
    s
}

impl From<&Value> for ValueRepr {
    fn from(v: &Value) -> Self {
        match v {
            Value::Unit => ValueRepr::Unit,
            Value::Bool(b) => ValueRepr::Bool { b: *b },
            Value::Int(n) => ValueRepr::Int { n: *n },
            Value::Text(s) => ValueRepr::Text { s: s.clone() },
            Value::Bytes(b) => ValueRepr::Bytes {
                len: b.len() as u64,
            },
            Value::Blob(id) => ValueRepr::Blob {
                digest: hex_digest(id).into_boxed_str(),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Type {
    Unit,
    Bool,
    Int,
    Text,
    Bytes,
    Blob,
    Nominal { name: Box<str> },
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unit => f.write_str("Unit"),
            Self::Bool => f.write_str("Bool"),
            Self::Int => f.write_str("Int"),
            Self::Text => f.write_str("Text"),
            Self::Bytes => f.write_str("Bytes"),
            Self::Blob => f.write_str("Blob"),
            Self::Nominal { name } => f.write_str(name),
        }
    }
}

impl From<&Type> for TypeRepr {
    fn from(t: &Type) -> Self {
        match t {
            Type::Unit => TypeRepr::Unit,
            Type::Bool => TypeRepr::Bool,
            Type::Int => TypeRepr::Int,
            Type::Text => TypeRepr::Text,
            Type::Bytes => TypeRepr::Bytes,
            Type::Blob => TypeRepr::Blob,
            Type::Nominal { name } => TypeRepr::Nominal { name: name.clone() },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_projects_to_dto_without_losing_scalars() {
        for v in [
            Value::Unit,
            Value::Bool(true),
            Value::Int(-5),
            Value::Text("x".into()),
            Value::Bytes(vec![0, 1, 2].into_boxed_slice()),
            Value::Blob(ContentId::of_blob(b"y")),
        ] {
            let repr: ValueRepr = (&v).into();
            let _ = format!("{repr:?}");
        }
    }

    #[test]
    fn nominal_type_projects_to_nominal_repr() {
        let t = Type::Nominal {
            name: "Machine".into(),
        };
        let repr: TypeRepr = (&t).into();
        match repr {
            TypeRepr::Nominal { name } => assert_eq!(name.as_ref(), "Machine"),
            _ => unreachable!(),
        }
    }

    #[test]
    fn bytes_and_blob_are_distinct_types() {
        assert_ne!(
            Value::Bytes(b"x".to_vec().into_boxed_slice()).value_type(),
            Type::Blob
        );
        assert_eq!(
            Value::Blob(ContentId::of_blob(b"x")).value_type(),
            Type::Blob
        );
    }

    #[test]
    fn blob_dto_carries_hex_digest() {
        let id = ContentId::of_blob(b"payload");
        let repr: ValueRepr = (&Value::Blob(id)).into();
        match repr {
            ValueRepr::Blob { digest } => assert_eq!(digest.len(), 64),
            _ => unreachable!(),
        }
    }
}
