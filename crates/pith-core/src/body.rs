//! The represented rule body: a pure rule's computation as data (decisions
//! 0038, 0062).
//!
//! A host rule's body is Rust the kernel cannot read; a represented body is
//! this module's expression tree, elaborated and closed: typechecked against
//! its rule's interface, name-resolved into de Bruijn indices, and carrying
//! nothing a digest must not survive — no spans, no labels, no binder names.
//! The constructor set is fixed by decision 0062 and closed under 0038's
//! amendment rule: a new constructor arrives by record, not by addition.
//!
//! Totality is a property of the set rather than a check the validator runs:
//! there is no recursion constructor, repetition is a structural fold over a
//! finite list, and every primitive is total. A body can fail — `Fail` and
//! `TextOfBytes` are deterministic value failures — but it cannot diverge.

use pith_ids::BodyIrDigest;

use crate::rule::Interface;
use crate::value::{NominalType, RecordField, SumType, Type, Value};

/// How deeply a body may nest. Bounded so that validating and decoding a body
/// a file delivered cannot overflow the stack, on the same terms the value
/// codec bounds nominal nesting.
pub const MAX_BODY_DEPTH: u32 = 128;

/// A pure rule's body. The expression is checked against a rule's interface by
/// [`Self::validate`]; the inputs are the deepest binders and the expression's
/// type must be the interface's output.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RuleBody {
    expression: BodyExpr,
}

impl RuleBody {
    #[must_use]
    pub fn new(expression: BodyExpr) -> Self {
        Self { expression }
    }

    #[must_use]
    pub fn expression(&self) -> &BodyExpr {
        &self.expression
    }

    /// The body's revision half: a domain-separated digest over its canonical
    /// encoding, under the digest domain whose version is the body-encoding
    /// version (decisions 0038, 0062).
    #[must_use]
    pub fn digest(&self) -> BodyIrDigest {
        BodyIrDigest::of_manifest(&self.encode_canonical())
    }

    /// Check the body against `interface`: every binder is resolved, every
    /// expression is typed, every match is exhaustive, and the expression
    /// inhabits the interface's output type.
    ///
    /// # Errors
    /// [`BodyError`] naming the first refusal.
    pub fn validate(&self, interface: &Interface) -> Result<(), BodyError> {
        let mut binders = Vec::with_capacity(interface.inputs.len());
        for input in &interface.inputs {
            binders.push(Inferred::Typed(input.clone()));
        }
        match infer(&self.expression, &mut binders, 0)? {
            Inferred::Bottom => Ok(()),
            Inferred::Typed(found) if found == interface.output => Ok(()),
            Inferred::Typed(found) => Err(BodyError::OutputTypeMismatch {
                expected: interface.output.clone(),
                found,
            }),
        }
    }
}

/// One step a represented body yields to, plus the pure expression language
/// between the yields.
///
/// Binders are de Bruijn indices: `Bound(0)` is the most recently bound value.
/// A body's inputs are bound in interface order before it runs, so the last
/// input is `Bound(0)`. A request's resumption binds its output at `Bound(0)`;
/// `NeedAll` binds one binder per request in request order, so the first
/// request's result is `Bound(0)`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BodyExpr {
    /// An embedded value. Construction that must name a declaration — a sum
    /// constructor, a nominal wrapper, an empty list — belongs to the
    /// constructors below, which carry the annotation a literal cannot
    /// recover.
    Literal(Value),
    /// The binder `index` steps out from the innermost.
    Bound(usize),
    /// Bind `bound`'s value for `rest`.
    Let {
        bound: Box<BodyExpr>,
        rest: Box<BodyExpr>,
    },
    /// Fail the body with `message`, which inhabits `Text`. The evaluator
    /// wraps the message in the represented-body diagnostic; the construct
    /// inhabits every type, the way a value-level failure must.
    Fail { message: Box<BodyExpr> },
    /// Build a record from named field expressions, in canonical name order.
    Record {
        fields: Box<[RecordField<BodyExpr>]>,
    },
    /// Read one named field.
    Field {
        record: Box<BodyExpr>,
        name: Box<str>,
    },
    /// Build one constructor of a declared sum. The declaration travels in the
    /// expression rather than being resolved through a table, on the ground
    /// 0047 fixed for types.
    MakeSum {
        declared: SumType,
        constructor: Box<str>,
        payload: Option<Box<BodyExpr>>,
    },
    /// Eliminate a declared sum. The arms cover the scrutinee's constructors
    /// exactly, in canonical name order; an arm over a constructor carrying a
    /// payload runs under one binder for it.
    Match {
        scrutinee: Box<BodyExpr>,
        arms: Box<[MatchArm]>,
    },
    /// Wrap a representation in a declared nominal type.
    Wrap {
        declared: NominalType,
        representation: Box<BodyExpr>,
    },
    /// Unwrap a nominal value to its representation. The nominal's own type
    /// names the declaration, so no annotation rides along.
    Unwrap { nominal: Box<BodyExpr> },
    /// Build a list with its element type, which an empty list cannot recover
    /// from its items.
    List {
        element: Type,
        items: Box<[BodyExpr]>,
    },
    /// Prepend one element to a list.
    Cons {
        head: Box<BodyExpr>,
        tail: Box<BodyExpr>,
    },
    /// Structural case on a list: `empty` when it holds nothing, `cons` with
    /// the head at `Bound(0)` and the tail at `Bound(1)`. Grouping and
    /// first-match have no total spelling under folds alone — a fold's
    /// accumulator starts at a value, and the value a data-dependent start
    /// would need does not exist — so the two list constructors get the
    /// eliminator the sum constructors already have.
    MatchList {
        list: Box<BodyExpr>,
        empty: Box<BodyExpr>,
        cons: Box<BodyExpr>,
    },
    /// Concatenate two lists of one element type.
    Append {
        left: Box<BodyExpr>,
        right: Box<BodyExpr>,
    },
    /// Fold `source` left to right. `init` is the accumulator's first value;
    /// `step` runs with the element at `Bound(0)` and the accumulator at
    /// `Bound(1)`, and returns the accumulator's next value.
    Fold {
        source: Box<BodyExpr>,
        init: Box<BodyExpr>,
        step: Box<BodyExpr>,
    },
    /// Sort a list by the canonical encoding of the key each element maps to.
    /// The sort is stable, so equal keys keep their source order and the
    /// result is a function of the input alone.
    SortBy {
        list: Box<BodyExpr>,
        key: Box<BodyExpr>,
    },
    /// `then` or `otherwise` by `condition`.
    If {
        condition: Box<BodyExpr>,
        then: Box<BodyExpr>,
        otherwise: Box<BodyExpr>,
    },
    /// Structural value equality.
    Equal {
        left: Box<BodyExpr>,
        right: Box<BodyExpr>,
    },
    /// The three total integer operations (decision 0055). Division stays out
    /// until a consumer can answer its zero case.
    IntAdd {
        left: Box<BodyExpr>,
        right: Box<BodyExpr>,
    },
    IntSubtract {
        left: Box<BodyExpr>,
        right: Box<BodyExpr>,
    },
    IntMultiply {
        left: Box<BodyExpr>,
        right: Box<BodyExpr>,
    },
    /// Render any value as text, the diagnostic rendering: integers in
    /// decimal, blobs by their digest. This is how a body names what it
    /// refuses.
    Describe { value: Box<BodyExpr> },
    /// Concatenate two texts.
    TextConcat {
        left: Box<BodyExpr>,
        right: Box<BodyExpr>,
    },
    /// Decode bytes as UTF-8 text. Invalid bytes fail the body; the failure
    /// is a value, deterministic in the bytes, not a divergence.
    TextOfBytes { bytes: Box<BodyExpr> },
    /// Request one pure computation and continue under its result.
    Need {
        request: BodyRequest,
        resume: Box<BodyExpr>,
    },
    /// Request a batch of pure computations that do not depend on one another
    /// (decision 0029), and continue under one binder per request in request
    /// order. The batch is fixed by the body, so its requests may name
    /// different interfaces.
    NeedAll {
        requests: Box<[BodyRequest]>,
        resume: Box<BodyExpr>,
    },
    /// Request one computation per element of `source`, with the declared
    /// independence of a batch (decision 0029). The interface is fixed by the
    /// body and the count is data: the fan-out shape a body cannot spell as a
    /// static batch. `request` is built with the element at `Bound(0)`; the
    /// resumption binds the results list in source order.
    NeedEach {
        source: Box<BodyExpr>,
        request: BodyRequest,
        resume: Box<BodyExpr>,
    },
    /// Materialize content and continue under its bytes.
    NeedBlob {
        content: Box<BodyExpr>,
        resume: Box<BodyExpr>,
    },
    /// Request an action computation. The action rule that serves the request
    /// stays host-tier; what is represented is this body's request and its
    /// continuation.
    NeedAction {
        request: BodyRequest,
        resume: Box<BodyExpr>,
    },
    /// Request an observation (decision 0060).
    NeedObservation {
        request: BodyRequest,
        resume: Box<BodyExpr>,
    },
}

impl BodyExpr {
    /// Build a record expression, sorting the fields into canonical name
    /// order.
    ///
    /// # Errors
    /// [`crate::DuplicateNameError`] when two fields share a name.
    pub fn record(
        fields: impl Into<Box<[RecordField<BodyExpr>]>>,
    ) -> Result<Self, crate::DuplicateNameError> {
        let mut fields = fields.into();
        fields.sort_by(|left, right| left.name.cmp(&right.name));
        for [earlier, later] in fields.array_windows::<2>() {
            if earlier.name == later.name {
                return Err(crate::DuplicateNameError {
                    name: earlier.name.clone(),
                });
            }
        }
        Ok(Self::Record { fields })
    }
}

/// One arm of a [`BodyExpr::Match`]: the constructor it covers and the
/// expression it evaluates, under one binder when the constructor carries a
/// payload.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MatchArm {
    pub constructor: Box<str>,
    pub body: Box<BodyExpr>,
}

/// One request a yield construct makes: the interface selection reads and the
/// inputs it consumes. The interface is written in the body — there is no
/// ambient rule table to resolve against — so a request is self-describing
/// data the way a type reference is (decision 0047).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BodyRequest {
    pub interface: Interface,
    pub inputs: Box<[BodyExpr]>,
}

/// The type an expression synthesizes. `Fail` inhabits every type, so
/// synthesis has a bottom element and the places that must compare types
/// unify through it rather than refusing it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Inferred {
    Bottom,
    Typed(Type),
}

/// Why a body does not check against its rule's interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BodyError {
    OutputTypeMismatch {
        expected: Type,
        found: Type,
    },
    TypeMismatch {
        expected: Type,
        found: Type,
    },
    UnboundVariable {
        index: usize,
    },
    NotASum {
        found: Type,
    },
    NotARecord {
        found: Type,
    },
    NotANominal {
        found: Type,
    },
    NotAList {
        found: Type,
    },
    UnknownField {
        record: Type,
        field: Box<str>,
    },
    UnknownConstructor {
        sum: Box<str>,
        constructor: Box<str>,
    },
    PayloadPresenceMismatch {
        sum: Box<str>,
        constructor: Box<str>,
    },
    NonExhaustiveMatch {
        sum: Box<str>,
        missing: Box<[Box<str>]>,
    },
    UnknownArm {
        sum: Box<str>,
        constructor: Box<str>,
    },
    ArmsOutOfOrder {
        earlier: Box<str>,
        later: Box<str>,
    },
    FieldsOutOfOrder {
        earlier: Box<str>,
        later: Box<str>,
    },
    RequestArityMismatch {
        expected: usize,
        provided: usize,
    },
    EmptyNeedAll,
    DepthExceeded {
        limit: u32,
    },
}

impl std::fmt::Display for BodyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutputTypeMismatch { expected, found } => {
                write!(
                    f,
                    "the body produces {found}, but the rule declares {expected}"
                )
            }
            Self::TypeMismatch { expected, found } => {
                write!(f, "expected {expected}, found {found}")
            }
            Self::UnboundVariable { index } => {
                write!(f, "no binder is {index} steps out from here")
            }
            Self::NotASum { found } => write!(f, "expected a declared sum, found {found}"),
            Self::NotARecord { found } => write!(f, "expected a record, found {found}"),
            Self::NotANominal { found } => write!(f, "expected a nominal type, found {found}"),
            Self::NotAList { found } => write!(f, "expected a list, found {found}"),
            Self::UnknownField { record, field } => {
                write!(f, "the record type {record} has no field `{field}`")
            }
            Self::UnknownConstructor { sum, constructor } => {
                write!(f, "the sum {sum} declares no constructor `{constructor}`")
            }
            Self::PayloadPresenceMismatch { sum, constructor } => write!(
                f,
                "the payload given for `{constructor}` of {sum} disagrees with the constructor's \
                 declaration"
            ),
            Self::NonExhaustiveMatch { sum, missing } => {
                let missing: Vec<&str> = missing
                    .iter()
                    .map(|constructor| constructor.as_ref())
                    .collect();
                write!(
                    f,
                    "the match over {sum} does not cover {}",
                    missing.join(", ")
                )
            }
            Self::UnknownArm { sum, constructor } => write!(
                f,
                "`{constructor}` is not a constructor of the sum {sum} being matched"
            ),
            Self::ArmsOutOfOrder { earlier, later } => write!(
                f,
                "match arms must ascend by constructor name: `{earlier}` then `{later}`"
            ),
            Self::FieldsOutOfOrder { earlier, later } => write!(
                f,
                "record fields must ascend by name: `{earlier}` then `{later}`"
            ),
            Self::RequestArityMismatch { expected, provided } => write!(
                f,
                "the request's interface takes {expected} inputs but the body provides {provided}"
            ),
            Self::EmptyNeedAll => {
                f.write_str("a NeedAll of zero requests binds nothing and yields nothing")
            }
            Self::DepthExceeded { limit } => {
                write!(f, "the body nests deeper than {limit}")
            }
        }
    }
}

impl std::error::Error for BodyError {}

fn infer(
    expression: &BodyExpr,
    binders: &mut Vec<Inferred>,
    depth: u32,
) -> Result<Inferred, BodyError> {
    if depth >= MAX_BODY_DEPTH {
        return Err(BodyError::DepthExceeded {
            limit: MAX_BODY_DEPTH,
        });
    }
    let deeper = depth.saturating_add(1);
    match expression {
        BodyExpr::Literal(value) => Ok(Inferred::Typed(value.value_type())),
        BodyExpr::Bound(index) => binders
            .iter()
            .rev()
            .nth(*index)
            .cloned()
            .ok_or(BodyError::UnboundVariable { index: *index }),
        BodyExpr::Let { bound, rest } => {
            let bound_type = infer(bound, binders, deeper)?;
            binders.push(bound_type);
            let rest_type = infer(rest, binders, deeper);
            binders.pop();
            rest_type
        }
        BodyExpr::Fail { message } => {
            expect(infer(message, binders, deeper)?, &Type::Text)?;
            Ok(Inferred::Bottom)
        }
        BodyExpr::Record { fields } => infer_record(fields, binders, deeper),
        BodyExpr::Field { record, name } => infer_field(record, name, binders, deeper),
        BodyExpr::MakeSum {
            declared,
            constructor,
            payload,
        } => infer_make_sum(declared, constructor, payload, binders, deeper),
        BodyExpr::Match { scrutinee, arms } => match infer(scrutinee, binders, deeper)? {
            Inferred::Bottom => Ok(Inferred::Bottom),
            Inferred::Typed(Type::Sum(sum)) => infer_match(&sum, arms, binders, deeper),
            Inferred::Typed(found) => Err(BodyError::NotASum { found }),
        },
        BodyExpr::Wrap {
            declared,
            representation,
        } => infer_wrap(declared, representation, binders, deeper),
        BodyExpr::Unwrap { nominal } => match infer(nominal, binders, deeper)? {
            Inferred::Bottom => Ok(Inferred::Bottom),
            Inferred::Typed(Type::Nominal(declared)) => {
                Ok(Inferred::Typed(declared.representation.clone()))
            }
            Inferred::Typed(found) => Err(BodyError::NotANominal { found }),
        },
        BodyExpr::List { element, items } => infer_list(element, items, binders, deeper),
        BodyExpr::Cons { head, tail } => infer_cons(head, tail, binders, deeper),
        BodyExpr::MatchList { list, empty, cons } => {
            infer_match_list(list, empty, cons, binders, deeper)
        }
        BodyExpr::Append { left, right } => infer_append(left, right, binders, deeper),
        BodyExpr::Fold { source, init, step } => infer_fold(source, init, step, binders, deeper),
        BodyExpr::SortBy { list, key } => infer_sort_by(list, key, binders, deeper),
        BodyExpr::If {
            condition,
            then,
            otherwise,
        } => infer_if(condition, then, otherwise, binders, deeper),
        BodyExpr::Equal { left, right } => {
            unify(
                &infer(left, binders, deeper)?,
                &infer(right, binders, deeper)?,
            )?;
            Ok(Inferred::Typed(Type::Bool))
        }
        BodyExpr::IntAdd { left, right }
        | BodyExpr::IntSubtract { left, right }
        | BodyExpr::IntMultiply { left, right } => {
            expect(infer(left, binders, deeper)?, &Type::Int)?;
            expect(infer(right, binders, deeper)?, &Type::Int)?;
            Ok(Inferred::Typed(Type::Int))
        }
        BodyExpr::Describe { value } => {
            infer(value, binders, deeper)?;
            Ok(Inferred::Typed(Type::Text))
        }
        BodyExpr::TextConcat { left, right } => {
            expect(infer(left, binders, deeper)?, &Type::Text)?;
            expect(infer(right, binders, deeper)?, &Type::Text)?;
            Ok(Inferred::Typed(Type::Text))
        }
        BodyExpr::TextOfBytes { bytes } => {
            expect(infer(bytes, binders, deeper)?, &Type::Bytes)?;
            Ok(Inferred::Typed(Type::Text))
        }
        BodyExpr::Need { .. }
        | BodyExpr::NeedAll { .. }
        | BodyExpr::NeedEach { .. }
        | BodyExpr::NeedBlob { .. }
        | BodyExpr::NeedAction { .. }
        | BodyExpr::NeedObservation { .. } => infer_yield(expression, binders, deeper),
    }
}

fn infer_field(
    record: &BodyExpr,
    name: &str,
    binders: &mut Vec<Inferred>,
    depth: u32,
) -> Result<Inferred, BodyError> {
    match infer(record, binders, depth)? {
        Inferred::Bottom => Ok(Inferred::Bottom),
        Inferred::Typed(Type::Record(fields)) => {
            match fields.iter().find(|field| field.name.as_ref() == name) {
                Some(field) => Ok(Inferred::Typed(field.payload.clone())),
                None => Err(BodyError::UnknownField {
                    record: Type::Record(fields),
                    field: name.into(),
                }),
            }
        }
        Inferred::Typed(found) => Err(BodyError::NotARecord { found }),
    }
}

fn infer_make_sum(
    declared: &SumType,
    constructor: &str,
    payload: &Option<Box<BodyExpr>>,
    binders: &mut Vec<Inferred>,
    depth: u32,
) -> Result<Inferred, BodyError> {
    let found = declared
        .constructors
        .iter()
        .find(|candidate| candidate.name.as_ref() == constructor);
    match found {
        Some(candidate) if candidate.payload.is_some() == payload.is_some() => {
            if let (Some(declared_payload), Some(payload)) = (&candidate.payload, payload) {
                expect(infer(payload, binders, depth)?, declared_payload)?;
            }
            Ok(Inferred::Typed(Type::Sum(Box::new(declared.clone()))))
        }
        Some(_) => Err(BodyError::PayloadPresenceMismatch {
            sum: declared.coordinate.spelling().into(),
            constructor: constructor.into(),
        }),
        None => Err(BodyError::UnknownConstructor {
            sum: declared.coordinate.spelling().into(),
            constructor: constructor.into(),
        }),
    }
}

fn infer_wrap(
    declared: &NominalType,
    representation: &BodyExpr,
    binders: &mut Vec<Inferred>,
    depth: u32,
) -> Result<Inferred, BodyError> {
    expect(
        infer(representation, binders, depth)?,
        &declared.representation,
    )?;
    Ok(Inferred::Typed(Type::Nominal(Box::new(declared.clone()))))
}

fn infer_list(
    element: &Type,
    items: &[BodyExpr],
    binders: &mut Vec<Inferred>,
    depth: u32,
) -> Result<Inferred, BodyError> {
    for item in items {
        expect(infer(item, binders, depth)?, element)?;
    }
    Ok(Inferred::Typed(Type::List(Box::new(element.clone()))))
}

fn infer_cons(
    head: &BodyExpr,
    tail: &BodyExpr,
    binders: &mut Vec<Inferred>,
    depth: u32,
) -> Result<Inferred, BodyError> {
    let head = infer(head, binders, depth)?;
    let tail = infer(tail, binders, depth)?;
    let element = match head {
        Inferred::Bottom => element_of(tail.clone())?,
        Inferred::Typed(element) => Inferred::Typed(element),
    };
    unify(&tail, &list_of(element))
}

fn infer_match_list(
    list: &BodyExpr,
    empty: &BodyExpr,
    cons: &BodyExpr,
    binders: &mut Vec<Inferred>,
    depth: u32,
) -> Result<Inferred, BodyError> {
    let element = element_of(infer(list, binders, depth)?)?;
    let empty_type = infer(empty, binders, depth)?;
    binders.push(list_of(element.clone()));
    binders.push(element);
    let cons_type = infer(cons, binders, depth);
    binders.pop();
    binders.pop();
    unify(&empty_type, &cons_type?)
}

fn infer_append(
    left: &BodyExpr,
    right: &BodyExpr,
    binders: &mut Vec<Inferred>,
    depth: u32,
) -> Result<Inferred, BodyError> {
    let unified = unify(
        &infer(left, binders, depth)?,
        &infer(right, binders, depth)?,
    )?;
    match unified {
        Inferred::Bottom => Ok(Inferred::Bottom),
        Inferred::Typed(list @ Type::List(_)) => Ok(Inferred::Typed(list)),
        Inferred::Typed(found) => Err(BodyError::NotAList { found }),
    }
}

fn infer_fold(
    source: &BodyExpr,
    init: &BodyExpr,
    step: &BodyExpr,
    binders: &mut Vec<Inferred>,
    depth: u32,
) -> Result<Inferred, BodyError> {
    let element = element_of(infer(source, binders, depth)?)?;
    let accumulator = infer(init, binders, depth)?;
    binders.push(accumulator.clone());
    binders.push(element);
    let step_type = infer(step, binders, depth);
    binders.pop();
    binders.pop();
    unify(&accumulator, &step_type?)
}

fn infer_sort_by(
    list: &BodyExpr,
    key: &BodyExpr,
    binders: &mut Vec<Inferred>,
    depth: u32,
) -> Result<Inferred, BodyError> {
    let list_type = infer(list, binders, depth)?;
    binders.push(element_of(list_type.clone())?);
    let key_type = infer(key, binders, depth);
    binders.pop();
    key_type?;
    Ok(list_type)
}

fn infer_if(
    condition: &BodyExpr,
    then: &BodyExpr,
    otherwise: &BodyExpr,
    binders: &mut Vec<Inferred>,
    depth: u32,
) -> Result<Inferred, BodyError> {
    expect(infer(condition, binders, depth)?, &Type::Bool)?;
    unify(
        &infer(then, binders, depth)?,
        &infer(otherwise, binders, depth)?,
    )
}

/// The yield constructs share one shape: check the request, then check the
/// continuation under the values the engine resumes it with.
fn infer_yield(
    expression: &BodyExpr,
    binders: &mut Vec<Inferred>,
    depth: u32,
) -> Result<Inferred, BodyError> {
    match expression {
        BodyExpr::Need { request, resume } => {
            check_request(request, binders, depth)?;
            resume_under(resume, binders, depth, &[output_of(request)])
        }
        BodyExpr::NeedAll { requests, resume } => {
            if requests.is_empty() {
                return Err(BodyError::EmptyNeedAll);
            }
            let mut outputs = Vec::with_capacity(requests.len());
            for request in requests.iter() {
                check_request(request, binders, depth)?;
                outputs.push(output_of(request));
            }
            // Bound(0) is the first request's result, so the outputs are
            // pushed in reverse.
            outputs.reverse();
            resume_under(resume, binders, depth, &outputs)
        }
        BodyExpr::NeedEach {
            source,
            request,
            resume,
        } => {
            let element = element_of(infer(source, binders, depth)?)?;
            binders.push(element);
            let checked = check_request(request, binders, depth);
            binders.pop();
            checked?;
            resume_under(resume, binders, depth, &[list_of(output_of(request))])
        }
        BodyExpr::NeedBlob { content, resume } => {
            expect(infer(content, binders, depth)?, &Type::Blob)?;
            resume_under(resume, binders, depth, &[Inferred::Typed(Type::Bytes)])
        }
        BodyExpr::NeedAction { request, resume }
        | BodyExpr::NeedObservation { request, resume } => {
            check_request(request, binders, depth)?;
            resume_under(resume, binders, depth, &[output_of(request)])
        }
        // The dispatcher routes only the yield constructs here.
        _ => unreachable!("infer_yield is reached only through a yield construct"),
    }
}

fn output_of(request: &BodyRequest) -> Inferred {
    Inferred::Typed(request.interface.output.clone())
}

fn infer_record(
    fields: &[RecordField<BodyExpr>],
    binders: &mut Vec<Inferred>,
    depth: u32,
) -> Result<Inferred, BodyError> {
    for [earlier, later] in fields.array_windows::<2>() {
        if earlier.name.as_ref() >= later.name.as_ref() {
            return Err(BodyError::FieldsOutOfOrder {
                earlier: earlier.name.clone(),
                later: later.name.clone(),
            });
        }
    }
    let mut typed: Vec<RecordField<Type>> = Vec::with_capacity(fields.len());
    for field in fields {
        // A failing field expression contributes the `Unit` a best-effort
        // field type must name; the record it sits in will never be built.
        let payload = match infer(&field.payload, binders, depth)? {
            Inferred::Bottom => Type::Unit,
            Inferred::Typed(payload) => payload,
        };
        typed.push(RecordField {
            name: field.name.clone(),
            payload,
        });
    }
    Ok(Inferred::Typed(Type::Record(typed.into())))
}

fn infer_match(
    sum: &SumType,
    arms: &[MatchArm],
    binders: &mut Vec<Inferred>,
    depth: u32,
) -> Result<Inferred, BodyError> {
    let spelling = sum.coordinate.spelling();
    for [earlier, later] in arms.array_windows::<2>() {
        if earlier.constructor.as_ref() >= later.constructor.as_ref() {
            return Err(BodyError::ArmsOutOfOrder {
                earlier: earlier.constructor.clone(),
                later: later.constructor.clone(),
            });
        }
    }
    let declared: Vec<&str> = sum
        .constructors
        .iter()
        .map(|constructor| constructor.name.as_ref())
        .collect();
    let covered: Vec<&str> = arms.iter().map(|arm| arm.constructor.as_ref()).collect();
    if let Some(constructor) = covered
        .iter()
        .find(|constructor| !declared.contains(constructor))
    {
        return Err(BodyError::UnknownArm {
            sum: spelling.into(),
            constructor: (**constructor).into(),
        });
    }
    let missing: Box<[Box<str>]> = declared
        .iter()
        .filter(|constructor| !covered.contains(constructor))
        .map(|constructor| (**constructor).into())
        .collect();
    if !missing.is_empty() {
        return Err(BodyError::NonExhaustiveMatch {
            sum: spelling.into(),
            missing,
        });
    }
    let mut result: Option<Inferred> = None;
    for arm in arms {
        let payload = sum
            .constructors
            .iter()
            .find(|constructor| constructor.name == arm.constructor)
            .and_then(|constructor| constructor.payload.clone());
        let arm_type = match payload {
            Some(payload) => {
                binders.push(Inferred::Typed(payload));
                let arm_type = infer(&arm.body, binders, depth);
                binders.pop();
                arm_type?
            }
            None => infer(&arm.body, binders, depth)?,
        };
        result = match result {
            None | Some(Inferred::Bottom) => Some(arm_type),
            Some(result) => {
                unify(&result, &arm_type)?;
                Some(result)
            }
        };
    }
    Ok(result.map_or(Inferred::Bottom, std::convert::identity))
}

fn check_request(
    request: &BodyRequest,
    binders: &mut Vec<Inferred>,
    depth: u32,
) -> Result<(), BodyError> {
    if request.inputs.len() != request.interface.inputs.len() {
        return Err(BodyError::RequestArityMismatch {
            expected: request.interface.inputs.len(),
            provided: request.inputs.len(),
        });
    }
    for (input, expected) in request.inputs.iter().zip(request.interface.inputs.iter()) {
        expect(infer(input, binders, depth)?, expected)?;
    }
    Ok(())
}

fn resume_under(
    resume: &BodyExpr,
    binders: &mut Vec<Inferred>,
    depth: u32,
    resumed: &[Inferred],
) -> Result<Inferred, BodyError> {
    binders.extend(resumed.iter().cloned());
    let resumed_type = infer(resume, binders, depth);
    for _ in resumed {
        binders.pop();
    }
    resumed_type
}

fn expect(inferred: Inferred, expected: &Type) -> Result<(), BodyError> {
    match inferred {
        Inferred::Bottom => Ok(()),
        Inferred::Typed(found) if &found == expected => Ok(()),
        Inferred::Typed(found) => Err(BodyError::TypeMismatch {
            expected: expected.clone(),
            found,
        }),
    }
}

/// The element type of a list expression. A failing list contributes a bottom
/// element, which inhabits every type the way `Fail` does.
fn element_of(inferred: Inferred) -> Result<Inferred, BodyError> {
    match inferred {
        Inferred::Bottom => Ok(Inferred::Bottom),
        Inferred::Typed(Type::List(element)) => Ok(Inferred::Typed(*element)),
        Inferred::Typed(found) => Err(BodyError::NotAList { found }),
    }
}

fn list_of(element: Inferred) -> Inferred {
    match element {
        Inferred::Bottom => Inferred::Bottom,
        Inferred::Typed(element) => Inferred::Typed(Type::List(Box::new(element))),
    }
}

fn unify(left: &Inferred, right: &Inferred) -> Result<Inferred, BodyError> {
    match (left, right) {
        (Inferred::Bottom, other) | (other, Inferred::Bottom) => Ok(other.clone()),
        (Inferred::Typed(left), Inferred::Typed(right)) if left == right => {
            Ok(Inferred::Typed(left.clone()))
        }
        (Inferred::Typed(left), Inferred::Typed(right)) => Err(BodyError::TypeMismatch {
            expected: left.clone(),
            found: right.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::Interface;
    use crate::value::{SumConstructor, declared_nominal, declared_sum};

    fn first_input(interface: &Interface) -> Type {
        interface
            .inputs
            .first()
            .cloned()
            .unwrap_or_else(|| unreachable!("the test declares one input"))
    }

    fn passthrough(interface: Interface) -> RuleBody {
        let inputs = interface.inputs.len();
        RuleBody::new(BodyExpr::Bound(inputs.saturating_sub(1)))
    }

    #[test]
    fn a_body_that_returns_its_first_input_validates() {
        let interface = Interface {
            inputs: Box::new([Type::Int, Type::Text]),
            output: Type::Int,
        };
        // Two inputs, bound in interface order: the last input is `Bound(0)`,
        // so the first input is one step further out.
        let body = RuleBody::new(BodyExpr::Bound(1));
        assert_eq!(body.validate(&interface), Ok(()));
        assert_eq!(passthrough(interface.clone()).validate(&interface), Ok(()));
    }

    #[test]
    fn a_body_of_the_wrong_output_type_is_refused() {
        let interface = Interface {
            inputs: Box::new([Type::Int]),
            output: Type::Text,
        };
        let body = RuleBody::new(BodyExpr::Bound(0));
        assert_eq!(
            body.validate(&interface),
            Err(BodyError::OutputTypeMismatch {
                expected: Type::Text,
                found: Type::Int,
            })
        );
    }

    #[test]
    fn a_binder_beyond_the_stack_is_refused() {
        let interface = Interface {
            inputs: Box::new([Type::Int]),
            output: Type::Int,
        };
        let body = RuleBody::new(BodyExpr::Bound(1));
        assert_eq!(
            body.validate(&interface),
            Err(BodyError::UnboundVariable { index: 1 })
        );
    }

    #[test]
    fn fail_inhabits_every_output_type() {
        let interface = Interface {
            inputs: Box::new([]),
            output: declared_nominal("test", "Thing", Type::Blob),
        };
        let body = RuleBody::new(BodyExpr::Fail {
            message: Box::new(BodyExpr::Literal(Value::Text("refused".into()))),
        });
        assert_eq!(body.validate(&interface), Ok(()));
    }

    #[test]
    fn a_match_must_cover_the_declared_constructors_in_order() {
        let sum = declared_sum(
            "test",
            "Shape",
            [
                SumConstructor {
                    name: "circle".into(),
                    payload: Some(Type::Int),
                },
                SumConstructor {
                    name: "square".into(),
                    payload: None,
                },
            ],
        );
        let interface = Interface {
            inputs: Box::new([sum.clone()]),
            output: Type::Text,
        };
        let arms = |constructors: &[&str]| {
            constructors
                .iter()
                .map(|constructor| MatchArm {
                    constructor: (*constructor).into(),
                    body: Box::new(BodyExpr::Literal(Value::Text("named".into()))),
                })
                .collect::<Box<[_]>>()
        };
        let matching = |arms| {
            RuleBody::new(BodyExpr::Match {
                scrutinee: Box::new(BodyExpr::Bound(0)),
                arms,
            })
        };

        assert_eq!(
            matching(arms(&["circle", "square"])).validate(&interface),
            Ok(())
        );
        assert_eq!(
            matching(arms(&["circle"])).validate(&interface),
            Err(BodyError::NonExhaustiveMatch {
                sum: "test.Shape".into(),
                missing: Box::new(["square".into()]),
            })
        );
        assert_eq!(
            matching(arms(&["square", "circle"])).validate(&interface),
            Err(BodyError::ArmsOutOfOrder {
                earlier: "square".into(),
                later: "circle".into(),
            })
        );
        assert_eq!(
            matching(arms(&["circle", "square", "triangle"])).validate(&interface),
            Err(BodyError::UnknownArm {
                sum: "test.Shape".into(),
                constructor: "triangle".into(),
            })
        );
    }

    #[test]
    fn a_match_binds_the_payload_of_the_constructor_it_covers() {
        let sum = declared_sum(
            "test",
            "Shape",
            [
                SumConstructor {
                    name: "circle".into(),
                    payload: Some(Type::Int),
                },
                SumConstructor {
                    name: "square".into(),
                    payload: None,
                },
            ],
        );
        let interface = Interface {
            inputs: Box::new([sum]),
            output: Type::Int,
        };
        let body = RuleBody::new(BodyExpr::Match {
            scrutinee: Box::new(BodyExpr::Bound(0)),
            arms: Box::new([
                MatchArm {
                    constructor: "circle".into(),
                    body: Box::new(BodyExpr::Bound(0)),
                },
                MatchArm {
                    constructor: "square".into(),
                    body: Box::new(BodyExpr::Literal(Value::int(0))),
                },
            ]),
        });
        assert_eq!(body.validate(&interface), Ok(()));
    }

    #[test]
    fn a_nominal_wraps_only_its_declared_representation() {
        let nominal = declared_nominal("test", "Object", Type::Blob);
        let interface = Interface {
            inputs: Box::new([Type::Blob]),
            output: nominal.clone(),
        };
        let wrapping = |representation| {
            RuleBody::new(BodyExpr::Wrap {
                declared: match nominal.clone() {
                    Type::Nominal(declared) => *declared,
                    other => unreachable!("the declared type is a nominal: {other}"),
                },
                representation: Box::new(representation),
            })
        };
        assert_eq!(
            wrapping(BodyExpr::Bound(0)).validate(&interface),
            Ok(()),
            "a blob inhabits the declared representation"
        );
        assert_eq!(
            wrapping(BodyExpr::Literal(Value::Text("path".into()))).validate(&interface),
            Err(BodyError::TypeMismatch {
                expected: Type::Blob,
                found: Type::Text,
            })
        );
    }

    #[test]
    fn unwrapping_a_nominal_yields_its_representation() {
        let nominal = declared_nominal("test", "Object", Type::Blob);
        let interface = Interface {
            inputs: Box::new([nominal.clone()]),
            output: Type::Blob,
        };
        let body = RuleBody::new(BodyExpr::Unwrap {
            nominal: Box::new(BodyExpr::Bound(0)),
        });
        assert_eq!(body.validate(&interface), Ok(()));

        let not_nominal = Interface {
            inputs: Box::new([Type::Text]),
            output: Type::Blob,
        };
        assert_eq!(
            body.validate(&not_nominal),
            Err(BodyError::NotANominal { found: Type::Text })
        );
    }

    #[test]
    fn a_missing_field_is_refused_with_the_record_type() {
        let interface = Interface {
            inputs: Box::new([Type::Record(Box::new([RecordField {
                name: "path".into(),
                payload: Type::Text,
            }]))]),
            output: Type::Text,
        };
        let body = RuleBody::new(BodyExpr::Field {
            record: Box::new(BodyExpr::Bound(0)),
            name: "content".into(),
        });
        assert_eq!(
            body.validate(&interface),
            Err(BodyError::UnknownField {
                record: first_input(&interface),
                field: "content".into(),
            })
        );
    }

    #[test]
    fn record_fields_must_ascend_by_name() {
        let interface = Interface {
            inputs: Box::new([]),
            output: Type::Record(Box::new([
                RecordField {
                    name: "a".into(),
                    payload: Type::Int,
                },
                RecordField {
                    name: "b".into(),
                    payload: Type::Int,
                },
            ])),
        };
        let fields = |names: [&str; 2]| {
            BodyExpr::record([
                RecordField {
                    name: names[0].into(),
                    payload: BodyExpr::Literal(Value::int(1)),
                },
                RecordField {
                    name: names[1].into(),
                    payload: BodyExpr::Literal(Value::int(2)),
                },
            ])
            .unwrap()
        };
        let body = RuleBody::new(fields(["a", "b"]));
        assert_eq!(body.validate(&interface), Ok(()));

        let out_of_order = BodyExpr::Record {
            fields: Box::new([
                RecordField {
                    name: "b".into(),
                    payload: BodyExpr::Literal(Value::int(2)),
                },
                RecordField {
                    name: "a".into(),
                    payload: BodyExpr::Literal(Value::int(1)),
                },
            ]),
        };
        let body = RuleBody::new(out_of_order);
        assert_eq!(
            body.validate(&interface),
            Err(BodyError::FieldsOutOfOrder {
                earlier: "b".into(),
                later: "a".into(),
            })
        );
    }

    #[test]
    fn a_request_must_match_its_interface_arity_and_types() {
        let nominal = declared_nominal("test", "Object", Type::Blob);
        let compile = Interface {
            inputs: Box::new([nominal.clone()]),
            output: nominal.clone(),
        };
        let interface = Interface {
            inputs: Box::new([nominal.clone()]),
            output: nominal,
        };
        let arity = RuleBody::new(BodyExpr::Need {
            request: BodyRequest {
                interface: compile.clone(),
                inputs: Box::new([]),
            },
            resume: Box::new(BodyExpr::Bound(0)),
        });
        assert_eq!(
            arity.validate(&interface),
            Err(BodyError::RequestArityMismatch {
                expected: 1,
                provided: 0,
            })
        );

        let mistyped = RuleBody::new(BodyExpr::Need {
            request: BodyRequest {
                interface: compile,
                inputs: Box::new([BodyExpr::Literal(Value::Text("not an object".into()))]),
            },
            resume: Box::new(BodyExpr::Bound(0)),
        });
        assert_eq!(
            mistyped.validate(&interface),
            Err(BodyError::TypeMismatch {
                expected: first_input(&interface),
                found: Type::Text,
            })
        );
    }

    #[test]
    fn a_need_all_binds_one_binder_per_request_in_request_order() {
        let objects = declared_nominal("test", "Object", Type::Blob);
        let report = declared_nominal("test", "Report", Type::Bool);
        let compile = Interface {
            inputs: Box::new([objects.clone()]),
            output: objects.clone(),
        };
        let check = Interface {
            inputs: Box::new([objects.clone()]),
            output: report.clone(),
        };
        let interface = Interface {
            inputs: Box::new([objects.clone()]),
            output: report,
        };
        // The batch's requests are built before any result exists — that is
        // what declaring their independence means — so each sees only the
        // input at Bound(0). The resume runs under [second result, first
        // result, input]: Bound(0) is the first request's result.
        let body = RuleBody::new(BodyExpr::NeedAll {
            requests: Box::new([
                BodyRequest {
                    interface: compile,
                    inputs: Box::new([BodyExpr::Bound(0)]),
                },
                BodyRequest {
                    interface: check.clone(),
                    inputs: Box::new([BodyExpr::Bound(0)]),
                },
            ]),
            resume: Box::new(BodyExpr::Need {
                request: BodyRequest {
                    interface: check,
                    inputs: Box::new([BodyExpr::Bound(0)]),
                },
                resume: Box::new(BodyExpr::Bound(0)),
            }),
        });
        assert_eq!(body.validate(&interface), Ok(()));
    }

    #[test]
    fn an_empty_need_all_is_refused() {
        let interface = Interface {
            inputs: Box::new([Type::Int]),
            output: Type::Int,
        };
        let body = RuleBody::new(BodyExpr::NeedAll {
            requests: Box::new([]),
            resume: Box::new(BodyExpr::Bound(0)),
        });
        assert_eq!(body.validate(&interface), Err(BodyError::EmptyNeedAll));
    }

    #[test]
    fn a_need_each_checks_its_request_against_the_element() {
        let objects = declared_nominal("test", "Object", Type::Blob);
        let compile = Interface {
            inputs: Box::new([objects.clone()]),
            output: objects.clone(),
        };
        let interface = Interface {
            inputs: Box::new([Type::List(Box::new(objects.clone()))]),
            output: Type::List(Box::new(objects.clone())),
        };
        let body = RuleBody::new(BodyExpr::NeedEach {
            source: Box::new(BodyExpr::Bound(0)),
            request: BodyRequest {
                interface: compile,
                // Bound(0) is the element the batch is iterating.
                inputs: Box::new([BodyExpr::Bound(0)]),
            },
            resume: Box::new(BodyExpr::Bound(0)),
        });
        assert_eq!(body.validate(&interface), Ok(()));
    }

    #[test]
    fn a_fold_types_its_step_against_element_and_accumulator() {
        let interface = Interface {
            inputs: Box::new([Type::List(Box::new(Type::Int))]),
            output: Type::Int,
        };
        let body = RuleBody::new(BodyExpr::Fold {
            source: Box::new(BodyExpr::Bound(0)),
            init: Box::new(BodyExpr::Literal(Value::int(0))),
            // element at Bound(0), accumulator at Bound(1).
            step: Box::new(BodyExpr::IntAdd {
                left: Box::new(BodyExpr::Bound(1)),
                right: Box::new(BodyExpr::Bound(0)),
            }),
        });
        assert_eq!(body.validate(&interface), Ok(()));

        // A step whose own operands check but whose type is not the
        // accumulator's: the fold's accumulator rule, not the operands'.
        let swapped = RuleBody::new(BodyExpr::Fold {
            source: Box::new(BodyExpr::Bound(0)),
            init: Box::new(BodyExpr::Literal(Value::Text("".into()))),
            step: Box::new(BodyExpr::IntAdd {
                left: Box::new(BodyExpr::Bound(0)),
                right: Box::new(BodyExpr::Bound(0)),
            }),
        });
        assert_eq!(
            swapped.validate(&interface),
            Err(BodyError::TypeMismatch {
                expected: Type::Text,
                found: Type::Int,
            })
        );
    }

    #[test]
    fn a_match_list_binds_head_then_tail() {
        let interface = Interface {
            inputs: Box::new([Type::List(Box::new(Type::Int))]),
            output: Type::Int,
        };
        let body = RuleBody::new(BodyExpr::MatchList {
            list: Box::new(BodyExpr::Bound(0)),
            empty: Box::new(BodyExpr::Literal(Value::int(0))),
            // head at Bound(0), tail at Bound(1). The inner case adds this
            // cons's head to the outer one; its empty arm is the outer head
            // alone, so a one-element list sums to itself.
            cons: Box::new(BodyExpr::MatchList {
                list: Box::new(BodyExpr::Bound(1)),
                empty: Box::new(BodyExpr::Bound(0)),
                cons: Box::new(BodyExpr::IntAdd {
                    left: Box::new(BodyExpr::Bound(2)),
                    right: Box::new(BodyExpr::Bound(0)),
                }),
            }),
        });
        assert_eq!(body.validate(&interface), Ok(()));
    }

    #[test]
    fn a_match_list_arms_must_agree() {
        let interface = Interface {
            inputs: Box::new([Type::List(Box::new(Type::Int))]),
            output: Type::Int,
        };
        let body = RuleBody::new(BodyExpr::MatchList {
            list: Box::new(BodyExpr::Bound(0)),
            empty: Box::new(BodyExpr::Literal(Value::Text("none".into()))),
            cons: Box::new(BodyExpr::Bound(0)),
        });
        assert_eq!(
            body.validate(&interface),
            Err(BodyError::TypeMismatch {
                expected: Type::Text,
                found: Type::Int,
            })
        );
    }

    #[test]
    fn a_body_nesting_past_the_bound_is_refused() {
        let interface = Interface {
            inputs: Box::new([]),
            output: Type::Int,
        };
        let mut body = BodyExpr::Literal(Value::int(0));
        for _ in 0..MAX_BODY_DEPTH {
            body = BodyExpr::Let {
                bound: Box::new(body),
                rest: Box::new(BodyExpr::Bound(0)),
            };
        }
        let body = RuleBody::new(body);
        assert_eq!(
            body.validate(&interface),
            Err(BodyError::DepthExceeded {
                limit: MAX_BODY_DEPTH
            })
        );
    }
}
