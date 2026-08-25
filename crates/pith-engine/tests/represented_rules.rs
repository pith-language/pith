use pith_core::{
    BodyExpr, BodyRequest, BodyRevision, DeclarationTable, Interface, MatchArm, Pure, RecordField,
    Request, Rule, RuleBody, SumConstructor, Type, Value,
};
use pith_diag::{EngineCode, Span, StableCode};
use pith_engine::{Engine, Evaluation, PureRule, PureRuleFrame, PureStep, Resumption};

fn interface(inputs: impl Into<Box<[Type]>>, output: Type) -> Interface {
    Interface {
        inputs: inputs.into(),
        output,
    }
}

fn request(interface: Interface, inputs: impl Into<Box<[Value]>>) -> Request<Pure> {
    Request::new("represented", interface, inputs, Span::none())
}

fn evaluate(body: RuleBody, interface: Interface, inputs: impl Into<Box<[Value]>>) -> Value {
    let mut engine = Engine::new();
    let registered = engine.register_represented_rule(
        "represented-tests",
        "body",
        interface.clone(),
        Span::none(),
        body,
    );
    assert!(registered.is_ok());
    let result = engine.evaluate_pure(&request(interface, inputs));
    assert!(result.is_ok());
    match result {
        Ok(evaluation) => evaluation.value,
        Err(_) => Value::Unit,
    }
}

fn evaluation_value(result: pith_diag::PithResult<Evaluation>) -> Value {
    assert!(result.is_ok());
    match result {
        Ok(evaluation) => evaluation.value,
        Err(_) => Value::Unit,
    }
}

#[test]
fn scalar_expressions_evaluate() {
    let signature = interface([Type::Int], Type::Text);
    let calculated = BodyExpr::IntMultiply {
        left: Box::new(BodyExpr::IntAdd {
            left: Box::new(BodyExpr::Bound(0)),
            right: Box::new(BodyExpr::Literal(Value::int(2))),
        }),
        right: Box::new(BodyExpr::Literal(Value::int(3))),
    };
    let body = RuleBody::new(BodyExpr::Let {
        bound: Box::new(calculated),
        rest: Box::new(BodyExpr::If {
            condition: Box::new(BodyExpr::Equal {
                left: Box::new(BodyExpr::IntSubtract {
                    left: Box::new(BodyExpr::Bound(0)),
                    right: Box::new(BodyExpr::Literal(Value::int(6))),
                }),
                right: Box::new(BodyExpr::IntMultiply {
                    left: Box::new(BodyExpr::Bound(1)),
                    right: Box::new(BodyExpr::Literal(Value::int(3))),
                }),
            }),
            then: Box::new(BodyExpr::TextConcat {
                left: Box::new(BodyExpr::Describe {
                    value: Box::new(BodyExpr::Bound(0)),
                }),
                right: Box::new(BodyExpr::TextOfBytes {
                    bytes: Box::new(BodyExpr::Literal(Value::Bytes(Box::from(b"!".as_slice())))),
                }),
            }),
            otherwise: Box::new(BodyExpr::Fail {
                message: Box::new(BodyExpr::Literal(Value::Text("wrong branch".into()))),
            }),
        }),
    });

    assert_eq!(
        evaluate(body, signature, [Value::int(4)]),
        Value::Text("18!".into())
    );
}

#[test]
fn declared_values_and_structural_list_operations_evaluate() {
    let mut declarations = DeclarationTable::new("represented-tests");
    let numbers = declarations
        .nominal("Numbers", Type::List(Box::new(Type::Int)))
        .ok()
        .and_then(|declared| match declared {
            Type::Nominal(numbers) => Some(numbers),
            _ => None,
        })
        .unwrap();
    let choice = declarations
        .sum(
            "Choice",
            [
                SumConstructor {
                    name: "None".into(),
                    payload: None,
                },
                SumConstructor {
                    name: "Some".into(),
                    payload: Some(Type::Nominal(numbers.clone())),
                },
            ],
        )
        .ok()
        .and_then(|declared| match declared {
            Type::Sum(choice) => Some(choice),
            _ => None,
        })
        .unwrap();
    let list = BodyExpr::Append {
        left: Box::new(BodyExpr::Cons {
            head: Box::new(BodyExpr::Literal(Value::int(3))),
            tail: Box::new(BodyExpr::List {
                element: Type::Int,
                items: Box::new([BodyExpr::Literal(Value::int(1))]),
            }),
        }),
        right: Box::new(BodyExpr::List {
            element: Type::Int,
            items: Box::new([
                BodyExpr::Literal(Value::int(2)),
                BodyExpr::Literal(Value::int(1)),
            ]),
        }),
    };
    let sorted = BodyExpr::SortBy {
        list: Box::new(list),
        key: Box::new(BodyExpr::Bound(0)),
    };
    let wrapped = BodyExpr::Wrap {
        declared: (*numbers).clone(),
        representation: Box::new(sorted),
    };
    let selected = BodyExpr::MakeSum {
        declared: (*choice).clone(),
        constructor: "Some".into(),
        payload: Some(Box::new(wrapped)),
    };
    let body = RuleBody::new(BodyExpr::Match {
        scrutinee: Box::new(selected),
        arms: Box::new([
            MatchArm {
                constructor: "None".into(),
                body: Box::new(BodyExpr::Fail {
                    message: Box::new(BodyExpr::Literal(Value::Text("empty".into()))),
                }),
            },
            MatchArm {
                constructor: "Some".into(),
                body: Box::new(BodyExpr::Let {
                    bound: Box::new(BodyExpr::Unwrap {
                        nominal: Box::new(BodyExpr::Bound(0)),
                    }),
                    rest: Box::new(BodyExpr::MatchList {
                        list: Box::new(BodyExpr::Bound(0)),
                        empty: Box::new(BodyExpr::Literal(Value::int(0))),
                        cons: Box::new(BodyExpr::Fold {
                            source: Box::new(BodyExpr::Cons {
                                head: Box::new(BodyExpr::Bound(0)),
                                tail: Box::new(BodyExpr::Bound(1)),
                            }),
                            init: Box::new(BodyExpr::Literal(Value::int(0))),
                            step: Box::new(BodyExpr::IntAdd {
                                left: Box::new(BodyExpr::Bound(1)),
                                right: Box::new(BodyExpr::Bound(0)),
                            }),
                        }),
                    }),
                }),
            },
        ]),
    });

    assert_eq!(evaluate(body, interface([], Type::Int), []), Value::int(7));
}

#[test]
fn sort_by_is_stable_for_equal_keys() {
    let row_type = Type::record([
        RecordField {
            name: "key".into(),
            payload: Type::Bool,
        },
        RecordField {
            name: "value".into(),
            payload: Type::Text,
        },
    ]);
    let row_type = row_type.unwrap();
    let row = |key, value: &str| {
        BodyExpr::record([
            RecordField {
                name: "key".into(),
                payload: BodyExpr::Literal(Value::Bool(key)),
            },
            RecordField {
                name: "value".into(),
                payload: BodyExpr::Literal(Value::Text(value.into())),
            },
        ])
    };
    let rows: Box<[BodyExpr]> = [
        row(false, "first"),
        row(false, "second"),
        row(true, "third"),
    ]
    .into_iter()
    .map(|row| row.unwrap())
    .collect();
    let body = RuleBody::new(BodyExpr::SortBy {
        list: Box::new(BodyExpr::List {
            element: row_type.clone(),
            items: rows,
        }),
        key: Box::new(BodyExpr::Field {
            record: Box::new(BodyExpr::Bound(0)),
            name: "key".into(),
        }),
    });
    let result = evaluate(body, interface([], Type::List(Box::new(row_type))), []);
    let rows = match result {
        Value::List(rows) => Some(rows),
        _ => None,
    }
    .unwrap();
    let values: Vec<&str> = rows
        .iter()
        .map(|row| match row {
            Value::Record(fields) => fields
                .iter()
                .find(|field| field.name.as_ref() == "value")
                .and_then(|field| match &field.payload {
                    Value::Text(value) => Some(value.as_ref()),
                    _ => None,
                })
                .unwrap_or("missing value"),
            _ => "not a row",
        })
        .collect();
    assert_eq!(values, ["first", "second", "third"]);
}

struct Echo;

impl PureRule for Echo {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(EchoFrame {
            value: inputs.first().cloned(),
        })
    }
}

struct EchoFrame {
    value: Option<Value>,
}

impl PureRuleFrame for EchoFrame {
    fn step(&mut self, _: Option<Resumption>) -> pith_diag::PithResult<PureStep> {
        match self.value.take() {
            Some(value) => Ok(PureStep::Complete(value)),
            None => Err(pith_diag::DiagnosticSink::new()),
        }
    }
}

#[test]
fn pure_requests_and_dynamic_batches_resume_the_body() {
    let echo_interface = interface([Type::Int], Type::Int);
    let batch_interface = interface(
        [Type::List(Box::new(Type::Int))],
        Type::List(Box::new(Type::Int)),
    );
    let batch_body = RuleBody::new(BodyExpr::NeedEach {
        source: Box::new(BodyExpr::Bound(0)),
        request: BodyRequest {
            interface: echo_interface.clone(),
            inputs: Box::new([BodyExpr::Bound(0)]),
        },
        resume: Box::new(BodyExpr::Bound(0)),
    });
    let nested_interface = interface([Type::Int, Type::Unit], Type::Int);
    let nested_body = RuleBody::new(BodyExpr::Need {
        request: BodyRequest {
            interface: echo_interface.clone(),
            inputs: Box::new([BodyExpr::Bound(1)]),
        },
        resume: Box::new(BodyExpr::NeedAll {
            requests: Box::new([
                BodyRequest {
                    interface: echo_interface.clone(),
                    inputs: Box::new([BodyExpr::Bound(0)]),
                },
                BodyRequest {
                    interface: echo_interface.clone(),
                    inputs: Box::new([BodyExpr::Bound(2)]),
                },
            ]),
            resume: Box::new(BodyExpr::IntAdd {
                left: Box::new(BodyExpr::Bound(0)),
                right: Box::new(BodyExpr::Bound(1)),
            }),
        }),
    });
    let mut engine = Engine::new();
    let echo = Rule::declared(
        "represented-tests",
        "echo",
        BodyRevision(1),
        echo_interface,
        Span::none(),
    );
    engine.register_rule(echo, Echo);
    assert!(
        engine
            .register_represented_rule(
                "represented-tests",
                "batch",
                batch_interface.clone(),
                Span::none(),
                batch_body,
            )
            .is_ok()
    );
    assert!(
        engine
            .register_represented_rule(
                "represented-tests",
                "nested",
                nested_interface.clone(),
                Span::none(),
                nested_body,
            )
            .is_ok()
    );

    let batch = evaluation_value(engine.evaluate_pure(&request(
        batch_interface,
        [Value::List(Box::new([Value::int(1), Value::int(2)]))],
    )));
    assert_eq!(batch, Value::List(Box::new([Value::int(1), Value::int(2)])));
    let nested = evaluation_value(
        engine.evaluate_pure(&request(nested_interface, [Value::int(4), Value::Unit])),
    );
    assert_eq!(nested, Value::int(8));
}

#[test]
fn content_reads_have_a_synchronous_entry_point() {
    let signature = interface([Type::Blob], Type::Text);
    let body = RuleBody::new(BodyExpr::NeedBlob {
        content: Box::new(BodyExpr::Bound(0)),
        resume: Box::new(BodyExpr::TextOfBytes {
            bytes: Box::new(BodyExpr::Bound(0)),
        }),
    });
    let mut engine = Engine::new();
    let content = engine.put_blob(b"from the store").unwrap();
    assert!(
        engine
            .register_represented_rule(
                "represented-tests",
                "content",
                signature.clone(),
                Span::none(),
                body,
            )
            .is_ok()
    );

    let evaluation =
        evaluation_value(engine.evaluate_with_content(&request(signature, [Value::Blob(content)])));
    assert_eq!(evaluation, Value::Text("from the store".into()));
}

fn assert_pure_entry_refuses(body: RuleBody, expected_interface: Interface) {
    let mut engine = Engine::new();
    assert!(
        engine
            .register_represented_rule(
                "represented-tests",
                "effectful",
                expected_interface.clone(),
                Span::none(),
                body,
            )
            .is_ok()
    );
    let result = engine.evaluate_pure(&request(expected_interface, []));
    assert!(result.is_err());
    let diagnostics = match result {
        Err(diagnostics) => diagnostics,
        Ok(_) => pith_diag::DiagnosticSink::new(),
    };
    assert_eq!(
        diagnostics.iter().next().map(|diagnostic| diagnostic.code),
        Some(StableCode::from(EngineCode::EffectfulStepInPure))
    );
}

#[test]
fn action_and_observation_requests_reach_the_step_protocol() {
    let action_interface = interface([], Type::Unit);
    assert_pure_entry_refuses(
        RuleBody::new(BodyExpr::NeedAction {
            request: BodyRequest {
                interface: action_interface,
                inputs: Box::new([]),
            },
            resume: Box::new(BodyExpr::Bound(0)),
        }),
        interface([], Type::Unit),
    );

    let observation_interface = interface([], Type::Unit);
    assert_pure_entry_refuses(
        RuleBody::new(BodyExpr::NeedObservation {
            request: BodyRequest {
                interface: observation_interface,
                inputs: Box::new([]),
            },
            resume: Box::new(BodyExpr::Bound(0)),
        }),
        interface([], Type::Unit),
    );
}

#[test]
fn declared_failures_use_the_represented_body_code() {
    let body = RuleBody::new(BodyExpr::Fail {
        message: Box::new(BodyExpr::Literal(Value::Text("refused".into()))),
    });
    let signature = interface([], Type::Unit);
    let mut engine = Engine::new();
    assert!(
        engine
            .register_represented_rule(
                "represented-tests",
                "failure",
                signature.clone(),
                Span::none(),
                body,
            )
            .is_ok()
    );
    let result = engine.evaluate_pure(&request(signature, []));
    assert!(result.is_err());
    let diagnostics = match result {
        Err(diagnostics) => diagnostics,
        Ok(_) => pith_diag::DiagnosticSink::new(),
    };
    let diagnostic = diagnostics.iter().next();
    assert_eq!(
        diagnostic.map(|diagnostic| diagnostic.code),
        Some(StableCode::from(EngineCode::RepresentedBodyFailed))
    );
    assert_eq!(
        diagnostic.map(|diagnostic| diagnostic.message.0.as_ref()),
        Some("refused")
    );
}

#[test]
fn an_invalid_body_is_not_registered() {
    let mut engine = Engine::new();
    let result = engine.register_represented_rule(
        "represented-tests",
        "invalid",
        interface([], Type::Int),
        Span::none(),
        RuleBody::new(BodyExpr::Literal(Value::Bool(true))),
    );

    assert!(result.is_err());
    assert_eq!(engine.query().rules().count(), 0);
}
