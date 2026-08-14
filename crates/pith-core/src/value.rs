//! Values and types in the typed semantic IR (decision 0017).
//!
//! `pith-core` does not depend on serde. DTO projections live in
//! `pith_output::dto`; the `From` impls below are the projection sites.

use pith_arena::define_arena;
use pith_ids::ContentId;
use pith_output::dto::{SumConstructorRepr, TypeRepr, ValueRepr};

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
    /// Named fields, the landed slice of the closed record constructor 0026
    /// names in the calculus constructor set. Closed: a record inhabits a
    /// record type only when the field sets are equal, with no width or depth
    /// subtyping and no optional fields. Construction sorts the fields by
    /// name, which is the canonical order 0026 fixes for the encoding.
    Record(Box<[RecordField<Value>]>),
    /// One constructor of a declared sum, the landed slice of the sum
    /// constructor 0026 names in the calculus constructor set. The value
    /// carries the sum's name the way [`Value::Nominal`] carries its own —
    /// the declaration site that would resolve the name does not exist yet —
    /// plus the constructor it selects and that constructor's payload. It
    /// cannot recover the sibling constructors, which is the sum half of the
    /// `value_type` asymmetry documented on [`Value::is_type`].
    Sum {
        type_name: Box<str>,
        constructor: Box<str>,
        payload: Option<Box<Value>>,
    },
}

/// One named field of a record. Shared by [`Value::Record`] (the field's
/// value) and [`Type::Record`] (the field's type).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RecordField<T> {
    pub name: Box<str>,
    pub payload: T,
}

/// One constructor of a declared sum, with the payload type it carries when
/// it carries one.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SumConstructor {
    pub name: Box<str>,
    pub payload: Option<Type>,
}

/// A closed set of named things — a record's fields, a sum's constructors —
/// was built with one name twice. The second entry would be unreachable by
/// name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateNameError {
    pub name: Box<str>,
}

impl std::fmt::Display for DuplicateNameError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "the name `{}` is declared twice", self.name)
    }
}

impl std::error::Error for DuplicateNameError {}

/// Sort a closed set of named entries into canonical name order, refusing a
/// name used twice: canonical order is strictly ascending, so a duplicate
/// would also be unencodable.
fn sorted_by_unique_name<T>(
    entries: impl Into<Box<[T]>>,
    name: impl Fn(&T) -> &str,
) -> Result<Box<[T]>, DuplicateNameError> {
    let mut entries = entries.into();
    entries.sort_by(|left, right| name(left).cmp(name(right)));
    for pair in entries.windows(2) {
        if let [earlier, later] = pair
            && name(earlier) == name(later)
        {
            return Err(DuplicateNameError {
                name: name(earlier).into(),
            });
        }
    }
    Ok(entries)
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
            Self::Record(fields) => Type::Record(
                fields
                    .iter()
                    .map(|field| RecordField {
                        name: field.name.clone(),
                        payload: field.payload.value_type(),
                    })
                    .collect(),
            ),
            // The declared sum's other constructors are not recoverable from
            // one of its values, so the best-effort type is the singleton sum
            // holding only the constructor this value selected. That is not
            // the declared type, and `is_type` is the check that decides
            // inhabitation; see its doc comment.
            Self::Sum {
                type_name,
                constructor,
                payload,
            } => Type::Sum {
                name: type_name.clone(),
                constructors: [SumConstructor {
                    name: constructor.clone(),
                    payload: payload.as_deref().map(|payload| payload.value_type()),
                }]
                .into(),
            },
        }
    }

    /// Whether this value inhabits `expected`. Use it at request-input and
    /// result-type checks; `value_type` is the best-effort type for diagnostics
    /// and the two agree everywhere except where a value cannot know its own
    /// whole type, which happens from both directions. an empty list inhabits
    /// every `List<T>` while `value_type` must pick one element type (`Unit`,
    /// when there is no element to ask). a sum value knows the constructor it
    /// selected but cannot recover its sibling constructors, so it inhabits
    /// every declared sum that contains that constructor with a matching
    /// payload, while `value_type` names only the singleton sum it can honestly
    /// construct. a nominal value matches its own name only; its
    /// representation does not match the representation's type (decision
    /// 0026).
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
            (Self::Record(fields), Type::Record(declared)) => {
                // Both sides carry their fields sorted by name, so one walk
                // decides: a closed record matches its own field set exactly,
                // with no width or depth subtyping (0026).
                fields.len() == declared.len()
                    && fields.iter().zip(declared.iter()).all(|(field, expected)| {
                        field.name == expected.name && field.payload.is_type(&expected.payload)
                    })
            }
            (
                Self::Sum {
                    type_name,
                    constructor,
                    payload,
                },
                Type::Sum {
                    name: expected_name,
                    constructors,
                },
            ) => {
                type_name == expected_name
                    && constructors.iter().any(|declared| {
                        declared.name == *constructor
                            && match (&declared.payload, payload.as_deref()) {
                                (Some(expected_payload), Some(payload)) => {
                                    payload.is_type(expected_payload)
                                }
                                (None, None) => true,
                                _ => false,
                            }
                    })
            }
            (Self::Unit, _)
            | (Self::Bool(_), _)
            | (Self::Int(_), _)
            | (Self::Text(_), _)
            | (Self::Bytes(_), _)
            | (Self::Blob(_), _)
            | (Self::Nominal { .. }, _)
            | (Self::List(_), _)
            | (Self::Record(_), _)
            | (Self::Sum { .. }, _) => false,
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
            Self::Record(fields) => {
                let inner: Vec<String> = fields
                    .iter()
                    .map(|field| format!("{}: {}", field.name, field.payload.describe()))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
            Self::Sum {
                type_name,
                constructor,
                payload,
            } => match payload.as_deref() {
                Some(payload) => format!("{}::{}({})", type_name, constructor, payload.describe()),
                None => format!("{}::{}", type_name, constructor),
            },
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
            Value::Record(fields) => ValueRepr::Record {
                fields: fields
                    .iter()
                    .map(|field| (field.name.clone(), (&field.payload).into()))
                    .collect(),
            },
            Value::Sum {
                type_name,
                constructor,
                payload,
            } => ValueRepr::Sum {
                name: type_name.clone(),
                constructor: constructor.clone(),
                payload: payload.as_deref().map(|payload| Box::new(payload.into())),
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
    /// The closed record constructor of the 0026 calculus: named fields,
    /// sorted by name at construction, which is the canonical order the
    /// encoding writes and the only order the decoder accepts.
    Record(Box<[RecordField<Type>]>),
    /// The declared sum constructor of the 0026 calculus: a nominal name over
    /// a fixed set of constructors, each optionally carrying a typed payload,
    /// sorted by constructor name at construction. Not a record with a tag
    /// field and not a polymorphic variant — 0026 rejects both and gives the
    /// reasons.
    Sum {
        name: Box<str>,
        constructors: Box<[SumConstructor]>,
    },
}

impl Type {
    /// Build a record type, sorting the fields into canonical name order.
    ///
    /// # Errors
    /// [`DuplicateNameError`] when two fields share a name.
    pub fn record(fields: impl Into<Box<[RecordField<Type>]>>) -> Result<Self, DuplicateNameError> {
        Ok(Self::Record(sorted_by_unique_name(
            fields,
            |field: &RecordField<Type>| &field.name,
        )?))
    }

    /// Build a declared sum type, sorting the constructors into canonical
    /// name order.
    ///
    /// # Errors
    /// [`DuplicateNameError`] when two constructors share a name.
    pub fn sum(
        name: impl Into<Box<str>>,
        constructors: impl Into<Box<[SumConstructor]>>,
    ) -> Result<Self, DuplicateNameError> {
        Ok(Self::Sum {
            name: name.into(),
            constructors: sorted_by_unique_name(constructors, |constructor: &SumConstructor| {
                &constructor.name
            })?,
        })
    }
}

impl Value {
    /// Build a record value, sorting the fields into canonical name order.
    ///
    /// # Errors
    /// [`DuplicateNameError`] when two fields share a name.
    pub fn record(
        fields: impl Into<Box<[RecordField<Value>]>>,
    ) -> Result<Self, DuplicateNameError> {
        Ok(Self::Record(sorted_by_unique_name(
            fields,
            |field: &RecordField<Value>| &field.name,
        )?))
    }
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
            Self::Record(fields) => {
                f.write_str("{ ")?;
                let mut separator = "";
                for field in &**fields {
                    f.write_str(separator)?;
                    write!(f, "{}: {}", field.name, field.payload)?;
                    separator = ", ";
                }
                f.write_str(" }")
            }
            Self::Sum { name, constructors } => {
                write!(f, "{name}::{{")?;
                let mut separator = "";
                for constructor in &**constructors {
                    f.write_str(separator)?;
                    match &constructor.payload {
                        Some(payload) => write!(f, "{}({payload})", constructor.name)?,
                        None => f.write_str(&constructor.name)?,
                    }
                    separator = ", ";
                }
                f.write_str("}")
            }
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
            Type::Record(fields) => TypeRepr::Record {
                fields: fields
                    .iter()
                    .map(|field| (field.name.clone(), (&field.payload).into()))
                    .collect(),
            },
            Type::Sum { name, constructors } => TypeRepr::Sum {
                name: name.clone(),
                constructors: constructors
                    .iter()
                    .map(|constructor| SumConstructorRepr {
                        name: constructor.name.clone(),
                        payload: constructor
                            .payload
                            .as_ref()
                            .map(|payload| Box::new(payload.into())),
                    })
                    .collect(),
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
            Value::record([
                RecordField {
                    name: "name".into(),
                    payload: Value::Text("pith".into()),
                },
                RecordField {
                    name: "version".into(),
                    payload: Value::Int(1),
                },
            ])
            .unwrap(),
            Value::Sum {
                type_name: "Source".into(),
                constructor: "Git".into(),
                payload: Some(Box::new(Value::Text("main".into()))),
            },
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
            (
                Value::record([
                    RecordField {
                        name: "name".into(),
                        payload: Value::Text("pith".into()),
                    },
                    RecordField {
                        name: "version".into(),
                        payload: Value::Int(1),
                    },
                ])
                .unwrap(),
                Type::record([
                    RecordField {
                        name: "name".into(),
                        payload: Type::Text,
                    },
                    RecordField {
                        name: "version".into(),
                        payload: Type::Int,
                    },
                ])
                .unwrap(),
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
            Type::record([
                RecordField {
                    name: "name".into(),
                    payload: Type::Text,
                },
                RecordField {
                    name: "version".into(),
                    payload: Type::Int,
                },
            ])
            .unwrap(),
            Type::record([RecordField {
                name: "name".into(),
                payload: Type::Text,
            }])
            .unwrap(),
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

    #[test]
    fn record_construction_orders_fields_and_rejects_duplicates() {
        // Canonical order is name order (0026), so construction order cannot
        // reach equality, hashing, or the encoding. A repeated name has no
        // meaning in a closed record.
        let first = Value::record([
            RecordField {
                name: "b".into(),
                payload: Value::Int(1),
            },
            RecordField {
                name: "a".into(),
                payload: Value::Text("x".into()),
            },
        ])
        .unwrap();
        let second = Value::record([
            RecordField {
                name: "a".into(),
                payload: Value::Text("x".into()),
            },
            RecordField {
                name: "b".into(),
                payload: Value::Int(1),
            },
        ])
        .unwrap();
        assert_eq!(first, second);

        let duplicate = Value::record([
            RecordField {
                name: "a".into(),
                payload: Value::Int(1),
            },
            RecordField {
                name: "a".into(),
                payload: Value::Int(2),
            },
        ]);
        assert_eq!(
            duplicate.unwrap_err(),
            DuplicateNameError { name: "a".into() }
        );
        assert_eq!(
            Type::record([
                RecordField {
                    name: "a".into(),
                    payload: Type::Int,
                },
                RecordField {
                    name: "a".into(),
                    payload: Type::Text,
                },
            ])
            .unwrap_err(),
            DuplicateNameError { name: "a".into() }
        );
    }

    #[test]
    fn a_record_matches_only_its_own_field_set() {
        // Closed records: no width subtyping in either direction and no depth
        // subtyping on a field (0026). A nominal payload keeps its identity
        // inside a record field the way it does inside a list.
        let value = Value::record([
            RecordField {
                name: "name".into(),
                payload: Value::Text("pith".into()),
            },
            RecordField {
                name: "version".into(),
                payload: Value::Int(1),
            },
        ])
        .unwrap();
        let same = Type::record([
            RecordField {
                name: "version".into(),
                payload: Type::Int,
            },
            RecordField {
                name: "name".into(),
                payload: Type::Text,
            },
        ])
        .unwrap();
        let wider = Type::record([
            RecordField {
                name: "name".into(),
                payload: Type::Text,
            },
            RecordField {
                name: "version".into(),
                payload: Type::Int,
            },
            RecordField {
                name: "options".into(),
                payload: Type::List(Box::new(Type::Text)),
            },
        ])
        .unwrap();
        let narrower = Type::record([RecordField {
            name: "name".into(),
            payload: Type::Text,
        }])
        .unwrap();
        let deeper = Type::record([
            RecordField {
                name: "name".into(),
                payload: Type::Text,
            },
            RecordField {
                name: "version".into(),
                payload: Type::List(Box::new(Type::Int)),
            },
        ])
        .unwrap();

        assert!(value.is_type(&same));
        assert!(!value.is_type(&wider));
        assert!(!value.is_type(&narrower));
        assert!(!value.is_type(&deeper));
    }

    #[test]
    fn a_sum_value_inhabits_the_declared_sums_that_contain_its_constructor() {
        // A sum value knows its own constructor and payload but not its
        // siblings, so inhabitation is membership: the declared name must
        // match, the constructor must be in the declared set, and the payload
        // must be present exactly when the declaration carries one and inhabit
        // the declared payload type.
        let source = Type::sum(
            "Source",
            [
                SumConstructor {
                    name: "Git".into(),
                    payload: Some(Type::Text),
                },
                SumConstructor {
                    name: "Path".into(),
                    payload: None,
                },
            ],
        )
        .unwrap();
        let git = Value::Sum {
            type_name: "Source".into(),
            constructor: "Git".into(),
            payload: Some(Box::new(Value::Text("main".into()))),
        };
        let path = Value::Sum {
            type_name: "Source".into(),
            constructor: "Path".into(),
            payload: None,
        };
        let renamed = Type::sum(
            "Origin",
            [
                SumConstructor {
                    name: "Git".into(),
                    payload: Some(Type::Text),
                },
                SumConstructor {
                    name: "Path".into(),
                    payload: None,
                },
            ],
        )
        .unwrap();
        let without_git = Type::sum(
            "Source",
            [SumConstructor {
                name: "Path".into(),
                payload: None,
            }],
        )
        .unwrap();
        let mistyped = Type::sum(
            "Source",
            [
                SumConstructor {
                    name: "Git".into(),
                    payload: Some(Type::Int),
                },
                SumConstructor {
                    name: "Path".into(),
                    payload: None,
                },
            ],
        )
        .unwrap();
        let bare_git = Type::sum(
            "Source",
            [
                SumConstructor {
                    name: "Git".into(),
                    payload: None,
                },
                SumConstructor {
                    name: "Path".into(),
                    payload: None,
                },
            ],
        )
        .unwrap();

        assert!(git.is_type(&source));
        assert!(path.is_type(&source));
        assert!(!git.is_type(&renamed));
        assert!(!git.is_type(&without_git));
        assert!(!git.is_type(&mistyped));
        assert!(!git.is_type(&bare_git));
        assert!(path.is_type(&bare_git));
    }

    #[test]
    fn sum_construction_orders_constructors_and_rejects_duplicates() {
        let first = Type::sum(
            "Source",
            [
                SumConstructor {
                    name: "Git".into(),
                    payload: Some(Type::Text),
                },
                SumConstructor {
                    name: "Archive".into(),
                    payload: Some(Type::Blob),
                },
            ],
        )
        .unwrap();
        let second = Type::sum(
            "Source",
            [
                SumConstructor {
                    name: "Archive".into(),
                    payload: Some(Type::Blob),
                },
                SumConstructor {
                    name: "Git".into(),
                    payload: Some(Type::Text),
                },
            ],
        )
        .unwrap();
        assert_eq!(first, second);

        let duplicate = Type::sum(
            "Source",
            [
                SumConstructor {
                    name: "Git".into(),
                    payload: None,
                },
                SumConstructor {
                    name: "Git".into(),
                    payload: Some(Type::Text),
                },
            ],
        );
        assert_eq!(
            duplicate.unwrap_err(),
            DuplicateNameError { name: "Git".into() }
        );
    }

    #[test]
    fn a_sum_value_names_the_singleton_it_can_and_inhabits_the_declaration() {
        // The mirror of the empty-list asymmetry: `value_type` cannot recover
        // the sibling constructors, so it names the singleton sum holding only
        // the selected constructor — a type the value does inhabit, since the
        // singleton contains that constructor with a matching payload — while
        // `is_type` accepts the declared type as well. Request-input checking
        // compares with `is_type`, so the singleton is a diagnostic, never a
        // gate.
        let declared = Type::sum(
            "Source",
            [
                SumConstructor {
                    name: "Git".into(),
                    payload: Some(Type::Text),
                },
                SumConstructor {
                    name: "Path".into(),
                    payload: None,
                },
            ],
        )
        .unwrap();
        let git = Value::Sum {
            type_name: "Source".into(),
            constructor: "Git".into(),
            payload: Some(Box::new(Value::Text("main".into()))),
        };

        let best_effort = git.value_type();
        assert_eq!(
            best_effort,
            Type::Sum {
                name: "Source".into(),
                constructors: [SumConstructor {
                    name: "Git".into(),
                    payload: Some(Type::Text),
                }]
                .into(),
            }
        );
        assert_ne!(best_effort, declared);
        assert!(git.is_type(&best_effort));
        assert!(git.is_type(&declared));
    }
}
