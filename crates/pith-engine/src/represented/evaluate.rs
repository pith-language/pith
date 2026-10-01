use std::collections::VecDeque;

use pith_core::{BodyExpr, EffectKind, Int, MatchArm, RecordField, Value};
use pith_diag::DiagnosticSink;

use crate::{PureStep, Resumption};

use super::iteration::{continue_fold, evaluate_sort_keys};
use super::request::{
    await_all, await_each, await_request, drive_dynamic_batch, drive_request_inputs,
    drive_static_batch,
};
use super::state::{BinaryFinish, Cont, Environment, Evaluation, Frame, ResumeFrame};
use super::{body_failure, internal_failure};

impl Evaluation {
    /// Sequence `frame` behind `self`: over a completed value the frame runs
    /// at once, onto a suspension it is pushed, and through a failure it is
    /// dropped.
    #[must_use]
    pub(super) fn bind(self, frame: Frame) -> Self {
        match self {
            Self::Complete(value) => apply(frame, value),
            Self::Yield { step, then } => Self::Yield {
                step,
                then: then.pushed(frame),
            },
            Self::Failed(diagnostics) => Self::Failed(diagnostics),
        }
    }
}

impl Cont {
    /// Resume a suspension: the head frame handles the engine's resumption,
    /// then the frames above it run innermost-first until the body yields
    /// again, completes, or fails.
    #[must_use]
    pub(super) fn resume(self, resumption: Resumption) -> Evaluation {
        let (head, frames) = self.into_parts();
        let evaluation = match head {
            ResumeFrame::One { body, environment } => match resumption {
                Resumption::One(value) => evaluate(*body, environment.with(value)),
                Resumption::Many(_) => internal("single request resumed with a batch"),
            },
            ResumeFrame::PerRequest { body, environment } => match resumption {
                Resumption::Many(values) => {
                    evaluate(*body, environment.with_all(values.into_iter().rev()))
                }
                Resumption::One(_) => internal("static batch resumed with one value"),
            },
            ResumeFrame::AsList { body, environment } => match resumption {
                Resumption::Many(values) => evaluate(*body, environment.with(Value::List(values))),
                Resumption::One(_) => internal("dynamic batch resumed with one value"),
            },
        };
        advance(evaluation, frames)
    }
}

/// Run `evaluation` under `frames`, the sequencing frames above it: a
/// completed value passes to the next frame, a suspension keeps its
/// continuation, a failure ends the run.
fn advance(mut evaluation: Evaluation, mut frames: VecDeque<Frame>) -> Evaluation {
    loop {
        evaluation = match evaluation {
            Evaluation::Complete(value) => match frames.pop_front() {
                Some(frame) => apply(frame, value),
                None => return Evaluation::Complete(value),
            },
            Evaluation::Yield { step, mut then } => {
                then.append_frames(frames);
                return Evaluation::Yield { step, then };
            }
            Evaluation::Failed(diagnostics) => return Evaluation::Failed(diagnostics),
        };
    }
}

/// Hand `value`, the value the work beneath `frame` produced, to that frame.
fn apply(frame: Frame, value: Value) -> Evaluation {
    match frame {
        Frame::Let { rest, environment } => evaluate(*rest, environment.with(value)),
        Frame::FailMessage => expect_text(value).map_or_else(Evaluation::Failed, |message| {
            Evaluation::Failed(body_failure(message))
        }),
        Frame::Field { name } => {
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
        }
        Frame::MakeSum {
            declared,
            constructor,
        } => Evaluation::Complete(Value::Sum {
            type_name: declared.coordinate.spelling().into(),
            constructor,
            payload: Some(Box::new(value)),
        }),
        Frame::Match { arms, environment } => evaluate_match(value, arms, environment),
        Frame::Wrap { declared } => Evaluation::Complete(Value::Nominal {
            name: declared.coordinate.spelling().into(),
            representation: Box::new(value),
        }),
        Frame::Unwrap => {
            let Value::Nominal { representation, .. } = value else {
                return internal("validated unwrap received a non-nominal value");
            };
            Evaluation::Complete(*representation)
        }
        Frame::Describe => Evaluation::Complete(Value::Text(value.describe().into())),
        Frame::TextOfBytes => {
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
        }
        Frame::If {
            then,
            otherwise,
            environment,
        } => {
            let Value::Bool(condition) = value else {
                return internal("validated condition received a non-boolean value");
            };
            if condition {
                evaluate(*then, environment)
            } else {
                evaluate(*otherwise, environment)
            }
        }
        Frame::ConsHead { tail, environment } => {
            evaluate(*tail, environment).bind(Frame::ConsTail { head: value })
        }
        Frame::ConsTail { head } => {
            let Value::List(tail) = value else {
                return internal("validated cons received a non-list tail");
            };
            let mut values = Vec::with_capacity(tail.len().saturating_add(1));
            values.push(head);
            values.extend(tail);
            Evaluation::Complete(Value::List(values.into_boxed_slice()))
        }
        Frame::MatchList {
            cons,
            empty,
            environment,
        } => {
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
        }
        Frame::BinaryLeft {
            finish,
            right,
            environment,
        } => evaluate(right, environment).bind(Frame::BinaryRight {
            finish,
            left: value,
        }),
        Frame::BinaryRight { finish, left } => finish_binary(finish, left, value),
        Frame::FoldSource {
            init,
            step,
            environment,
        } => {
            let Value::List(values) = value else {
                return internal("validated fold received a non-list source");
            };
            evaluate(*init, environment.clone()).bind(Frame::FoldStep {
                pending: values.into_iter(),
                step: *step,
                environment,
            })
        }
        Frame::FoldStep {
            pending,
            step,
            environment,
        } => continue_fold(pending, value, step, environment),
        Frame::SortBy { key, environment } => {
            let Value::List(values) = value else {
                return internal("validated sort received a non-list value");
            };
            evaluate_sort_keys(values.into_iter(), *key, environment, Vec::new())
        }
        Frame::SortKey {
            pending,
            key,
            environment,
            mut keyed,
            element,
        } => {
            keyed.push((value.encode_canonical(), element));
            evaluate_sort_keys(pending, key, environment, keyed)
        }
        Frame::ListItems {
            pending,
            environment,
            mut values,
        } => {
            values.push(value);
            drive_items(pending, environment, values)
        }
        Frame::RecordFields {
            names,
            pending,
            environment,
            mut values,
        } => {
            values.push(value);
            drive_fields(names, pending, environment, values)
        }
        Frame::RequestInputs {
            kind,
            resume,
            environment,
            mut draft,
        } => {
            draft.values.push(value);
            drive_request_inputs(kind, resume, environment, draft)
        }
        Frame::StaticBatch {
            pending_requests,
            environment,
            resume,
            evaluated,
            mut draft,
        } => {
            draft.values.push(value);
            drive_static_batch(pending_requests, environment, resume, evaluated, draft)
        }
        Frame::DynamicBatch {
            pending_values,
            template,
            environment,
            element_environment,
            resume,
            evaluated,
            mut draft,
        } => {
            draft.values.push(value);
            drive_dynamic_batch(
                pending_values,
                template,
                environment,
                element_environment,
                resume,
                evaluated,
                draft,
            )
        }
        Frame::EachSource {
            template,
            environment,
            resume,
        } => {
            let Value::List(values) = value else {
                return internal("validated dynamic batch received a non-list source");
            };
            await_each(values.into_iter(), template, environment, resume)
        }
        Frame::BlobContent {
            resume,
            environment,
        } => {
            let Value::Blob(content) = value else {
                return internal("validated content request received a non-blob value");
            };
            Evaluation::Yield {
                step: PureStep::NeedBlob(content),
                then: Cont::awaiting(ResumeFrame::One {
                    body: resume,
                    environment,
                }),
            }
        }
    }
}

/// Evaluate one pure expression under `environment` until it completes,
/// suspends, or fails.
pub(super) fn evaluate(expression: BodyExpr, environment: Environment) -> Evaluation {
    match expression {
        BodyExpr::Literal(value) => Evaluation::Complete(value),
        BodyExpr::Bound(index) => environment.get(index).map_or_else(
            || internal("represented body referenced an unavailable binder"),
            Evaluation::Complete,
        ),
        BodyExpr::Let { bound, rest } => {
            evaluate(*bound, environment.clone()).bind(Frame::Let { rest, environment })
        }
        BodyExpr::Fail { message } => evaluate(*message, environment).bind(Frame::FailMessage),
        BodyExpr::Record { fields } => evaluate_record(fields, environment),
        BodyExpr::Field { record, name } => {
            evaluate(*record, environment).bind(Frame::Field { name })
        }
        BodyExpr::MakeSum {
            declared,
            constructor,
            payload,
        } => match payload {
            Some(payload) => evaluate(*payload, environment).bind(Frame::MakeSum {
                declared,
                constructor,
            }),
            None => Evaluation::Complete(Value::Sum {
                type_name: declared.coordinate.spelling().into(),
                constructor,
                payload: None,
            }),
        },
        BodyExpr::Match { scrutinee, arms } => {
            evaluate(*scrutinee, environment.clone()).bind(Frame::Match { arms, environment })
        }
        BodyExpr::Wrap {
            declared,
            representation,
        } => evaluate(*representation, environment).bind(Frame::Wrap { declared }),
        BodyExpr::Unwrap { nominal } => evaluate(*nominal, environment).bind(Frame::Unwrap),
        BodyExpr::List { items, .. } => drive_items(items.into_iter(), environment, Vec::new()),
        BodyExpr::Cons { head, tail } => {
            evaluate(*head, environment.clone()).bind(Frame::ConsHead { tail, environment })
        }
        BodyExpr::MatchList { list, empty, cons } => {
            evaluate(*list, environment.clone()).bind(Frame::MatchList {
                cons,
                empty,
                environment,
            })
        }
        BodyExpr::Append { left, right } => {
            evaluate_binary(*left, *right, environment, BinaryFinish::Append)
        }
        BodyExpr::Fold { source, init, step } => {
            evaluate(*source, environment.clone()).bind(Frame::FoldSource {
                init,
                step,
                environment,
            })
        }
        BodyExpr::SortBy { list, key } => {
            evaluate(*list, environment.clone()).bind(Frame::SortBy { key, environment })
        }
        BodyExpr::If {
            condition,
            then,
            otherwise,
        } => evaluate(*condition, environment.clone()).bind(Frame::If {
            then,
            otherwise,
            environment,
        }),
        BodyExpr::Equal { left, right } => {
            evaluate_binary(*left, *right, environment, BinaryFinish::Equal)
        }
        BodyExpr::IntAdd { left, right } => {
            evaluate_binary(*left, *right, environment, BinaryFinish::IntAdd)
        }
        BodyExpr::IntSubtract { left, right } => {
            evaluate_binary(*left, *right, environment, BinaryFinish::IntSubtract)
        }
        BodyExpr::IntMultiply { left, right } => {
            evaluate_binary(*left, *right, environment, BinaryFinish::IntMultiply)
        }
        BodyExpr::Describe { value } => evaluate(*value, environment).bind(Frame::Describe),
        BodyExpr::TextConcat { left, right } => {
            evaluate_binary(*left, *right, environment, BinaryFinish::TextConcat)
        }
        BodyExpr::TextOfBytes { bytes } => evaluate(*bytes, environment).bind(Frame::TextOfBytes),
        BodyExpr::TextBreak { text, separator } => {
            evaluate_binary(*text, *separator, environment, BinaryFinish::TextBreak)
        }
        BodyExpr::TextJoin { list, separator } => {
            evaluate_binary(*list, *separator, environment, BinaryFinish::TextJoin)
        }
        BodyExpr::Need { request, resume } => {
            await_request(EffectKind::Pure, request, resume, environment)
        }
        BodyExpr::NeedAll { requests, resume } => await_all(requests, resume, environment),
        BodyExpr::NeedEach {
            source,
            request,
            resume,
        } => evaluate(*source, environment.clone()).bind(Frame::EachSource {
            template: request,
            environment,
            resume,
        }),
        BodyExpr::NeedBlob { content, resume } => {
            evaluate(*content, environment.clone()).bind(Frame::BlobContent {
                resume,
                environment,
            })
        }
        BodyExpr::NeedAction { request, resume } => {
            await_request(EffectKind::Action, request, resume, environment)
        }
        BodyExpr::NeedObservation { request, resume } => {
            await_request(EffectKind::Observation, request, resume, environment)
        }
    }
}

/// Evaluate a record's field payloads in canonical order, then assemble the
/// record from the values.
fn evaluate_record(fields: Box<[RecordField<BodyExpr>]>, environment: Environment) -> Evaluation {
    let (names, expressions): (Vec<_>, Vec<_>) = fields
        .into_iter()
        .map(|field| (field.name, field.payload))
        .unzip();
    drive_fields(
        names.into_iter(),
        expressions.into_iter(),
        environment,
        Vec::new(),
    )
}

/// Select and run the arm `value`'s constructor picks.
fn evaluate_match(value: Value, arms: Box<[MatchArm]>, environment: Environment) -> Evaluation {
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

/// Evaluate a list literal's items, collecting the values.
fn drive_items(
    mut pending: std::vec::IntoIter<BodyExpr>,
    environment: Environment,
    mut values: Vec<Value>,
) -> Evaluation {
    loop {
        let Some(item) = pending.next() else {
            return Evaluation::Complete(Value::List(values.into_boxed_slice()));
        };
        match evaluate(item, environment.clone()) {
            Evaluation::Complete(value) => values.push(value),
            Evaluation::Yield { step, then } => {
                return Evaluation::Yield {
                    step,
                    then: then.pushed(Frame::ListItems {
                        pending,
                        environment,
                        values,
                    }),
                };
            }
            Evaluation::Failed(diagnostics) => return Evaluation::Failed(diagnostics),
        }
    }
}

/// Evaluate a record's field payloads in canonical order, collecting the
/// values to pair with their names.
fn drive_fields(
    names: std::vec::IntoIter<Box<str>>,
    mut pending: std::vec::IntoIter<BodyExpr>,
    environment: Environment,
    mut values: Vec<Value>,
) -> Evaluation {
    loop {
        let Some(expression) = pending.next() else {
            let fields = names
                .into_iter()
                .zip(values)
                .map(|(name, payload)| RecordField { name, payload })
                .collect();
            return Evaluation::Complete(Value::Record(fields));
        };
        match evaluate(expression, environment.clone()) {
            Evaluation::Complete(value) => values.push(value),
            Evaluation::Yield { step, then } => {
                return Evaluation::Yield {
                    step,
                    then: then.pushed(Frame::RecordFields {
                        names,
                        pending,
                        environment,
                        values,
                    }),
                };
            }
            Evaluation::Failed(diagnostics) => return Evaluation::Failed(diagnostics),
        }
    }
}

/// Evaluate the left operand of a two-operand expression, holding the right
/// and the finish for when both have landed.
fn evaluate_binary(
    left: BodyExpr,
    right: BodyExpr,
    environment: Environment,
    finish: BinaryFinish,
) -> Evaluation {
    evaluate(left, environment.clone()).bind(Frame::BinaryLeft {
        finish,
        right,
        environment,
    })
}

/// Combine a two-operand expression's landed operands.
fn finish_binary(finish: BinaryFinish, left: Value, right: Value) -> Evaluation {
    match finish {
        BinaryFinish::Equal => Evaluation::Complete(Value::Bool(left == right)),
        BinaryFinish::IntAdd => finish_integers(Int::added, left, right),
        BinaryFinish::IntSubtract => finish_integers(Int::subtracted, left, right),
        BinaryFinish::IntMultiply => finish_integers(Int::multiplied, left, right),
        BinaryFinish::TextConcat => {
            let (Ok(left), Ok(right)) = (expect_text(left), expect_text(right)) else {
                return internal("validated text concatenation received a non-text value");
            };
            let mut joined = String::with_capacity(left.len().saturating_add(right.len()));
            joined.push_str(&left);
            joined.push_str(&right);
            Evaluation::Complete(Value::Text(joined.into_boxed_str()))
        }
        BinaryFinish::TextBreak => {
            let (Ok(text), Ok(separator)) = (expect_text(left), expect_text(right)) else {
                return internal("validated text break received a non-text value");
            };
            // An empty separator never matches anything, and Rust's
            // `str::split` panics on one, so handle the empty case first.
            let parts: Vec<Value> = if separator.is_empty() {
                vec![Value::Text(text)]
            } else {
                text.split(separator.as_ref())
                    .map(|part| Value::Text(part.into()))
                    .collect()
            };
            Evaluation::Complete(Value::List(parts.into_boxed_slice()))
        }
        BinaryFinish::TextJoin => {
            let (Ok(list), Ok(separator)) = (expect_text_list(left), expect_text(right)) else {
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
        }
        BinaryFinish::Append => {
            let (Value::List(left), Value::List(right)) = (left, right) else {
                return internal("validated append received a non-list value");
            };
            let mut values = left.into_vec();
            values.extend(right);
            Evaluation::Complete(Value::List(values.into_boxed_slice()))
        }
    }
}

/// Run a total integer operation over two landed operands.
fn finish_integers(operation: fn(&Int, &Int) -> Int, left: Value, right: Value) -> Evaluation {
    let (Value::Int(left), Value::Int(right)) = (left, right) else {
        return internal("validated integer operation received a non-integer value");
    };
    Evaluation::Complete(Value::Int(operation(&left, &right)))
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

pub(super) fn internal(message: &'static str) -> Evaluation {
    Evaluation::Failed(internal_failure(message))
}
