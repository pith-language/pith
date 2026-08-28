use pith_core::{Action, BodyExpr, Int, MatchArm, Observation, Pure, RecordField, Value};
use pith_diag::DiagnosticSink;

use crate::{PureStep, Resumption};

use super::iteration::{continue_fold, evaluate_expressions, evaluate_sort_keys};
use super::request::{
    evaluate_each_request, evaluate_request, evaluate_requests, yield_many, yield_one,
};
use super::state::{Environment, Evaluation};
use super::{body_failure, internal_failure};

pub(super) fn evaluate(expression: BodyExpr, environment: Environment) -> Evaluation<Value> {
    match expression {
        BodyExpr::Literal(value) => Evaluation::Complete(value),
        BodyExpr::Bound(index) => environment.get(index).map_or_else(
            || internal("represented body referenced an unavailable binder"),
            Evaluation::Complete,
        ),
        BodyExpr::Let { bound, rest } => evaluate(*bound, environment.clone())
            .and_then(move |value| evaluate(*rest, environment.with(value))),
        BodyExpr::Fail { message } => evaluate(*message, environment).and_then(|value| {
            expect_text(value).map_or_else(Evaluation::Failed, |message| {
                Evaluation::Failed(body_failure(message))
            })
        }),
        BodyExpr::Record { fields } => evaluate_record(fields, environment),
        BodyExpr::Field { record, name } => evaluate(*record, environment).and_then(move |value| {
            let Value::Record(fields) = value else {
                return internal("validated field access received a non-record value");
            };
            fields
                .into_iter()
                .find(|field| field.name == name)
                .map_or_else(
                    || internal("validated field access could not find its field"),
                    |field| Evaluation::Complete(field.payload),
                )
        }),
        BodyExpr::MakeSum {
            declared,
            constructor,
            payload,
        } => match payload {
            Some(payload) => evaluate(*payload, environment).and_then(move |payload| {
                Evaluation::Complete(Value::Sum {
                    type_name: declared.coordinate.spelling().into(),
                    constructor,
                    payload: Some(Box::new(payload)),
                })
            }),
            None => Evaluation::Complete(Value::Sum {
                type_name: declared.coordinate.spelling().into(),
                constructor,
                payload: None,
            }),
        },
        BodyExpr::Match { scrutinee, arms } => evaluate(*scrutinee, environment.clone())
            .and_then(move |value| evaluate_match(value, arms, environment)),
        BodyExpr::Wrap {
            declared,
            representation,
        } => evaluate(*representation, environment).and_then(move |representation| {
            Evaluation::Complete(Value::Nominal {
                name: declared.coordinate.spelling().into(),
                representation: Box::new(representation),
            })
        }),
        BodyExpr::Unwrap { nominal } => evaluate(*nominal, environment).and_then(|value| {
            let Value::Nominal { representation, .. } = value else {
                return internal("validated unwrap received a non-nominal value");
            };
            Evaluation::Complete(*representation)
        }),
        BodyExpr::List { items, .. } => evaluate_expressions(items, environment)
            .and_then(|items| Evaluation::Complete(Value::List(items.into_boxed_slice()))),
        BodyExpr::Cons { head, tail } => {
            evaluate(*head, environment.clone()).and_then(move |head| {
                evaluate(*tail, environment).and_then(move |tail| {
                    let Value::List(tail) = tail else {
                        return internal("validated cons received a non-list tail");
                    };
                    let mut values = Vec::with_capacity(tail.len().saturating_add(1));
                    values.push(head);
                    values.extend(tail);
                    Evaluation::Complete(Value::List(values.into_boxed_slice()))
                })
            })
        }
        BodyExpr::MatchList { list, empty, cons } => {
            evaluate(*list, environment.clone()).and_then(move |value| {
                let Value::List(values) = value else {
                    return internal("validated list match received a non-list value");
                };
                let mut values = values.into_iter();
                match values.next() {
                    Some(head) => evaluate(
                        *cons,
                        environment.with(Value::List(values.collect())).with(head),
                    ),
                    None => evaluate(*empty, environment),
                }
            })
        }
        BodyExpr::Append { left, right } => {
            evaluate(*left, environment.clone()).and_then(move |left| {
                evaluate(*right, environment).and_then(move |right| {
                    let (Value::List(left), Value::List(right)) = (left, right) else {
                        return internal("validated append received a non-list value");
                    };
                    let mut values = left.into_vec();
                    values.extend(right);
                    Evaluation::Complete(Value::List(values.into_boxed_slice()))
                })
            })
        }
        BodyExpr::Fold { source, init, step } => {
            evaluate(*source, environment.clone()).and_then(move |source| {
                let Value::List(values) = source else {
                    return internal("validated fold received a non-list source");
                };
                evaluate(*init, environment.clone()).and_then(move |initial| {
                    continue_fold(values.into_iter(), initial, *step, environment)
                })
            })
        }
        BodyExpr::SortBy { list, key } => {
            evaluate(*list, environment.clone()).and_then(move |list| {
                let Value::List(values) = list else {
                    return internal("validated sort received a non-list value");
                };
                evaluate_sort_keys(values.into_iter(), *key, environment, Vec::new())
            })
        }
        BodyExpr::If {
            condition,
            then,
            otherwise,
        } => evaluate(*condition, environment.clone()).and_then(move |condition| {
            let Value::Bool(condition) = condition else {
                return internal("validated condition received a non-boolean value");
            };
            if condition {
                evaluate(*then, environment)
            } else {
                evaluate(*otherwise, environment)
            }
        }),
        BodyExpr::Equal { left, right } => {
            evaluate_binary(*left, *right, environment, |left, right| {
                Evaluation::Complete(Value::Bool(left == right))
            })
        }
        BodyExpr::IntAdd { left, right } => {
            evaluate_integers(*left, *right, environment, |left, right| left.added(&right))
        }
        BodyExpr::IntSubtract { left, right } => {
            evaluate_integers(*left, *right, environment, |left, right| {
                left.subtracted(&right)
            })
        }
        BodyExpr::IntMultiply { left, right } => {
            evaluate_integers(*left, *right, environment, |left, right| {
                left.multiplied(&right)
            })
        }
        BodyExpr::Describe { value } => evaluate(*value, environment)
            .and_then(|value| Evaluation::Complete(Value::Text(value.describe().into()))),
        BodyExpr::TextConcat { left, right } => {
            evaluate_binary(*left, *right, environment, |left, right| {
                let (Ok(left), Ok(right)) = (expect_text(left), expect_text(right)) else {
                    return internal("validated text concatenation received a non-text value");
                };
                let mut joined = String::with_capacity(left.len().saturating_add(right.len()));
                joined.push_str(&left);
                joined.push_str(&right);
                Evaluation::Complete(Value::Text(joined.into_boxed_str()))
            })
        }
        BodyExpr::TextOfBytes { bytes } => evaluate(*bytes, environment).and_then(|value| {
            let Value::Bytes(bytes) = value else {
                return internal("validated UTF-8 decoding received a non-bytes value");
            };
            match String::from_utf8(bytes.into_vec()) {
                Ok(text) => Evaluation::Complete(Value::Text(text.into_boxed_str())),
                Err(error) => Evaluation::Failed(body_failure(format!(
                    "bytes are not UTF-8 at offset {}",
                    error.utf8_error().valid_up_to()
                ))),
            }
        }),
        BodyExpr::TextBreak { text, separator } => {
            evaluate_binary(*text, *separator, environment, |text, separator| {
                let (Ok(text), Ok(separator)) = (expect_text(text), expect_text(separator)) else {
                    return internal("validated text break received a non-text value");
                };
                // An empty separator never matches (decision 0064), and
                // Rust's `str::split` panics on one, so the decree and the
                // guard coincide.
                let parts: Vec<Value> = if separator.is_empty() {
                    vec![Value::Text(text)]
                } else {
                    text.split(separator.as_ref())
                        .map(|part| Value::Text(part.into()))
                        .collect()
                };
                Evaluation::Complete(Value::List(parts.into_boxed_slice()))
            })
        }
        BodyExpr::TextJoin { list, separator } => {
            evaluate_binary(*list, *separator, environment, |list, separator| {
                let (Ok(list), Ok(separator)) = (expect_text_list(list), expect_text(separator))
                else {
                    return internal("validated text join received a non-list or non-text value");
                };
                let mut joined = String::new();
                for (index, field) in list.iter().enumerate() {
                    if index > 0 {
                        joined.push_str(&separator);
                    }
                    joined.push_str(field);
                }
                Evaluation::Complete(Value::Text(joined.into_boxed_str()))
            })
        }
        BodyExpr::Need { request, resume } => {
            evaluate_request::<Pure>(request, environment.clone())
                .and_then(move |request| yield_one(PureStep::Need(request), *resume, environment))
        }
        BodyExpr::NeedAll { requests, resume } => {
            evaluate_requests::<Pure>(requests, environment.clone()).and_then(move |requests| {
                yield_many(
                    PureStep::NeedAll(requests.into_boxed_slice()),
                    *resume,
                    environment,
                )
            })
        }
        BodyExpr::NeedEach {
            source,
            request,
            resume,
        } => evaluate(*source, environment.clone()).and_then(move |source| {
            let Value::List(values) = source else {
                return internal("validated dynamic batch received a non-list source");
            };
            evaluate_each_request(values.into_iter(), request, environment.clone(), Vec::new())
                .and_then(move |requests| {
                    if requests.is_empty() {
                        return evaluate(*resume, environment.with(Value::List(Box::new([]))));
                    }
                    Evaluation::Yield {
                        step: PureStep::NeedAll(requests.into_boxed_slice()),
                        resume: Box::new(move |resumption| match resumption {
                            Resumption::Many(values) => {
                                evaluate(*resume, environment.with(Value::List(values)))
                            }
                            Resumption::One(_) => internal("dynamic batch resumed with one value"),
                        }),
                    }
                })
        }),
        BodyExpr::NeedBlob { content, resume } => {
            evaluate(*content, environment.clone()).and_then(move |content| {
                let Value::Blob(content) = content else {
                    return internal("validated content request received a non-blob value");
                };
                yield_one(PureStep::NeedBlob(content), *resume, environment)
            })
        }
        BodyExpr::NeedAction { request, resume } => {
            evaluate_request::<Action>(request, environment.clone()).and_then(move |request| {
                yield_one(PureStep::NeedAction(request), *resume, environment)
            })
        }
        BodyExpr::NeedObservation { request, resume } => {
            evaluate_request::<Observation>(request, environment.clone()).and_then(move |request| {
                yield_one(PureStep::NeedObservation(request), *resume, environment)
            })
        }
    }
}

fn evaluate_record(
    fields: Box<[RecordField<BodyExpr>]>,
    environment: Environment,
) -> Evaluation<Value> {
    let (names, expressions): (Vec<_>, Vec<_>) = fields
        .into_iter()
        .map(|field| (field.name, field.payload))
        .unzip();
    evaluate_expressions(expressions.into_boxed_slice(), environment).and_then(move |values| {
        let fields = names
            .into_iter()
            .zip(values)
            .map(|(name, payload)| RecordField { name, payload })
            .collect();
        Evaluation::Complete(Value::Record(fields))
    })
}

fn evaluate_match(
    value: Value,
    arms: Box<[MatchArm]>,
    environment: Environment,
) -> Evaluation<Value> {
    let Value::Sum {
        constructor,
        payload,
        ..
    } = value
    else {
        return internal("validated match received a non-sum value");
    };
    let Some(arm) = arms.into_iter().find(|arm| arm.constructor == constructor) else {
        return internal("validated match could not find its constructor arm");
    };
    match payload {
        Some(payload) => evaluate(*arm.body, environment.with(*payload)),
        None => evaluate(*arm.body, environment),
    }
}

fn evaluate_binary(
    left: BodyExpr,
    right: BodyExpr,
    environment: Environment,
    combine: impl FnOnce(Value, Value) -> Evaluation<Value> + Send + 'static,
) -> Evaluation<Value> {
    evaluate(left, environment.clone()).and_then(move |left| {
        evaluate(right, environment).and_then(move |right| combine(left, right))
    })
}

fn evaluate_integers(
    left: BodyExpr,
    right: BodyExpr,
    environment: Environment,
    operation: impl FnOnce(Int, Int) -> Int + Send + 'static,
) -> Evaluation<Value> {
    evaluate_binary(left, right, environment, move |left, right| {
        let (Value::Int(left), Value::Int(right)) = (left, right) else {
            return internal("validated integer operation received a non-integer value");
        };
        Evaluation::Complete(Value::Int(operation(left, right)))
    })
}

fn expect_text(value: Value) -> Result<Box<str>, DiagnosticSink> {
    match value {
        Value::Text(text) => Ok(text),
        _ => Err(internal_failure(
            "validated text expression produced a non-text value",
        )),
    }
}

fn expect_text_list(value: Value) -> Result<Box<[Box<str>]>, DiagnosticSink> {
    match value {
        Value::List(fields) => {
            let mut texts = Vec::with_capacity(fields.len());
            for field in fields {
                texts.push(expect_text(field)?);
            }
            Ok(texts.into_boxed_slice())
        }
        _ => Err(internal_failure(
            "validated text join produced a non-list value",
        )),
    }
}

pub(super) fn internal<T>(message: &'static str) -> Evaluation<T> {
    Evaluation::Failed(internal_failure(message))
}
