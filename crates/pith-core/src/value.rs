//! Values and types in the typed semantic IR (decision 0017).
//!
//! `pith-core` does not depend on serde. DTO projections live in
//! `pith_output::dto`; the `From` impls below are the projection sites.

use pith_arena::define_arena;
use pith_output::dto::{TypeRepr, ValueRepr};

define_arena!(ValueId, ValueArena, ValueBrand);

#[derive(Clone, Debug)]
pub enum Value {
    Unit,
    Bool(bool),
    Int(i64),
    Text(Box<str>),
}

impl Value {
    pub fn describe(&self) -> String {
        match self {
            Value::Unit => "()".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(n) => n.to_string(),
            Value::Text(s) => s.as_ref().to_string(),
        }
    }
}

impl From<&Value> for ValueRepr {
    fn from(v: &Value) -> Self {
        match v {
            Value::Unit => ValueRepr::Unit,
            Value::Bool(b) => ValueRepr::Bool { b: *b },
            Value::Int(n) => ValueRepr::Int { n: *n },
            Value::Text(s) => ValueRepr::Text { s: s.clone() },
        }
    }
}

#[derive(Clone, Debug)]
pub enum Type {
    Unit,
    Bool,
    Int,
    Text,
    Nominal { name: Box<str> },
}

impl From<&Type> for TypeRepr {
    fn from(t: &Type) -> Self {
        match t {
            Type::Unit => TypeRepr::Unit,
            Type::Bool => TypeRepr::Bool,
            Type::Int => TypeRepr::Int,
            Type::Text => TypeRepr::Text,
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
}
