//! Values and types in the typed semantic IR (decision 0017).
//!
//! `pith-core` does not depend on serde. DTO projections live in
//! `pith_output::dto`; the `From` impls in this module are the projection
//! sites.

use pith_arena::define_arena;
use pith_ids::ContentId;

use crate::declaration::{Coordinate, Declaration, DeclarationBody};
use crate::int::Int;
use pith_output::dto::{SumConstructorRepr, TypeRepr, ValueRepr};

define_arena!(ValueId, ValueArena, ValueBrand);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    Unit,
    Bool(bool),
    /// An integer of arbitrary precision (decision 0055). Addition,
    /// subtraction, and multiplication are closed over it, which is what makes
    /// arithmetic total and lets 0018's termination construction cover it
    /// without an overflow case.
    Int(Int),
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
    /// use: the request and result checks in this crate treat a list as
    /// inhabiting `List<T>` when every element inhabits `T`, and an empty
    /// list as inhabiting every `List<T>`.
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
            // A value's name is a string, not a table lookup, so the best a
            // value can do is synthesize the declaration its own content
            // supports: the module split off the coordinate spelling, and the
            // representation's best-effort type as the declared representation.
            // For a fully determined representation the synthesis is the
            // declared one (decision 0047).
            Self::Nominal {
                name,
                representation,
            } => Type::Nominal(Box::new(NominalType {
                coordinate: Coordinate::parse(name),
                representation: representation.value_type(),
            })),
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
            } => Type::Sum(Box::new(SumType {
                coordinate: Coordinate::parse(type_name),
                constructors: [SumConstructor {
                    name: constructor.clone(),
                    payload: payload.as_deref().map(|payload| payload.value_type()),
                }]
                .into(),
            })),
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
        self.inhabits(expected, None)
    }

    /// [`Self::is_type`] with the enclosing declaration a [`Type::Cut`] resolves
    /// to. `None` at the top level, where a cut has nothing to resolve against.
    ///
    /// Threading the enclosing reference rather than taking a table keeps the
    /// public check a function of its two arguments (decision 0047). Recursion
    /// terminates on the value: each step through a cut consumes one level of a
    /// finite value, so checking costs time proportional to the value rather than
    /// to the type.
    fn inhabits(&self, expected: &Type, enclosing: Option<&Type>) -> bool {
        if let Type::Cut = expected {
            return match enclosing {
                Some(declaration) => self.inhabits(declaration, enclosing),
                None => false,
            };
        }
        match (self, expected) {
            (Self::Unit, Type::Unit)
            | (Self::Bool(_), Type::Bool)
            | (Self::Int(_), Type::Int)
            | (Self::Text(_), Type::Text)
            | (Self::Bytes(_), Type::Bytes)
            | (Self::Blob(_), Type::Blob) => true,
            // Two checks in order, which is what closes the representation
            // hole (decision 0047): the value names the declaration's
            // coordinate, and its representation inhabits the declared
            // representation type. A value naming `xylem.Object` while holding
            // a `Text` is refused at the same gate that already checked the
            // name. The declaration becomes the enclosing one, so a cut inside
            // its representation resolves back to it.
            (
                Self::Nominal {
                    name,
                    representation,
                },
                Type::Nominal(declared),
            ) => {
                name.as_ref() == declared.coordinate.spelling()
                    && representation.inhabits(&declared.representation, Some(expected))
            }
            (Self::List(elements), Type::List(element)) => elements
                .iter()
                .all(|value| value.inhabits(element, enclosing)),
            (Self::Record(fields), Type::Record(declared)) => {
                // Both sides carry their fields sorted by name, so one walk
                // decides: a closed record matches its own field set exactly,
                // with no width or depth subtyping (0026).
                fields.len() == declared.len()
                    && fields.iter().zip(declared.iter()).all(|(field, expected)| {
                        field.name == expected.name
                            && field.payload.inhabits(&expected.payload, enclosing)
                    })
            }
            (
                Self::Sum {
                    type_name,
                    constructor,
                    payload,
                },
                Type::Sum(declared_sum),
            ) => {
                type_name.as_ref() == declared_sum.coordinate.spelling()
                    && declared_sum.constructors.iter().any(|declared| {
                        declared.name == *constructor
                            && match (&declared.payload, payload.as_deref()) {
                                (Some(expected_payload), Some(payload)) => {
                                    payload.inhabits(expected_payload, Some(expected))
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
            // A JSON number cannot carry an arbitrary-precision integer without
            // a reader's float parser rounding it, so the projection renders the
            // decimal (decision 0055).
            Value::Int(n) => ValueRepr::Int {
                decimal: n.to_string().into_boxed_str(),
            },
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

/// A use-site reference to a declared nominal type, carrying the declaration
/// (decision 0047).
///
/// The declaration travels in the type rather than being resolved through an
/// ambient table, which is what keeps [`Value::is_type`] a total function of its
/// two arguments and keeps a decoded type able to check as well as a constructed
/// one. The cost is bytes; pre-release those are cheaper than the correctness
/// risk of a checker whose answer depends on which table the reader happens to
/// hold.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NominalType {
    pub coordinate: Coordinate,
    pub representation: Type,
}

/// A use-site reference to a declared sum, carrying the declaration.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SumType {
    pub coordinate: Coordinate,
    pub constructors: Box<[SumConstructor]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Type {
    Unit,
    Bool,
    Int,
    Text,
    Bytes,
    Blob,
    /// A declared nominal type. Carries its declaration, so `is_type` verifies
    /// that a value naming this coordinate also holds a representation the
    /// declaration admits — the check decision 0026 promised "when the
    /// declaration lands."
    Nominal(Box<NominalType>),
    /// The type of a [`Value::List`] whose elements inhabit the element type.
    /// Type application is reified (0026): `List<Int>` and `List<Text>` are
    /// distinct types and distinct interface participants.
    List(Box<Type>),
    /// The closed record constructor of the 0026 calculus: named fields,
    /// sorted by name at construction, which is the canonical order the
    /// encoding writes and the only order the decoder accepts.
    Record(Box<[RecordField<Type>]>),
    /// The declared sum constructor of the 0026 calculus: a declared name over
    /// a fixed set of constructors, each optionally carrying a typed payload,
    /// sorted by constructor name at construction. Not a record with a tag
    /// field and not a polymorphic variant — 0026 rejects both and gives the
    /// reasons.
    Sum(Box<SumType>),
    /// The recursion cut: an occurrence of the declaration currently being
    /// declared, inside its own body (decision 0047).
    ///
    /// `xylem.Tree = Node(List<Tree>) | Leaf(Int)` has a finite canonical form
    /// because the inner `Tree` is this, rather than the whole declaration
    /// expanded again. Only *direct* self-reference is spellable, so mutual
    /// recursion between two declarations is unconstructible rather than
    /// refused — the cut names its enclosing declaration and has no way to name
    /// another.
    ///
    /// A cut outside a declaration body inhabits nothing: there is no enclosing
    /// declaration to resolve it against, and `is_type` answers `false` rather
    /// than guessing.
    Cut,
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

    /// The use-site type of `declaration`.
    ///
    /// A nominal or a sum yields a reference carrying its declaration; an alias
    /// yields its target, expanded, because an alias has no spelling of its own
    /// (decision 0047).
    #[must_use]
    pub fn of_declaration(declaration: &Declaration) -> Self {
        match declaration.body() {
            DeclarationBody::Nominal { representation } => Self::Nominal(Box::new(NominalType {
                coordinate: declaration.coordinate().clone(),
                representation: representation.clone(),
            })),
            DeclarationBody::Sum { constructors } => Self::Sum(Box::new(SumType {
                coordinate: declaration.coordinate().clone(),
                constructors: constructors.clone(),
            })),
            DeclarationBody::Alias { target } => target.clone(),
        }
    }

    /// Whether this type reaches a [`Type::Cut`], which is what makes an alias
    /// recursive and therefore unexpandable (decision 0047).
    ///
    /// The walk stops at a nominal or a sum reference: a cut inside one of those
    /// belongs to *that* declaration and is finite there, so it says nothing
    /// about whether this type expands forever.
    #[must_use]
    pub fn reaches_cut(&self) -> bool {
        match self {
            Self::Cut => true,
            Self::List(element) => element.reaches_cut(),
            Self::Record(fields) => fields.iter().any(|field| field.payload.reaches_cut()),
            Self::Unit
            | Self::Bool
            | Self::Int
            | Self::Text
            | Self::Bytes
            | Self::Blob
            | Self::Nominal(_)
            | Self::Sum(_) => false,
        }
    }
}

/// A nominal type declared in a throwaway single-entry table, for tests and
/// doctests that need a declaration and not a module.
///
/// There is deliberately no public shortcut past the table: a declaration
/// reference exists only because a table admitted the name, which is what makes
/// the duplicate and recursive-alias refusals real rather than advisory
/// (decision 0047).
#[cfg(test)]
pub(crate) fn declared_nominal(module: &str, name: &str, representation: Type) -> Type {
    let mut table = crate::declaration::DeclarationTable::new(module);
    match table.nominal(name, representation) {
        Ok(declared) => declared,
        Err(error) => unreachable!("a fresh table admits one name: {error}"),
    }
}

/// A declared sum in a throwaway single-entry table. See [`declared_nominal`].
#[cfg(test)]
pub(crate) fn declared_sum(
    module: &str,
    name: &str,
    constructors: impl Into<Box<[SumConstructor]>>,
) -> Type {
    let mut table = crate::declaration::DeclarationTable::new(module);
    match table.sum(name, constructors) {
        Ok(declared) => declared,
        Err(error) => unreachable!("a fresh table admits one name: {error}"),
    }
}

/// Sort a constructor set into canonical name order, refusing a repeat.
///
/// Used by [`crate::declaration::DeclarationTable::sum`], which is where a sum's
/// constructors are fixed now that a sum type carries its declaration.
///
/// # Errors
/// [`DuplicateNameError`] when two constructors share a name.
pub(crate) fn sorted_constructors(
    constructors: impl Into<Box<[SumConstructor]>>,
) -> Result<Box<[SumConstructor]>, DuplicateNameError> {
    sorted_by_unique_name(constructors, |constructor: &SumConstructor| {
        &constructor.name
    })
}

impl Value {
    /// An integer value from any machine integer that converts into an [`Int`].
    #[must_use]
    pub fn int(value: impl Into<Int>) -> Self {
        Self::Int(value.into())
    }

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
            Self::Nominal(declared) => f.write_str(&declared.coordinate.spelling()),
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
            Self::Cut => f.write_str("Self"),
            Self::Sum(declared) => {
                let (name, constructors) = (&declared.coordinate, &declared.constructors);
                write!(f, "{}::{{", name.spelling())?;
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
            // The projection carries the coordinate's spelling rather than the
            // declaration. A rendered type is for a reader, and the declared
            // representation is the checker's business — decision 0047 puts the
            // body in the type so `is_type` needs no table, not so every
            // diagnostic repeats it.
            Type::Nominal(declared) => TypeRepr::Nominal {
                name: declared.coordinate.spelling().into(),
            },
            Type::Cut => TypeRepr::Cut,
            Type::List(element) => TypeRepr::List {
                element: Box::new(element.as_ref().into()),
            },
            Type::Record(fields) => TypeRepr::Record {
                fields: fields
                    .iter()
                    .map(|field| (field.name.clone(), (&field.payload).into()))
                    .collect(),
            },
            Type::Sum(declared) => TypeRepr::Sum {
                name: declared.coordinate.spelling().into(),
                constructors: declared
                    .constructors
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
            Value::int(-5),
            Value::Text("x".into()),
            Value::Bytes(vec![0, 1, 2].into_boxed_slice()),
            Value::Blob(ContentId::of_blob(b"y")),
            Value::Nominal {
                name: "test.MachineId".into(),
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
                    payload: Value::int(1),
                },
            ])
            .unwrap(),
            Value::Sum {
                type_name: "test.Source".into(),
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
        let t = declared_nominal("test", "Machine", Type::Blob);
        let repr: TypeRepr = (&t).into();
        match repr {
            // The projection carries the coordinate's spelling, so a reader of
            // a rendered type sees which module declared it (decision 0047).
            TypeRepr::Nominal { name } => assert_eq!(name.as_ref(), "test.Machine"),
            _ => unreachable!(),
        }
    }

    #[test]
    fn an_arbitrary_precision_integer_does_not_widen_the_value_it_lives_in() {
        // The magnitude is inline for the range the type used to hold, and the
        // sum variant's three fields are wider than that, so no value in the
        // engine grew when the integer stopped being an `i64` (decision 0055).
        assert!(
            size_of::<Int>() <= size_of::<(Box<str>, Box<str>, Option<Box<Value>>)>(),
            "an integer is now the variant that decides how big every value is"
        );
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
            (Value::int(7), Type::Int),
            (Value::Text("x".into()), Type::Text),
            (Value::Bytes(b"y".to_vec().into_boxed_slice()), Type::Bytes),
            (Value::Blob(ContentId::of_blob(b"z")), Type::Blob),
            (
                Value::Nominal {
                    name: "test.MachineId".into(),
                    representation: Box::new(Value::Text("m-1".into())),
                },
                declared_nominal("test", "MachineId", Type::Text),
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
                        payload: Value::int(1),
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
            declared_nominal("test", "MachineId", Type::Text),
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
                name: "test.Object".into(),
                representation: Box::new(Value::Blob(ContentId::of_blob(b"o"))),
            }]
            .into_boxed_slice(),
        );
        let blobs = Value::List(vec![Value::Blob(ContentId::of_blob(b"o"))].into_boxed_slice());

        assert!(texts.is_type(&Type::List(Box::new(Type::Text))));
        assert!(!texts.is_type(&Type::List(Box::new(Type::Int))));
        assert!(objects.is_type(&Type::List(Box::new(declared_nominal(
            "test",
            "Object",
            Type::Blob
        )))));
        assert!(!objects.is_type(&Type::List(Box::new(Type::Blob))));
        assert!(!blobs.is_type(&Type::List(Box::new(declared_nominal(
            "test",
            "Object",
            Type::Blob
        )))));
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
            name: "test.MachineId".into(),
            representation: Box::new(Value::Text("m-1".into())),
        };
        assert!(machine_id.is_type(&declared_nominal("test", "MachineId", Type::Text)));
        assert!(!machine_id.is_type(&Type::Text));
        assert!(!machine_id.is_type(&declared_nominal("test", "OtherId", Type::Text)));
    }

    #[test]
    fn record_construction_orders_fields_and_rejects_duplicates() {
        // Canonical order is name order (0026), so construction order cannot
        // reach equality, hashing, or the encoding. A repeated name has no
        // meaning in a closed record.
        let first = Value::record([
            RecordField {
                name: "b".into(),
                payload: Value::int(1),
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
                payload: Value::int(1),
            },
        ])
        .unwrap();
        assert_eq!(first, second);

        let duplicate = Value::record([
            RecordField {
                name: "a".into(),
                payload: Value::int(1),
            },
            RecordField {
                name: "a".into(),
                payload: Value::int(2),
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
                payload: Value::int(1),
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
    fn a_record_inherits_the_asymmetry_of_its_fields() {
        // The agreement test's record pairing uses fully-determined payloads
        // — Text, Int — which is the one case where agreement holds. Nested
        // ambiguity breaks it, so the matrix's claim cannot include these
        // values. What holds everywhere is reflexivity: is_type accepts each
        // value against its own value_type.
        let source_sum = declared_sum(
            "test",
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
        );
        let empty_list_field = Value::record([RecordField {
            name: "f".into(),
            payload: Value::List(vec![].into_boxed_slice()),
        }])
        .unwrap();
        let sum_field = Value::record([RecordField {
            name: "s".into(),
            payload: Value::Sum {
                type_name: "test.Source".into(),
                constructor: "Git".into(),
                payload: Some(Box::new(Value::Text("main".into()))),
            },
        }])
        .unwrap();

        // The empty-list field: the value inhabits field types its
        // best-effort type does not name.
        let best_effort_list = empty_list_field.value_type();
        assert_eq!(
            best_effort_list,
            Type::record([RecordField {
                name: "f".into(),
                payload: Type::List(Box::new(Type::Unit)),
            }])
            .unwrap()
        );
        assert!(
            empty_list_field.is_type(
                &Type::record([RecordField {
                    name: "f".into(),
                    payload: Type::List(Box::new(Type::Text)),
                }])
                .unwrap()
            )
        );
        assert!(
            empty_list_field.is_type(
                &Type::record([RecordField {
                    name: "f".into(),
                    payload: Type::List(Box::new(Type::Int)),
                }])
                .unwrap()
            )
        );

        // The sum field: the value inhabits the declared sum its best-effort
        // type collapses to a singleton of.
        let best_effort_sum = sum_field.value_type();
        let declared_field = Type::record([RecordField {
            name: "s".into(),
            payload: source_sum,
        }])
        .unwrap();
        assert_ne!(best_effort_sum, declared_field);
        assert!(sum_field.is_type(&declared_field));

        assert!(empty_list_field.is_type(&best_effort_list));
        assert!(sum_field.is_type(&best_effort_sum));

        // A sum whose payload is an empty list inherits both exceptions at
        // once: value_type names the singleton holding `List<Unit>`, while
        // the value inhabits every declared sum carrying the constructor with
        // a `List<Text>` or `List<Int>` payload.
        let empty_list_payload = Value::Sum {
            type_name: "test.Source".into(),
            constructor: "Archive".into(),
            payload: Some(Box::new(Value::List(vec![].into_boxed_slice()))),
        };
        let best_effort_payload = empty_list_payload.value_type();
        assert_eq!(
            best_effort_payload,
            declared_sum(
                "test",
                "Source",
                [SumConstructor {
                    name: "Archive".into(),
                    payload: Some(Type::List(Box::new(Type::Unit))),
                }],
            )
        );
        for element in [Type::Text, Type::Int] {
            assert!(empty_list_payload.is_type(&declared_sum(
                "test",
                "Source",
                [
                    SumConstructor {
                        name: "Archive".into(),
                        payload: Some(Type::List(Box::new(element.clone()))),
                    },
                    SumConstructor {
                        name: "Git".into(),
                        payload: Some(Type::Text),
                    },
                ],
            )));
        }
        assert!(empty_list_payload.is_type(&best_effort_payload));
    }

    #[test]
    fn a_sum_value_inhabits_the_declared_sums_that_contain_its_constructor() {
        // A sum value knows its own constructor and payload but not its
        // siblings, so inhabitation is membership: the declared name must
        // match, the constructor must be in the declared set, and the payload
        // must be present exactly when the declaration carries one and inhabit
        // the declared payload type.
        let source = declared_sum(
            "test",
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
        );
        let git = Value::Sum {
            type_name: "test.Source".into(),
            constructor: "Git".into(),
            payload: Some(Box::new(Value::Text("main".into()))),
        };
        let path = Value::Sum {
            type_name: "test.Source".into(),
            constructor: "Path".into(),
            payload: None,
        };
        let renamed = declared_sum(
            "test",
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
        );
        let without_git = declared_sum(
            "test",
            "Source",
            [SumConstructor {
                name: "Path".into(),
                payload: None,
            }],
        );
        let mistyped = declared_sum(
            "test",
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
        );
        let bare_git = declared_sum(
            "test",
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
        );

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
        let first = declared_sum(
            "test",
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
        );
        let second = declared_sum(
            "test",
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
        );
        assert_eq!(first, second);

        // A repeated constructor is refused where a sum is now declared: the
        // table, not a free constructor (decision 0047).
        let duplicate = crate::declaration::DeclarationTable::new("test").sum(
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
            duplicate,
            Err(crate::declaration::DeclarationError::DuplicateName {
                module: "test".into(),
                name: "Git".into(),
            })
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
        let declared = declared_sum(
            "test",
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
        );
        let git = Value::Sum {
            type_name: "test.Source".into(),
            constructor: "Git".into(),
            payload: Some(Box::new(Value::Text("main".into()))),
        };

        let best_effort = git.value_type();
        assert_eq!(
            best_effort,
            declared_sum(
                "test",
                "Source",
                [SumConstructor {
                    name: "Git".into(),
                    payload: Some(Type::Text),
                }]
            )
        );
        assert_ne!(best_effort, declared);
        assert!(git.is_type(&best_effort));
        assert!(git.is_type(&declared));
    }
}

#[cfg(test)]
mod declaration_checks {
    use super::*;
    use crate::declaration::DeclarationTable;

    #[test]
    fn a_wrong_representation_no_longer_inhabits_a_nominal_type() {
        // The hole decision 0047 exists to close. Before the declaration
        // landed, `Type::Nominal` carried a bare name and matched any value
        // carrying the same string, so a `Text` masquerading as content
        // identity passed every check the kernel ran.
        let mut table = DeclarationTable::new("xylem");
        let object = table.nominal("Object", Type::Blob).unwrap();

        let genuine = Value::Nominal {
            name: "xylem.Object".into(),
            representation: Box::new(Value::Blob(ContentId::of_blob(b"real"))),
        };
        let fabricated = Value::Nominal {
            name: "xylem.Object".into(),
            representation: Box::new(Value::Text("not a content identity".into())),
        };

        assert!(genuine.is_type(&object));
        assert!(
            !fabricated.is_type(&object),
            "a value naming the coordinate with a wrong representation must be refused"
        );
        // And inside a list, which is where a link interface takes its objects.
        let inside = Type::List(Box::new(object));
        assert!(Value::List([genuine].into()).is_type(&inside));
        assert!(!Value::List([fabricated].into()).is_type(&inside));
    }

    #[test]
    fn a_recursive_declaration_checks_a_value_in_time_proportional_to_the_value() {
        // `test.Tree` over `List<Tree>`: the inner occurrence is the cut, so
        // the type is finite and a value of it terminates (0047).
        let mut table = DeclarationTable::new("test");
        let tree = table
            .nominal("Tree", Type::List(Box::new(Type::Cut)))
            .unwrap();

        let leaf = || Value::Nominal {
            name: "test.Tree".into(),
            representation: Box::new(Value::List([].into())),
        };
        let nested = Value::Nominal {
            name: "test.Tree".into(),
            representation: Box::new(Value::List(
                [
                    leaf(),
                    Value::Nominal {
                        name: "test.Tree".into(),
                        representation: Box::new(Value::List([leaf()].into())),
                    },
                ]
                .into(),
            )),
        };
        assert!(nested.is_type(&tree));

        // A wrong representation at depth is still refused, so the cut carries
        // the check down rather than waving it through.
        let corrupt = Value::Nominal {
            name: "test.Tree".into(),
            representation: Box::new(Value::List([Value::Text("not a tree".into())].into())),
        };
        assert!(!corrupt.is_type(&tree));
    }

    #[test]
    fn a_cut_outside_a_declaration_inhabits_nothing() {
        // There is no enclosing declaration to resolve against, so the check
        // answers false rather than guessing.
        assert!(!Value::Unit.is_type(&Type::Cut));
        assert!(!Value::List([Value::Unit].into()).is_type(&Type::List(Box::new(Type::Cut))));
        // An *empty* list still inhabits every `List<T>`, cut included, which is
        // the documented empty-list asymmetry rather than a cut resolving.
        assert!(Value::List([].into()).is_type(&Type::List(Box::new(Type::Cut))));
    }

    #[test]
    fn a_value_naming_another_modules_declaration_is_refused() {
        let mut xylem = DeclarationTable::new("xylem");
        let object = xylem.nominal("Object", Type::Blob).unwrap();
        let from_elsewhere = Value::Nominal {
            name: "phloem.Object".into(),
            representation: Box::new(Value::Blob(ContentId::of_blob(b"real"))),
        };
        assert!(!from_elsewhere.is_type(&object));
    }

    #[test]
    fn every_value_still_inhabits_its_own_value_type() {
        // Reflexivity, which the synthesized best-effort declaration has to
        // preserve for a nominal (0047).
        for value in [
            Value::Unit,
            Value::Nominal {
                name: "test.Id".into(),
                representation: Box::new(Value::Text("x".into())),
            },
            Value::Nominal {
                name: "bare".into(),
                representation: Box::new(Value::Blob(ContentId::of_blob(b"b"))),
            },
            Value::Sum {
                type_name: "test.Source".into(),
                constructor: "Git".into(),
                payload: Some(Box::new(Value::Text("main".into()))),
            },
        ] {
            let synthesized = value.value_type();
            assert!(
                value.is_type(&synthesized),
                "{value:?} did not inhabit its own value_type {synthesized:?}"
            );
        }
    }
}
