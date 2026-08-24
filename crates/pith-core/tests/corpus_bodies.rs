//! The corpus measurement of the represented-body constructor set (decision
//! 0062): every pure rule body the four first-party domains registered is
//! either expressed here as a hand-built [`RuleBody`] that validates and
//! round-trips, or named below with the constructor it waits for.
//!
//! The interfaces and declaration shapes mirror the live tables in `xylem`,
//! `stele`, and the peer fixture domain; the coordinates and representations are
//! copied, not approximated, so a body expressed here is a body a migration
//! could carry. M-11 ships no evaluator, so expression means construction,
//! validation against the mirrored interface, and a canonical-encoding round
//! trip — the round M-12's interpreter and M-13's notation build on.

use pith_core::{
    BodyExpr, BodyRequest, DeclarationTable, Interface, MatchArm, NominalType, RecordField, Rule,
    RuleBody, RuleTier, SumConstructor, SumType, Type, Value,
};

// ---------------------------------------------------------------------------
// The three unexpressible bodies, named with what each waits for.
// ---------------------------------------------------------------------------

/// The corpus rule bodies the constructor set cannot express, each with the
/// reason. A body leaves this list only by amendment to 0062.
const NAMED: &[(&str, &str)] = &[
    (
        "xylem.compile-entry",
        "the depfile parse splits bytes on whitespace, joins continuations, \
         and strips prefixes; the set has no text-splitting constructor with an \
         agreed total semantics",
    ),
    (
        "example.render-entry",
        "the template scan walks `{{`..`}}` delimiters by index; the same \
         missing text-splitting constructor",
    ),
    (
        "phloem.resolve",
        "version ordering is a host trait object selected by a request-visible \
         name, and the search is backtracking with an undo stack — host \
         dispatch and general recursion, both outside the set by construction",
    ),
];

// ---------------------------------------------------------------------------
// Declaration mirrors
// ---------------------------------------------------------------------------

mod xylem {
    use super::*;

    pub fn toolchain() -> Type {
        nominal("Toolchain", Type::Text)
    }
    pub fn c_source() -> Type {
        nominal("CSource", Type::Blob)
    }
    pub fn object() -> Type {
        nominal("Object", Type::Blob)
    }
    pub fn executable() -> Type {
        nominal("Executable", Type::Blob)
    }
    pub fn test_report() -> Type {
        nominal("TestReport", Type::Bool)
    }

    fn nominal(name: &str, representation: Type) -> Type {
        declared("xylem", name, representation)
    }

    /// `{path: Text, content: Blob}`, the header shape a compile request
    /// declares beside its source.
    pub fn provided_header_record() -> Type {
        record_type(&[("content", Type::Blob), ("path", Type::Text)])
    }
    pub fn provided_headers() -> Type {
        Type::List(Box::new(provided_header_record()))
    }

    pub fn compile_interface() -> Interface {
        Interface {
            inputs: Box::new([toolchain(), c_source(), provided_headers()]),
            output: object(),
        }
    }
    pub fn link_interface() -> Interface {
        Interface {
            inputs: Box::new([toolchain(), list(object())]),
            output: executable(),
        }
    }
    pub fn test_interface() -> Interface {
        Interface {
            inputs: Box::new([toolchain(), executable()]),
            output: test_report(),
        }
    }
    pub fn generate_interface() -> Interface {
        Interface {
            inputs: Box::new([toolchain(), executable()]),
            output: c_source(),
        }
    }
}

mod stele {
    use super::*;

    pub fn file_body() -> Type {
        declared_sum(
            "stele",
            "FileBody",
            &[
                (
                    "file",
                    Some(record_type(&[
                        ("content", Type::Blob),
                        ("executable", Type::Bool),
                    ])),
                ),
                ("symlink", Some(record_type(&[("target", Type::Text)]))),
            ],
        )
    }
    pub fn file_entry() -> Type {
        record_type(&[("body", file_body()), ("path", Type::Text)])
    }
    pub fn file_set() -> Type {
        nominal("FileSet", list(file_entry()))
    }
    pub fn user_record() -> Type {
        record_type(&[
            ("gid", Type::Int),
            ("home", Type::Text),
            ("name", Type::Text),
            ("shell", Type::Text),
            ("uid", Type::Int),
        ])
    }
    pub fn user_table() -> Type {
        nominal("UserTable", list(user_record()))
    }
    pub fn unit_record() -> Type {
        record_type(&[
            ("after", list(Type::Text)),
            ("description", Type::Text),
            ("exec", Type::Text),
            ("name", Type::Text),
            ("wants", list(Type::Text)),
        ])
    }
    pub fn unit() -> Type {
        nominal("Unit", unit_record())
    }
    pub fn field_behavior() -> Type {
        declared_sum(
            "stele",
            "FieldBehavior",
            &[("agree", None), ("concat", None)],
        )
    }
    pub fn policy_entry() -> Type {
        record_type(&[("behavior", field_behavior()), ("field", Type::Text)])
    }
    pub fn unit_policy() -> Type {
        nominal("UnitPolicy", list(policy_entry()))
    }
    pub fn tools() -> Type {
        nominal("Tools", tools_record())
    }
    fn tools_record() -> Type {
        record_type(&[
            ("cat", Type::Text),
            ("chmod", Type::Text),
            ("closure", list(Type::Text)),
            ("ln", Type::Text),
            ("mkdir", Type::Text),
            ("shell", Type::Text),
        ])
    }
    pub fn boot() -> Type {
        nominal(
            "Boot",
            record_type(&[
                ("initrd", Type::Text),
                ("kernel", Type::Text),
                ("machine", Type::Text),
            ]),
        )
    }
    pub fn unit_text() -> Type {
        nominal("UnitText", Type::Text)
    }
    pub fn passwd_text() -> Type {
        nominal("PasswdText", Type::Text)
    }
    pub fn boot_text() -> Type {
        nominal("BootText", Type::Text)
    }
    pub fn system_tree() -> Type {
        nominal("SystemTree", Type::Blob)
    }

    fn nominal(name: &str, representation: Type) -> Type {
        declared("stele", name, representation)
    }

    pub fn contribution(payload_name: &str, payload: Type) -> Type {
        list(record_type(&[
            ("owner", Type::Text),
            (payload_name, payload),
        ]))
    }
    pub fn replacement() -> Type {
        record_type(&[
            ("expected-owner", Type::Text),
            ("field", Type::Text),
            ("value", Type::Text),
        ])
    }

    pub fn etc_interface() -> Interface {
        Interface {
            inputs: Box::new([contribution("files", file_set())]),
            output: file_set(),
        }
    }
    pub fn users_interface() -> Interface {
        Interface {
            inputs: Box::new([contribution("users", user_table())]),
            output: user_table(),
        }
    }
    pub fn unit_interface() -> Interface {
        Interface {
            inputs: Box::new([
                unit_policy(),
                contribution("unit", unit()),
                list(replacement()),
            ]),
            output: unit(),
        }
    }
    pub fn render_unit_interface() -> Interface {
        Interface {
            inputs: Box::new([unit()]),
            output: unit_text(),
        }
    }
    pub fn render_passwd_interface() -> Interface {
        Interface {
            inputs: Box::new([user_table()]),
            output: passwd_text(),
        }
    }
    pub fn render_boot_interface() -> Interface {
        Interface {
            inputs: Box::new([boot()]),
            output: boot_text(),
        }
    }
    pub fn assemble_interface() -> Interface {
        Interface {
            inputs: Box::new([
                tools(),
                Type::Text,
                Type::Text,
                file_set(),
                unit_text(),
                passwd_text(),
                boot_text(),
            ]),
            output: system_tree(),
        }
    }
    pub fn compose_system_interface() -> Interface {
        Interface {
            inputs: Box::new([
                tools(),
                boot(),
                contribution("files", file_set()),
                contribution("users", user_table()),
                unit_policy(),
                contribution("unit", unit()),
                list(replacement()),
            ]),
            output: system_tree(),
        }
    }
}

mod phloem {
    use super::*;

    /// `{content: Blob, path: Text}`, a measured source file.
    pub fn tree_entry() -> Type {
        record_type(&[("content", Type::Blob), ("path", Type::Text)])
    }
    pub fn tree() -> Type {
        list(tree_entry())
    }
    pub fn build() -> Type {
        record_type(&[
            ("includes", list(Type::Text)),
            ("sources", list(Type::Text)),
        ])
    }
    pub fn library() -> Type {
        record_type(&[
            ("headers", xylem::provided_headers()),
            ("objects", list(xylem::object())),
        ])
    }
    pub fn dependency() -> Type {
        record_type(&[("build", build()), ("tree", tree())])
    }

    pub fn package_build_interface() -> Interface {
        Interface {
            inputs: Box::new([xylem::toolchain(), tree(), build(), list(dependency())]),
            output: xylem::executable(),
        }
    }
    pub fn package_library_interface() -> Interface {
        Interface {
            inputs: Box::new([xylem::toolchain(), tree(), build()]),
            output: library(),
        }
    }
}

// ---------------------------------------------------------------------------
// Shared construction
// ---------------------------------------------------------------------------

fn declared(module: &str, name: &str, representation: Type) -> Type {
    let mut table = DeclarationTable::new(module);
    match table.nominal(name, representation) {
        Ok(declared) => declared,
        Err(error) => unreachable!("{module} declares {name} once: {error}"),
    }
}

fn declared_sum(module: &str, name: &str, constructors: &[(&str, Option<Type>)]) -> Type {
    let constructors: Box<[SumConstructor]> = constructors
        .iter()
        .map(|(constructor, payload)| SumConstructor {
            name: (*constructor).into(),
            payload: payload.clone(),
        })
        .collect();
    let mut table = DeclarationTable::new(module);
    match table.sum(name, constructors) {
        Ok(declared) => declared,
        Err(error) => unreachable!("{module} declares {name} once: {error}"),
    }
}

fn record_type(fields: &[(&str, Type)]) -> Type {
    let fields: Box<[RecordField<Type>]> = fields
        .iter()
        .map(|(name, payload)| RecordField {
            name: (*name).into(),
            payload: payload.clone(),
        })
        .collect();
    match Type::record(fields) {
        Ok(record) => record,
        Err(error) => unreachable!("a mirrored record names its fields once: {error}"),
    }
}

fn nominal_type_of(declared: &Type) -> NominalType {
    match declared {
        Type::Nominal(declared) => (**declared).clone(),
        other => unreachable!("a mirrored nominal is one: {other}"),
    }
}

fn sum_type_of(declared: &Type) -> SumType {
    match declared {
        Type::Sum(declared) => (**declared).clone(),
        other => unreachable!("a mirrored sum is one: {other}"),
    }
}

fn list(element: Type) -> Type {
    Type::List(Box::new(element))
}

fn lit(value: Value) -> BodyExpr {
    BodyExpr::Literal(value)
}

fn text(value: &str) -> Value {
    Value::Text(value.into())
}

fn bound(index: usize) -> BodyExpr {
    BodyExpr::Bound(index)
}

fn field(name: &str) -> impl Fn(BodyExpr) -> BodyExpr {
    let name: Box<str> = name.into();
    move |record| BodyExpr::Field {
        record: Box::new(record),
        name: name.clone(),
    }
}

fn record(fields: &[(&str, BodyExpr)]) -> BodyExpr {
    let fields: Box<[RecordField<BodyExpr>]> = fields
        .iter()
        .map(|(name, payload)| RecordField {
            name: (*name).into(),
            payload: payload.clone(),
        })
        .collect();
    match BodyExpr::record(fields) {
        Ok(record) => record,
        Err(error) => unreachable!("a mirrored record names its fields once: {error}"),
    }
}

fn request(interface: Interface, inputs: &[BodyExpr]) -> BodyRequest {
    BodyRequest {
        interface,
        inputs: inputs.to_vec().into_boxed_slice(),
    }
}

fn fold(source: BodyExpr, init: BodyExpr, step: BodyExpr) -> BodyExpr {
    BodyExpr::Fold {
        source: Box::new(source),
        init: Box::new(init),
        step: Box::new(step),
    }
}

fn if_(condition: BodyExpr, then: BodyExpr, otherwise: BodyExpr) -> BodyExpr {
    BodyExpr::If {
        condition: Box::new(condition),
        then: Box::new(then),
        otherwise: Box::new(otherwise),
    }
}

fn equal(left: BodyExpr, right: BodyExpr) -> BodyExpr {
    BodyExpr::Equal {
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn describe(value: BodyExpr) -> BodyExpr {
    BodyExpr::Describe {
        value: Box::new(value),
    }
}

/// Concatenate texts left to right; the empty list of parts is the empty text.
fn concat(parts: &[BodyExpr]) -> BodyExpr {
    let mut concatenated = lit(text(""));
    for part in parts {
        concatenated = BodyExpr::TextConcat {
            left: Box::new(concatenated),
            right: Box::new(part.clone()),
        };
    }
    concatenated
}

fn fail(message_parts: &[BodyExpr]) -> BodyExpr {
    BodyExpr::Fail {
        message: Box::new(concat(message_parts)),
    }
}

fn empty_list(element: Type) -> BodyExpr {
    BodyExpr::List {
        element,
        items: Box::new([]),
    }
}

/// Deepen every free binder reference by `extra`: an expression correct at one
/// scope stays correct under `extra` more pushed binders, which is what
/// embedding a captured subexpression inside a fold or case requires. The
/// cutoff keeps references a construct binds internally pointing where they
/// did.
///
/// A match is refused rather than descended into: whether an arm binds a
/// payload depends on the sum being eliminated, which the syntax does not
/// carry, and the corpus never embeds one under additional binders. The
/// general walk belongs to the elaborator M-12 builds.
fn shift(expression: &BodyExpr, extra: usize) -> BodyExpr {
    deepen(expression, extra, 0)
}

fn deepen(expression: &BodyExpr, extra: usize, cutoff: usize) -> BodyExpr {
    let deeper = |inner: &BodyExpr, pushed: usize| {
        Box::new(deepen(inner, extra, cutoff.saturating_add(pushed)))
    };
    match expression {
        BodyExpr::Bound(index) => BodyExpr::Bound(if *index >= cutoff {
            index.saturating_add(extra)
        } else {
            *index
        }),
        BodyExpr::Literal(value) => BodyExpr::Literal(value.clone()),
        BodyExpr::Match { .. } => {
            unreachable!("the corpus never embeds a match under additional binders")
        }
        BodyExpr::Let { bound, rest } => BodyExpr::Let {
            bound: deeper(bound, 0),
            rest: deeper(rest, 1),
        },
        BodyExpr::Fail { message } => BodyExpr::Fail {
            message: deeper(message, 0),
        },
        BodyExpr::Record { fields } => BodyExpr::Record {
            fields: fields
                .iter()
                .map(|field| RecordField {
                    name: field.name.clone(),
                    payload: deepen(&field.payload, extra, cutoff),
                })
                .collect(),
        },
        BodyExpr::Field { record, name } => BodyExpr::Field {
            record: deeper(record, 0),
            name: name.clone(),
        },
        BodyExpr::MakeSum {
            declared,
            constructor,
            payload,
        } => BodyExpr::MakeSum {
            declared: declared.clone(),
            constructor: constructor.clone(),
            payload: payload
                .as_deref()
                .map(|payload| deepen(payload, extra, cutoff).into()),
        },
        BodyExpr::Wrap {
            declared,
            representation,
        } => BodyExpr::Wrap {
            declared: declared.clone(),
            representation: deeper(representation, 0),
        },
        BodyExpr::Unwrap { nominal } => BodyExpr::Unwrap {
            nominal: deeper(nominal, 0),
        },
        BodyExpr::List { element, items } => BodyExpr::List {
            element: element.clone(),
            items: items
                .iter()
                .map(|item| deepen(item, extra, cutoff))
                .collect(),
        },
        BodyExpr::Cons { head, tail } => BodyExpr::Cons {
            head: deeper(head, 0),
            tail: deeper(tail, 0),
        },
        BodyExpr::Append { left, right } => BodyExpr::Append {
            left: deeper(left, 0),
            right: deeper(right, 0),
        },
        BodyExpr::Fold { source, init, step } => BodyExpr::Fold {
            source: deeper(source, 0),
            init: deeper(init, 0),
            step: deeper(step, 2),
        },
        BodyExpr::SortBy { list, key } => BodyExpr::SortBy {
            list: deeper(list, 0),
            key: deeper(key, 1),
        },
        BodyExpr::If {
            condition,
            then,
            otherwise,
        } => BodyExpr::If {
            condition: deeper(condition, 0),
            then: deeper(then, 0),
            otherwise: deeper(otherwise, 0),
        },
        BodyExpr::Equal { left, right }
        | BodyExpr::IntAdd { left, right }
        | BodyExpr::IntSubtract { left, right }
        | BodyExpr::IntMultiply { left, right }
        | BodyExpr::TextConcat { left, right } => {
            let left = Box::new(deepen(left, extra, cutoff));
            let right = Box::new(deepen(right, extra, cutoff));
            match expression {
                BodyExpr::Equal { .. } => BodyExpr::Equal { left, right },
                BodyExpr::IntAdd { .. } => BodyExpr::IntAdd { left, right },
                BodyExpr::IntSubtract { .. } => BodyExpr::IntSubtract { left, right },
                BodyExpr::IntMultiply { .. } => BodyExpr::IntMultiply { left, right },
                _ => BodyExpr::TextConcat { left, right },
            }
        }
        BodyExpr::Describe { value } => BodyExpr::Describe {
            value: deeper(value, 0),
        },
        BodyExpr::TextOfBytes { bytes } => BodyExpr::TextOfBytes {
            bytes: deeper(bytes, 0),
        },
        BodyExpr::Need { request, resume } => BodyExpr::Need {
            request: shift_request(request, extra, cutoff),
            resume: deeper(resume, 1),
        },
        BodyExpr::NeedAll { requests, resume } => BodyExpr::NeedAll {
            requests: requests
                .iter()
                .map(|request| shift_request(request, extra, cutoff))
                .collect(),
            resume: deeper(resume, requests.len()),
        },
        BodyExpr::NeedEach {
            source,
            request,
            resume,
        } => BodyExpr::NeedEach {
            source: deeper(source, 0),
            request: shift_request(request, extra, cutoff.saturating_add(1)),
            resume: deeper(resume, 1),
        },
        BodyExpr::NeedBlob { content, resume } => BodyExpr::NeedBlob {
            content: deeper(content, 0),
            resume: deeper(resume, 1),
        },
        BodyExpr::NeedAction { request, resume } => BodyExpr::NeedAction {
            request: shift_request(request, extra, cutoff),
            resume: deeper(resume, 1),
        },
        BodyExpr::NeedObservation { request, resume } => BodyExpr::NeedObservation {
            request: shift_request(request, extra, cutoff),
            resume: deeper(resume, 1),
        },
        BodyExpr::MatchList { list, empty, cons } => BodyExpr::MatchList {
            list: deeper(list, 0),
            empty: deeper(empty, 0),
            cons: deeper(cons, 2),
        },
    }
}

fn shift_request(request: &BodyRequest, extra: usize, cutoff: usize) -> BodyRequest {
    BodyRequest {
        interface: request.interface.clone(),
        inputs: request
            .inputs
            .iter()
            .map(|input| deepen(input, extra, cutoff))
            .collect(),
    }
}

/// Prepend every element of `source`, mapped by `map` over the element at
/// `Bound(0)`, producing the mapped list in reverse source order.
fn map_prepend(source: BodyExpr, element: Type, map: impl Fn(BodyExpr) -> BodyExpr) -> BodyExpr {
    fold(
        source,
        empty_list(element),
        BodyExpr::Cons {
            head: Box::new(map(bound(0))),
            tail: Box::new(bound(1)),
        },
    )
}

/// Reverse `list_of_type` by prepending its elements.
fn reverse(list_of_type: Type, source: BodyExpr) -> BodyExpr {
    fold(
        source,
        empty_list(list_of_type),
        BodyExpr::Cons {
            head: Box::new(bound(0)),
            tail: Box::new(bound(1)),
        },
    )
}

/// The first element of `source` satisfying `holds` over the element at
/// `Bound(0)`, or the failure `miss` names. The collect-then-case shape keeps
/// the miss a failure rather than a fabricated value.
///
/// `holds` builds its predicate at the fold-step scope, so a captured
/// expression it embeds must arrive already deepened by two.
fn first_matching(
    element: Type,
    source: BodyExpr,
    holds: impl Fn(BodyExpr) -> BodyExpr,
    found: impl Fn(BodyExpr) -> BodyExpr,
    miss: &[BodyExpr],
) -> BodyExpr {
    let matches = fold(
        source,
        empty_list(element.clone()),
        if_(
            holds(bound(0)),
            BodyExpr::Cons {
                head: Box::new(bound(0)),
                tail: Box::new(bound(1)),
            },
            bound(1),
        ),
    );
    BodyExpr::MatchList {
        list: Box::new(matches),
        empty: Box::new(fail(miss)),
        cons: Box::new(found(bound(0))),
    }
}

/// Assert the corpus claim for one body: it validates against the mirrored
/// interface, survives its own canonical encoding, and derives a represented
/// rule whose revision is a function of those bytes.
fn assert_expressed(module: &str, label: &str, interface: &Interface, body: &RuleBody) {
    assert_eq!(
        body.validate(interface),
        Ok(()),
        "`{module}.{label}` does not check against its mirrored interface"
    );
    let encoded = body.encode_canonical();
    let decoded = RuleBody::decode_canonical(&encoded)
        .unwrap_or_else(|error| unreachable!("`{module}.{label}` does not decode: {error}"));
    assert_eq!(&decoded, body);
    let rule = Rule::represented(
        module,
        label,
        body,
        interface.clone(),
        pith_diag::Span::none(),
    );
    assert_eq!(rule.tier, RuleTier::Represented);
    let again = Rule::represented(
        module,
        label,
        &decoded,
        interface.clone(),
        pith_diag::Span::none(),
    );
    assert_eq!(rule.revision, again.revision);
}

// ---------------------------------------------------------------------------
// xylem: the three action wrappers
// ---------------------------------------------------------------------------

/// `NeedAction` with the rule's own inputs forwarded verbatim, then the
/// action's result passed through: the whole of link, generate, and test.
fn action_passthrough(interface: &Interface) -> RuleBody {
    let forwarded: Box<[BodyExpr]> = (0..interface.inputs.len())
        .rev()
        .map(BodyExpr::Bound)
        .collect();
    RuleBody::new(BodyExpr::NeedAction {
        request: request(interface.clone(), &forwarded),
        resume: Box::new(bound(0)),
    })
}

#[test]
fn xylem_link_entry_is_expressed() {
    assert_expressed(
        "xylem",
        "link-entry",
        &xylem::link_interface(),
        &action_passthrough(&xylem::link_interface()),
    );
}

#[test]
fn xylem_generate_entry_is_expressed() {
    assert_expressed(
        "xylem",
        "generate-entry",
        &xylem::generate_interface(),
        &action_passthrough(&xylem::generate_interface()),
    );
}

#[test]
fn xylem_test_entry_is_expressed() {
    assert_expressed(
        "xylem",
        "test-entry",
        &xylem::test_interface(),
        &action_passthrough(&xylem::test_interface()),
    );
}

// ---------------------------------------------------------------------------
// stele: compose-system
// ---------------------------------------------------------------------------

/// `(Tools, Boot, file and user and unit contributions, policy, replacements)
/// -> SystemTree`: two static batches over different interfaces, then one
/// action over parts read out of the merged values.
#[test]
fn stele_compose_system_is_expressed() {
    let interface = stele::compose_system_interface();
    // Inputs, before any resumption: tools(9) boot(8) etc(7) users(6)
    // policy(5) units(4) repl(0).
    let compose_etc = request(stele::etc_interface(), &[bound(4)]);
    let compose_users = request(stele::users_interface(), &[bound(3)]);
    let compose_unit = request(stele::unit_interface(), &[bound(2), bound(1), bound(0)]);
    let body = RuleBody::new(BodyExpr::NeedAll {
        requests: Box::new([compose_etc, compose_users, compose_unit]),
        resume: Box::new({
            // Under [etc-file-set(0) users-table(1) unit(2) repl(3) units(4)
            // policy(5) users(6) etc(7) boot(8) tools(9)].
            let render_unit = request(stele::render_unit_interface(), &[bound(2)]);
            let render_users = request(stele::render_passwd_interface(), &[bound(1)]);
            let render_boot = request(stele::render_boot_interface(), &[bound(8)]);
            BodyExpr::NeedAll {
                requests: Box::new([render_unit, render_users, render_boot]),
                resume: Box::new({
                    // Under [unit-text(0) passwd-text(1) boot-text(2)
                    // file-set(3) users-table(4) unit(5) repl(6) units(7)
                    // policy(8) users(9) etc(10) boot(11) tools(12)] — the
                    // first batch pushed [file-set, table, unit], so the
                    // unit is three binders out.
                    let machine = field("machine")(BodyExpr::Unwrap {
                        nominal: Box::new(bound(11)),
                    });
                    let unit_name = field("name")(BodyExpr::Unwrap {
                        nominal: Box::new(bound(5)),
                    });
                    BodyExpr::NeedAction {
                        request: request(
                            stele::assemble_interface(),
                            &[
                                bound(12),
                                machine,
                                unit_name,
                                bound(3),
                                bound(0),
                                bound(1),
                                bound(2),
                            ],
                        ),
                        resume: Box::new(bound(0)),
                    }
                }),
            }
        }),
    });
    assert_expressed("stele", "compose-system", &interface, &body);
}

// ---------------------------------------------------------------------------
// stele: the keyed merges
// ---------------------------------------------------------------------------

/// The keyed merge both file and user composition run: decorate each entry
/// with its owner and key, sort by key then owner, then fold with the
/// head-of-kept case — agreeing entries collapse, one key naming two values
/// fails naming both owners — and finally rebuild the declared entry shape in
/// key order.
#[expect(
    clippy::too_many_arguments,
    reason = "a keyed merge is parameterized by its key, value, and rebuild projections"
)]
fn keyed_merge(
    input: BodyExpr,
    contribution_payload: &str,
    decorated: Type,
    entry_type: Type,
    key_of: impl Fn(BodyExpr) -> BodyExpr + Copy,
    value_of: impl Fn(BodyExpr) -> BodyExpr + Copy,
    rebuild: impl Fn(BodyExpr) -> BodyExpr + Copy,
    conflict_subject: &str,
) -> BodyExpr {
    // One contribution's entries, each decorated with the owner. The inner
    // fold walks the contribution's payload; `bound(2)` is the contribution
    // itself, two binders out.
    let decorate = fold(
        BodyExpr::Unwrap {
            nominal: Box::new(field(contribution_payload)(bound(0))),
        },
        bound(1),
        BodyExpr::Cons {
            head: Box::new(record(&[
                ("entry", bound(0)),
                ("key", key_of(bound(0))),
                ("owner", field("owner")(bound(2))),
            ])),
            tail: Box::new(bound(1)),
        },
    );
    let decorated_entries = fold(input, empty_list(decorated.clone()), decorate);

    let sorted = BodyExpr::SortBy {
        list: Box::new(decorated_entries),
        key: Box::new(BodyExpr::List {
            element: Type::Text,
            items: Box::new([field("key")(bound(0)), field("owner")(bound(0))]),
        }),
    };

    // The head of the kept list is the previous entry in sorted order, so
    // agreeing and conflicting neighbors are adjacent cases.
    let step = BodyExpr::MatchList {
        list: Box::new(bound(1)),
        empty: Box::new(BodyExpr::Cons {
            head: Box::new(bound(0)),
            tail: Box::new(bound(1)),
        }),
        cons: Box::new({
            // head(0) tail(1) element(2) kept(3).
            let conflict = fail(&[
                lit(text(&format!("the {conflict_subject} `"))),
                describe(field("key")(bound(2))),
                lit(text("` is `")),
                describe(value_of(bound(0))),
                lit(text("` from `")),
                describe(field("owner")(bound(0))),
                lit(text("` and `")),
                describe(value_of(bound(2))),
                lit(text("` from `")),
                describe(field("owner")(bound(2))),
                lit(text("`, and neither replaces the other")),
            ]);
            if_(
                equal(field("key")(bound(0)), field("key")(bound(2))),
                if_(
                    equal(value_of(bound(0)), value_of(bound(2))),
                    bound(3),
                    conflict,
                ),
                BodyExpr::Cons {
                    head: Box::new(bound(2)),
                    tail: Box::new(bound(3)),
                },
            )
        }),
    };
    let kept = fold(sorted, empty_list(decorated.clone()), step);

    let entries = map_prepend(kept, entry_type.clone(), rebuild);
    reverse(entry_type, entries)
}

#[test]
fn stele_compose_etc_is_expressed() {
    let interface = stele::etc_interface();
    let decorated = record_type(&[
        ("entry", stele::file_entry()),
        ("key", Type::Text),
        ("owner", Type::Text),
    ]);
    let merged = keyed_merge(
        bound(0),
        "files",
        decorated,
        stele::file_entry(),
        |entry| field("path")(entry),
        |entry| field("body")(field("entry")(entry)),
        |kept| {
            record(&[
                ("body", field("body")(field("entry")(kept.clone()))),
                ("path", field("key")(kept)),
            ])
        },
        "file",
    );
    let body = RuleBody::new(BodyExpr::Wrap {
        declared: nominal_type_of(&stele::file_set()),
        representation: Box::new(merged),
    });
    assert_expressed("stele", "compose-etc", &interface, &body);
}

#[test]
fn stele_compose_users_is_expressed() {
    let interface = stele::users_interface();
    let decorated = record_type(&[
        ("entry", stele::user_record()),
        ("key", Type::Text),
        ("owner", Type::Text),
    ]);
    let merged = keyed_merge(
        bound(0),
        "users",
        decorated,
        stele::user_record(),
        |entry| field("name")(entry),
        |entry| entry,
        |kept| field("entry")(kept),
        "account",
    );
    let body = RuleBody::new(BodyExpr::Wrap {
        declared: nominal_type_of(&stele::user_table()),
        representation: Box::new(merged),
    });
    assert_expressed("stele", "compose-users", &interface, &body);
}

// ---------------------------------------------------------------------------
// stele: compose-unit
// ---------------------------------------------------------------------------

/// The flattened contribution every later step works over: the unit's five
/// fields plus the owner.
fn flat_unit_type() -> Type {
    record_type(&[
        ("after", list(Type::Text)),
        ("description", Type::Text),
        ("exec", Type::Text),
        ("name", Type::Text),
        ("owner", Type::Text),
        ("wants", list(Type::Text)),
    ])
}

/// The policy's behavior for `name`, defaulting to agree: a fold over the
/// policy entries whose accumulator is the current answer.
fn policy_behavior(policy_list: BodyExpr, name: &str) -> BodyExpr {
    fold(
        policy_list,
        BodyExpr::MakeSum {
            declared: sum_type_of(&stele::field_behavior()),
            constructor: "agree".into(),
            payload: None,
        },
        if_(
            equal(field("field")(bound(0)), lit(text(name))),
            field("behavior")(bound(0)),
            bound(1),
        ),
    )
}

/// One merged field under `policy_lookup` and `contributions`, both correct at
/// the scope the result lands in: agree checks every carrier against the
/// first and takes that carrier's value; concat appends every carrier's list
/// in owner order and canonicalizes it. A concat-named field that is not a
/// list is the refusal the host body raises at its decode gate — the typed
/// body cannot append a text, so it fails here instead.
fn merged_field(
    name: &str,
    field_type: Type,
    policy_lookup: BodyExpr,
    contributions: BodyExpr,
) -> BodyExpr {
    let first_carrier = BodyExpr::MatchList {
        list: Box::new(contributions.clone()),
        empty: Box::new(fail(&[
            lit(text("a merged unit has no contribution to name its `")),
            lit(text(name)),
            lit(text("`")),
        ])),
        cons: Box::new(bound(0)),
    };
    let agree = if_(
        fold(
            contributions.clone(),
            lit(Value::Bool(true)),
            if_(
                equal(field(name)(bound(0)), field(name)(shift(&first_carrier, 2))),
                bound(1),
                fail(&[
                    lit(text("the `")),
                    lit(text(name)),
                    lit(text(
                        "` of a merged unit disagrees between its contributions",
                    )),
                ]),
            ),
        ),
        field(name)(first_carrier),
        fail(&[lit(text(
            "the agreement check failed without naming a value",
        ))]),
    );
    let concatenated = match field_type {
        Type::List(element) => {
            let collected = fold(
                contributions,
                empty_list((*element).clone()),
                BodyExpr::Append {
                    left: Box::new(field(name)(bound(0))),
                    right: Box::new(bound(1)),
                },
            );
            let sorted = BodyExpr::SortBy {
                list: Box::new(collected),
                key: Box::new(bound(0)),
            };
            let deduped = fold(
                sorted,
                empty_list((*element).clone()),
                BodyExpr::MatchList {
                    list: Box::new(bound(1)),
                    empty: Box::new(BodyExpr::Cons {
                        head: Box::new(bound(0)),
                        tail: Box::new(bound(1)),
                    }),
                    cons: Box::new(if_(
                        equal(bound(0), bound(2)),
                        bound(3),
                        BodyExpr::Cons {
                            head: Box::new(bound(2)),
                            tail: Box::new(bound(3)),
                        },
                    )),
                },
            );
            reverse((*element).clone(), deduped)
        }
        _ => fail(&[
            lit(text("the `")),
            lit(text(name)),
            lit(text(
                "` of a merged unit is concatenated, so its contributions must carry lists",
            )),
        ]),
    };

    BodyExpr::Match {
        scrutinee: Box::new(policy_lookup),
        arms: Box::new([
            MatchArm {
                constructor: "agree".into(),
                body: Box::new(agree),
            },
            MatchArm {
                constructor: "concat".into(),
                body: Box::new(concatenated),
            },
        ]),
    }
}

/// Apply every replacement in order: each names a field, the owner it expects
/// to be replacing, and the value that takes the field. A replacement whose
/// field is not one of the unit's, or whose owner no longer declares it, is
/// the refusal 0052 spells; otherwise the value lands on every carrier.
fn replacements_applied(contributions: BodyExpr, replacements: BodyExpr) -> BodyExpr {
    let unit_fields = ["after", "description", "exec", "name", "wants"];
    // Inside the outer step: replacement(0) carried(1).
    let named_field = unit_fields
        .iter()
        .rev()
        .fold(lit(Value::Bool(false)), |known, name| {
            if_(
                equal(field("field")(bound(0)), lit(text(name))),
                lit(Value::Bool(true)),
                known,
            )
        });
    let carried_count = fold(
        bound(1),
        lit(Value::int(0)),
        BodyExpr::IntAdd {
            left: Box::new(lit(Value::int(1))),
            right: Box::new(bound(1)),
        },
    );
    let expected_owns = fold(
        bound(1),
        lit(Value::Bool(false)),
        if_(
            equal(field("owner")(bound(0)), field("expected-owner")(bound(2))),
            lit(Value::Bool(true)),
            bound(1),
        ),
    );
    // The value lands on every carrier: a case on the field's name selects
    // which field the rebuilt record carries it in. Inside the mapping fold:
    // carrier(0) mapped-so-far(1) replacement(2).
    // A replacement's value is a text, so only a text field can receive it.
    // The untyped host body would land it on a list field and fail later at
    // a decode gate; the typed body refuses it here, at the replacement.
    let replaceable = ["description", "exec", "name"];
    let rebuilt_record = |name: &str| {
        let fields: Vec<(&str, BodyExpr)> = unit_fields
            .iter()
            .map(|other| {
                let payload = if *other == name {
                    field("value")(bound(2))
                } else {
                    field(other)(bound(0))
                };
                (*other, payload)
            })
            .chain(std::iter::once(("owner", field("owner")(bound(0)))))
            .collect();
        record(&fields)
    };
    let case_on_field = unit_fields.iter().rev().fold(bound(0), |rebuilt, name| {
        let arm = if replaceable.contains(name) {
            rebuilt_record(name)
        } else {
            fail(&[
                lit(text("a replacement cannot retarget the list field `")),
                lit(text(name)),
                lit(text("` with a text value")),
            ])
        };
        if_(
            equal(field("field")(bound(2)), lit(text(name))),
            arm,
            rebuilt,
        )
    });
    let mapped = fold(
        bound(1),
        empty_list(flat_unit_type()),
        BodyExpr::Cons {
            head: Box::new(case_on_field),
            tail: Box::new(bound(1)),
        },
    );
    let step = if_(
        named_field.clone(),
        if_(
            equal(carried_count.clone(), lit(Value::int(0))),
            fail(&[
                lit(text("`")),
                describe(field("expected-owner")(bound(0))),
                lit(text("` replaces a field no contribution declares")),
            ]),
            if_(
                expected_owns.clone(),
                mapped.clone(),
                fail(&[
                    lit(text("`")),
                    describe(field("expected-owner")(bound(0))),
                    lit(text(
                        "` replaces a field as if it still declared it, but its ownership moved",
                    )),
                ]),
            ),
        ),
        fail(&[
            lit(text("`")),
            describe(field("field")(bound(0))),
            lit(text("` is not a field of the unit being merged")),
        ]),
    );
    fold(replacements, contributions, step)
}

#[test]
fn stele_compose_unit_is_expressed() {
    let interface = stele::unit_interface();
    let flat = flat_unit_type();
    // Inputs: policy(2) contributions(1) replacements(0).
    let unwrap_unit = || BodyExpr::Unwrap {
        nominal: Box::new(field("unit")(bound(0))),
    };
    let flatten_step = BodyExpr::Cons {
        head: Box::new(record(&[
            ("after", field("after")(unwrap_unit())),
            ("description", field("description")(unwrap_unit())),
            ("exec", field("exec")(unwrap_unit())),
            ("name", field("name")(unwrap_unit())),
            ("owner", field("owner")(bound(0))),
            ("wants", field("wants")(unwrap_unit())),
        ])),
        tail: Box::new(bound(1)),
    };
    let body = RuleBody::new(BodyExpr::Let {
        // C: the flattened contributions.
        bound: Box::new(fold(bound(1), empty_list(flat.clone()), flatten_step)),
        rest: Box::new(BodyExpr::Let {
            // R: the replacements applied over them. C(0) replacements(1).
            bound: Box::new(replacements_applied(bound(0), bound(1))),
            rest: Box::new(BodyExpr::Let {
                // S: owner-sorted. R(0) C(1).
                bound: Box::new(BodyExpr::SortBy {
                    list: Box::new(bound(0)),
                    key: Box::new(field("owner")(bound(0))),
                }),
                rest: Box::new(BodyExpr::Let {
                    // P: the policy's entries. S(0) R(1) C(2), so the policy
                    // input is Bound(5).
                    bound: Box::new(BodyExpr::Unwrap {
                        nominal: Box::new(bound(5)),
                    }),
                    rest: Box::new(BodyExpr::Wrap {
                        declared: nominal_type_of(&stele::unit()),
                        // P(0) S(1).
                        representation: Box::new(record(&[
                            (
                                "after",
                                merged_field(
                                    "after",
                                    list(Type::Text),
                                    policy_behavior(bound(0), "after"),
                                    bound(1),
                                ),
                            ),
                            (
                                "description",
                                merged_field(
                                    "description",
                                    Type::Text,
                                    policy_behavior(bound(0), "description"),
                                    bound(1),
                                ),
                            ),
                            (
                                "exec",
                                merged_field(
                                    "exec",
                                    Type::Text,
                                    policy_behavior(bound(0), "exec"),
                                    bound(1),
                                ),
                            ),
                            (
                                "name",
                                merged_field(
                                    "name",
                                    Type::Text,
                                    policy_behavior(bound(0), "name"),
                                    bound(1),
                                ),
                            ),
                            (
                                "wants",
                                merged_field(
                                    "wants",
                                    list(Type::Text),
                                    policy_behavior(bound(0), "wants"),
                                    bound(1),
                                ),
                            ),
                        ])),
                    }),
                }),
            }),
        }),
    });
    assert_expressed("stele", "compose-unit", &interface, &body);
}

// ---------------------------------------------------------------------------
// stele: the renders
// ---------------------------------------------------------------------------

#[test]
fn stele_render_unit_is_expressed() {
    let interface = stele::render_unit_interface();
    let parts = BodyExpr::Unwrap {
        nominal: Box::new(bound(0)),
    };
    let joined_by_space = |list_expr: BodyExpr| {
        fold(
            list_expr,
            lit(text("")),
            if_(
                equal(bound(1), lit(text(""))),
                bound(0),
                concat(&[bound(1), lit(text(" ")), bound(0)]),
            ),
        )
    };
    let body = RuleBody::new(BodyExpr::Wrap {
        declared: nominal_type_of(&stele::unit_text()),
        representation: Box::new(concat(&[
            lit(text("[Unit]\n")),
            lit(text("Description=")),
            field("description")(parts.clone()),
            lit(text("\n")),
            if_(
                equal(field("after")(parts.clone()), empty_list(Type::Text)),
                lit(text("")),
                concat(&[
                    lit(text("After=")),
                    joined_by_space(field("after")(parts.clone())),
                    lit(text("\n")),
                ]),
            ),
            if_(
                equal(field("wants")(parts.clone()), empty_list(Type::Text)),
                lit(text("")),
                concat(&[
                    lit(text("Wants=")),
                    joined_by_space(field("wants")(parts.clone())),
                    lit(text("\n")),
                ]),
            ),
            lit(text("\n[Service]\n")),
            lit(text("ExecStart=")),
            field("exec")(parts),
            lit(text("\n")),
        ])),
    });
    assert_expressed("stele", "render-unit", &interface, &body);
}

/// The user-table projection. The host body narrows uid and gid to a machine
/// integer because its formatter takes one, refusing ids outside that range;
/// the represented body renders the arbitrary-precision decimal instead and
/// accepts what the host body refuses. The narrowing is a formatter boundary,
/// not a designed contract, and integer comparison — the constructor that
/// would restore the refusal — stays out until a domain needs it for its own
/// sake.
#[test]
fn stele_render_passwd_is_expressed() {
    let interface = stele::render_passwd_interface();
    let line = concat(&[
        field("name")(bound(0)),
        lit(text(":x:")),
        describe(field("uid")(bound(0))),
        lit(text(":")),
        describe(field("gid")(bound(0))),
        lit(text("::")),
        field("home")(bound(0)),
        lit(text(":")),
        field("shell")(bound(0)),
        lit(text("\n")),
    ]);
    let body = RuleBody::new(BodyExpr::Wrap {
        declared: nominal_type_of(&stele::passwd_text()),
        representation: Box::new(fold(
            BodyExpr::Unwrap {
                nominal: Box::new(bound(0)),
            },
            lit(text("")),
            BodyExpr::TextConcat {
                left: Box::new(bound(1)),
                right: Box::new(line),
            },
        )),
    });
    assert_expressed("stele", "render-passwd", &interface, &body);
}

#[test]
fn stele_render_boot_is_expressed() {
    let interface = stele::render_boot_interface();
    let record_of_boot = BodyExpr::Unwrap {
        nominal: Box::new(bound(0)),
    };
    let body = RuleBody::new(BodyExpr::Wrap {
        declared: nominal_type_of(&stele::boot_text()),
        representation: Box::new(concat(&[
            lit(text("title ")),
            field("machine")(record_of_boot.clone()),
            lit(text("\nlinux ")),
            field("kernel")(record_of_boot.clone()),
            lit(text("\ninitrd ")),
            field("initrd")(record_of_boot),
            lit(text("\n")),
        ])),
    });
    assert_expressed("stele", "render-boot", &interface, &body);
}

// ---------------------------------------------------------------------------
// phloem: package-library and package-build
// ---------------------------------------------------------------------------

/// The own include set: each declared include resolved against the tree,
/// path-sorted with adjacent duplicates collapsed. `tree` and `build` are
/// correct at the caller's scope.
fn own_includes(tree: BodyExpr, build: BodyExpr) -> BodyExpr {
    fn found_entry(entry: BodyExpr) -> BodyExpr {
        entry
    }
    let header = xylem::provided_header_record();
    // The map fold pushes two binders over the caller's scope, and the lookup
    // inside pushes two more over those.
    // A resolved include is the tree entry itself: its path and content are
    // the header record the compile request declares.
    let resolved = map_prepend(field("includes")(build), header.clone(), |include| {
        first_matching(
            phloem::tree_entry(),
            shift(&tree, 2),
            |entry| equal(field("path")(entry), shift(&include, 2)),
            found_entry,
            &[lit(text("the tree does not hold the offered include"))],
        )
    });
    let sorted = BodyExpr::SortBy {
        list: Box::new(resolved),
        key: Box::new(field("path")(bound(0))),
    };
    fold(
        sorted,
        empty_list(header),
        BodyExpr::MatchList {
            list: Box::new(bound(1)),
            empty: Box::new(BodyExpr::Cons {
                head: Box::new(bound(0)),
                tail: Box::new(bound(1)),
            }),
            cons: Box::new(if_(
                equal(field("path")(bound(0)), field("path")(bound(2))),
                bound(3),
                BodyExpr::Cons {
                    head: Box::new(bound(2)),
                    tail: Box::new(bound(3)),
                },
            )),
        },
    )
}

/// The prescribed sources resolved against the tree, in link order, refusing
/// an empty prescription.
fn resolved_sources(tree: BodyExpr, build: BodyExpr) -> BodyExpr {
    let contents = map_prepend(field("sources")(build.clone()), Type::Blob, |source| {
        first_matching(
            phloem::tree_entry(),
            shift(&tree, 2),
            |entry| equal(field("path")(entry), shift(&source, 2)),
            field("content"),
            &[lit(text("the tree does not hold the prescribed source"))],
        )
    });
    if_(
        equal(field("sources")(build), empty_list(Type::Text)),
        fail(&[lit(text(
            "the build prescribes no source, and a package with nothing to compile has no \
             procedure to run",
        ))]),
        reverse(Type::Blob, contents),
    )
}

#[test]
fn phloem_package_library_is_expressed() {
    let interface = phloem::package_library_interface();
    // Inputs: toolchain(2) tree(1) build(0).
    let body = RuleBody::new(BodyExpr::Let {
        // S: the resolved sources, in link order.
        bound: Box::new(resolved_sources(bound(1), bound(0))),
        rest: Box::new(BodyExpr::Let {
            // P: the own include set. S(0) build(1) tree(2) toolchain(3).
            bound: Box::new(own_includes(bound(2), bound(1))),
            rest: Box::new(BodyExpr::NeedEach {
                source: Box::new(bound(1)),
                request: request(
                    xylem::compile_interface(),
                    &[
                        // toolchain(5), the source element wrapped as CSource,
                        // and the provided set one binder below the element.
                        bound(5),
                        BodyExpr::Wrap {
                            declared: nominal_type_of(&xylem::c_source()),
                            representation: Box::new(bound(0)),
                        },
                        bound(1),
                    ],
                ),
                // Under [objects(0) P(1) S(2) build(3) tree(4) toolchain(5)].
                resume: Box::new(record(&[("headers", bound(1)), ("objects", bound(0))])),
            }),
        }),
    });
    assert_expressed("phloem", "package-library", &interface, &body);
}

/// The merged header set: the package's own includes joined with every
/// dependency's offered headers, path-sorted, agreeing duplicates collapsed,
/// and one spelling naming two contents the refusal the merge names both
/// digests for. `own` and `libraries` are correct at the caller's scope.
fn merged_provided(own: BodyExpr, libraries: BodyExpr) -> BodyExpr {
    let header = xylem::provided_header_record();
    let everything = fold(
        libraries,
        own,
        BodyExpr::Append {
            left: Box::new(field("headers")(bound(0))),
            right: Box::new(bound(1)),
        },
    );
    let sorted = BodyExpr::SortBy {
        list: Box::new(everything),
        key: Box::new(field("path")(bound(0))),
    };
    fold(
        sorted,
        empty_list(header),
        BodyExpr::MatchList {
            list: Box::new(bound(1)),
            empty: Box::new(BodyExpr::Cons {
                head: Box::new(bound(0)),
                tail: Box::new(bound(1)),
            }),
            cons: Box::new(if_(
                equal(field("path")(bound(0)), field("path")(bound(2))),
                if_(
                    equal(field("content")(bound(0)), field("content")(bound(2))),
                    bound(3),
                    fail(&[
                        lit(text("the include path `")),
                        describe(field("path")(bound(2))),
                        lit(text("` resolves to two contents, `")),
                        describe(field("content")(bound(0))),
                        lit(text("` and `")),
                        describe(field("content")(bound(2))),
                        lit(text("`: one spelling cannot name two headers")),
                    ]),
                ),
                BodyExpr::Cons {
                    head: Box::new(bound(2)),
                    tail: Box::new(bound(3)),
                },
            )),
        },
    )
}

#[test]
fn phloem_package_build_is_expressed() {
    let interface = phloem::package_build_interface();
    // Inputs: toolchain(3) tree(2) build(1) dependencies(0).
    let body = RuleBody::new(BodyExpr::NeedEach {
        // One library per dependency, the batch's count data.
        source: Box::new(bound(0)),
        request: request(
            phloem::package_library_interface(),
            &[bound(4), field("tree")(bound(0)), field("build")(bound(0))],
        ),
        resume: Box::new(BodyExpr::Let {
            // D: the dependencies' objects, in dependency order. Under
            // [libraries(0) dependencies(1) build(2) tree(3) toolchain(4)].
            bound: Box::new(fold(
                bound(0),
                empty_list(xylem::object()),
                BodyExpr::Append {
                    left: Box::new(field("objects")(bound(0))),
                    right: Box::new(bound(1)),
                },
            )),
            rest: Box::new(BodyExpr::Let {
                // S: the resolved sources. D(0) libraries(1) dependencies(2)
                // build(3) tree(4) toolchain(5).
                bound: Box::new(resolved_sources(bound(4), bound(3))),
                rest: Box::new(BodyExpr::Let {
                    // P: the merged provided headers. S(0) D(1) libraries(2)
                    // dependencies(3) build(4) tree(5) toolchain(6).
                    bound: Box::new(merged_provided(own_includes(bound(5), bound(4)), bound(2))),
                    rest: Box::new(BodyExpr::NeedEach {
                        source: Box::new(bound(1)),
                        request: request(
                            xylem::compile_interface(),
                            &[
                                bound(8),
                                BodyExpr::Wrap {
                                    declared: nominal_type_of(&xylem::c_source()),
                                    representation: Box::new(bound(0)),
                                },
                                bound(1),
                            ],
                        ),
                        // Under [own objects(0) P(1) S(2) D(3) libraries(4)
                        // dependencies(5) build(6) tree(7) toolchain(8)].
                        resume: Box::new(BodyExpr::Need {
                            request: request(
                                xylem::link_interface(),
                                // The dependent's objects link before its
                                // dependencies', so a symbol the package
                                // defines resolves in its own objects first.
                                &[
                                    bound(8),
                                    BodyExpr::Append {
                                        left: Box::new(bound(0)),
                                        right: Box::new(bound(3)),
                                    },
                                ],
                            ),
                            resume: Box::new(bound(0)),
                        }),
                    }),
                }),
            }),
        }),
    });
    assert_expressed("phloem", "package-build", &interface, &body);
}

// ---------------------------------------------------------------------------
// The inventory
// ---------------------------------------------------------------------------

/// Every corpus body this file expresses.
const EXPRESSED: &[&str] = &[
    "xylem.link-entry",
    "xylem.generate-entry",
    "xylem.test-entry",
    "stele.compose-etc",
    "stele.compose-users",
    "stele.compose-unit",
    "stele.compose-system",
    "stele.render-unit",
    "stele.render-passwd",
    "stele.render-boot",
    "phloem.package-library",
    "phloem.package-build",
];

/// The corpus is the fifteen pure rule bodies the four first-party domains
/// register: twelve expressed above, three named with what each waits for.
#[test]
fn the_corpus_is_fifteen_bodies() {
    assert_eq!(EXPRESSED.len() + NAMED.len(), 15);
    let mut named: Vec<&str> = NAMED.iter().map(|(rule, _)| *rule).collect();
    named.sort_unstable();
    let mut overlap: Vec<&str> = EXPRESSED
        .iter()
        .filter(|expressed| named.contains(expressed))
        .copied()
        .collect();
    overlap.sort_unstable();
    assert!(
        overlap.is_empty(),
        "a body is both expressed and named: {overlap:?}"
    );
}
