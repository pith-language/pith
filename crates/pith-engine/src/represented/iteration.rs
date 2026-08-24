use pith_core::{BodyExpr, Value};

use super::evaluate::evaluate;
use super::state::{Environment, Evaluation};

pub(super) fn evaluate_expressions(
    expressions: Box<[BodyExpr]>,
    environment: Environment,
) -> Evaluation<Vec<Value>> {
    continue_expressions(expressions.into_vec().into_iter(), environment, Vec::new())
}

fn continue_expressions(
    mut expressions: std::vec::IntoIter<BodyExpr>,
    environment: Environment,
    mut values: Vec<Value>,
) -> Evaluation<Vec<Value>> {
    loop {
        let Some(expression) = expressions.next() else {
            return Evaluation::Complete(values);
        };
        match evaluate(expression, environment.clone()) {
            Evaluation::Complete(value) => values.push(value),
            Evaluation::Yield { step, resume } => {
                return Evaluation::Yield {
                    step,
                    resume: Box::new(move |resumption| {
                        resume(resumption).and_then(move |value| {
                            values.push(value);
                            continue_expressions(expressions, environment, values)
                        })
                    }),
                };
            }
            Evaluation::Failed(diagnostics) => return Evaluation::Failed(diagnostics),
        }
    }
}

pub(super) fn continue_fold(
    mut values: std::vec::IntoIter<Value>,
    mut accumulator: Value,
    step: BodyExpr,
    environment: Environment,
) -> Evaluation<Value> {
    loop {
        let Some(element) = values.next() else {
            return Evaluation::Complete(accumulator);
        };
        let step_environment = environment.clone().with(accumulator).with(element);
        match evaluate(step.clone(), step_environment) {
            Evaluation::Complete(next) => accumulator = next,
            Evaluation::Yield {
                step: yielded,
                resume,
            } => {
                return Evaluation::Yield {
                    step: yielded,
                    resume: Box::new(move |resumption| {
                        resume(resumption)
                            .and_then(move |next| continue_fold(values, next, step, environment))
                    }),
                };
            }
            Evaluation::Failed(diagnostics) => return Evaluation::Failed(diagnostics),
        }
    }
}

pub(super) fn evaluate_sort_keys(
    mut values: std::vec::IntoIter<Value>,
    key: BodyExpr,
    environment: Environment,
    mut keyed: Vec<(Vec<u8>, Value)>,
) -> Evaluation<Value> {
    loop {
        let Some(value) = values.next() else {
            keyed.sort_by(|left, right| left.0.cmp(&right.0));
            return Evaluation::Complete(Value::List(
                keyed.into_iter().map(|(_, value)| value).collect(),
            ));
        };
        match evaluate(key.clone(), environment.clone().with(value.clone())) {
            Evaluation::Complete(sort_key) => {
                keyed.push((sort_key.encode_canonical(), value));
            }
            Evaluation::Yield { step, resume } => {
                return Evaluation::Yield {
                    step,
                    resume: Box::new(move |resumption| {
                        resume(resumption).and_then(move |sort_key| {
                            keyed.push((sort_key.encode_canonical(), value));
                            evaluate_sort_keys(values, key, environment, keyed)
                        })
                    }),
                };
            }
            Evaluation::Failed(diagnostics) => return Evaluation::Failed(diagnostics),
        }
    }
}
