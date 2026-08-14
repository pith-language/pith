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
    /// A value of a declared nominal type (decision 0026). The name identifies
    /// the declaration; the representation is the underlying value. A nominal
    /// value is not interchangeable with its representation: a `MachineId` over
    /// `Text` is a distinct type from `Text`.
    Nominal {
        name: Box<str>,
        representation: Box<Value>,
    },
    /// An ordered sequence of values, the landed slice of the `List<T>`
    /// constructor 0026 names in the calculus constructor set. Homogeneous in
    /// use: the request and result checks below treat a list as inhabiting
    /// `List<T>` when every element inhabits `T`, and an empty list as
    /// inhabiting every `List<T>`.
    List(Box<[Value]>),
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
            Self::Nominal { name, .. } => Type::Nominal { name: name.clone() },
            Self::List(elements) => Type::List(Box::new(
                elements.first().map_or(Type::Unit, Value::value_type),
            )),
        }
    }

    /// Whether this value inhabits `expected`. Use it at request-input and
    /// result-type checks; `value_type` is the best-effort type for diagnostics
    /// and the two agree everywhere except lists, where an empty list inhabits
    /// every `List<T>` while `value_type` must pick one element type (`Unit`,
    /// when there is no element to ask). A nominal value matches its own name
    /// only; its representation does not match the representation's type
    /// (decision 0026).
    #[must_use]
    pub fn is_type(&self, expected: &Type) -> bool {
        match (self, expected) {
            (Self::Unit, Type::Unit)
            | (Self::Bool(_), Type::Bool)
            | (Self::Int(_), Type::Int)
            | (Self::Text(_), Type::Text)
            | (Self::Bytes(_), Type::Bytes)
            | (Self::Blob(_), Type::Blob) => true,
            (
                Self::Nominal { name, .. },
                Type::Nominal {
                    name: expected_name,
                },
            ) => name == expected_name,
            (Self::List(elements), Type::List(element)) => {
                elements.iter().all(|value| value.is_type(element))
            }
            (Self::Unit, _)
            | (Self::Bool(_), _)
            | (Self::Int(_), _)
            | (Self::Text(_), _)
            | (Self::Bytes(_), _)
            | (Self::Blob(_), _)
            | (Self::Nominal { .. }, _)
            | (Self::List(_), _) => false,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Unit => "()".to_string(),
            Self::Bool(b) => b.to_string(),
            Self::Int(n) => n.to_string(),
            Self::Text(s) => s.as_ref().to_string(),
            Self::Bytes(b) => format!("bytes({})", b.len()),
            Self::Blob(id) => format!("blob({})", hex_digest(id)),
            Self::Nominal {
                name,
                representation,
            } => format!("{}({})", name, representation.describe()),
            Self::List(elements) => {
                let inner: Vec<String> = elements.iter().map(Self::describe).collect();
                format!("[{}]", inner.join(", "))
            }
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
            Value::Nominal {
                name,
                representation,
            } => ValueRepr::Nominal {
                name: name.clone(),
                representation: Box::new(representation.as_ref().into()),
            },
            Value::List(elements) => ValueRepr::List {
                elements: elements.iter().map(Into::into).collect(),
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
    Nominal {
        name: Box<str>,
    },
    /// The type of a [`Value::List`] whose elements inhabit the element type.
    /// Type application is reified (0026): `List<Int>` and `List<Text>` are
    /// distinct types and distinct interface participants.
    List(Box<Type>),
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
            Self::List(element) => write!(f, "List<{element}>"),
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
            Type::List(element) => TypeRepr::List {
                element: Box::new(element.as_ref().into()),
            },
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
            Value::Nominal {
                name: "MachineId".into(),
                representation: Box::new(Value::Text("m-1".into())),
            },
            Value::List(vec![].into_boxed_slice()),
            Value::List(vec![Value::Text("a".into()), Value::Text("b".into())].into_boxed_slice()),
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

    #[test]
    fn is_type_agrees_with_value_type() {
        // is_type must accept exactly the type value_type would return, and
        // reject every other type. Cover every variant pairing.
        let values = [
            (Value::Unit, Type::Unit),
            (Value::Bool(true), Type::Bool),
            (Value::Int(7), Type::Int),
            (Value::Text("x".into()), Type::Text),
            (Value::Bytes(b"y".to_vec().into_boxed_slice()), Type::Bytes),
            (Value::Blob(ContentId::of_blob(b"z")), Type::Blob),
            (
                Value::Nominal {
                    name: "MachineId".into(),
                    representation: Box::new(Value::Text("m-1".into())),
                },
                Type::Nominal {
                    name: "MachineId".into(),
                },
            ),
            (
                Value::List(
                    vec![Value::Text("a".into()), Value::Text("b".into())].into_boxed_slice(),
                ),
                Type::List(Box::new(Type::Text)),
            ),
        ];
        let all_types = [
            Type::Unit,
            Type::Bool,
            Type::Int,
            Type::Text,
            Type::Bytes,
            Type::Blob,
            Type::Nominal {
                name: "MachineId".into(),
            },
            Type::List(Box::new(Type::Text)),
            Type::List(Box::new(Type::Int)),
        ];
        for (value, own_type) in &values {
            for candidate in &all_types {
                assert_eq!(
                    value.is_type(candidate),
                    *own_type == *candidate,
                    "{:?}.is_type({:?}) disagreed with value_type",
                    value,
                    candidate,
                );
            }
        }
    }

    #[test]
    fn a_list_is_typed_by_its_elements() {
        // 0026 reifies type application: List<Text> and List<Int> are distinct
        // types, so a list of one element type does not inhabit another. A
        // nominal element keeps its identity inside the list, which is what
        // keeps two list-consuming rules from collapsing onto one interface.
        let texts = Value::List(vec![Value::Text("a".into())].into_boxed_slice());
        let objects = Value::List(
            vec![Value::Nominal {
                name: "Object".into(),
                representation: Box::new(Value::Blob(ContentId::of_blob(b"o"))),
            }]
            .into_boxed_slice(),
        );
        let blobs = Value::List(vec![Value::Blob(ContentId::of_blob(b"o"))].into_boxed_slice());

        assert!(texts.is_type(&Type::List(Box::new(Type::Text))));
        assert!(!texts.is_type(&Type::List(Box::new(Type::Int))));
        assert!(objects.is_type(&Type::List(Box::new(Type::Nominal {
            name: "Object".into()
        }))));
        assert!(!objects.is_type(&Type::List(Box::new(Type::Blob))));
        assert!(!blobs.is_type(&Type::List(Box::new(Type::Nominal {
            name: "Object".into()
        }))));
    }

    #[test]
    fn an_empty_list_inhabits_every_list_type() {
        // value_type must pick one element type when there is no element to
        // ask, so an empty list would otherwise be rejected at request-input
        // and result checks against any concrete List<T>. is_type is the
        // check those sites use, and it accepts it.
        let empty = Value::List(vec![].into_boxed_slice());
        assert!(empty.is_type(&Type::List(Box::new(Type::Text))));
        assert!(empty.is_type(&Type::List(Box::new(Type::Int))));
        assert_eq!(empty.value_type(), Type::List(Box::new(Type::Unit)));
    }

    #[test]
    fn nominal_value_is_not_interchangeable_with_its_representation() {
        // Decision 0026: a nominal value matches its own name only. It does not
        // match the representation's type, and a different nominal name is a
        // different type.
        let machine_id = Value::Nominal {
            name: "MachineId".into(),
            representation: Box::new(Value::Text("m-1".into())),
        };
        assert!(machine_id.is_type(&Type::Nominal {
            name: "MachineId".into(),
        }));
        assert!(!machine_id.is_type(&Type::Text));
        assert!(!machine_id.is_type(&Type::Nominal {
            name: "OtherId".into(),
        }));
    }
}
