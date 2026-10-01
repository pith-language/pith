use pith_core::{BodyExpr, Value};

use super::evaluate::evaluate;
use super::state::{Environment, Evaluation, Frame};

/// Fold `values` left to right under `step`, starting from `accumulator`.
/// The element sits at `Bound(0)`, the accumulator at `Bound(1)`.
pub(super) fn continue_fold(
    mut values: std::vec::IntoIter<Value>,
    mut accumulator: Value,
    step: BodyExpr,
    environment: Environment,
) -> Evaluation {
    loop {
        let Some(element) = values.next() else {
            return Evaluation::Complete(accumulator);
        };
        let step_environment = environment.clone().with(accumulator).with(element);
        match evaluate(step.clone(), step_environment) {
            Evaluation::Complete(next) => accumulator = next,
            Evaluation::Yield {
                step: yielded,
                then,
            } => {
                return Evaluation::Yield {
                    step: yielded,
                    then: then.pushed(Frame::FoldStep {
                        pending: values,
                        step,
                        environment,
                    }),
                };
            }
            Evaluation::Failed(diagnostics) => return Evaluation::Failed(diagnostics),
        }
    }
}

/// Sort `values` by the canonical encoding of the key `key` maps each to.
pub(super) fn evaluate_sort_keys(
    mut values: std::vec::IntoIter<Value>,
    key: BodyExpr,
    environment: Environment,
    mut keyed: Vec<(Vec<u8>, Value)>,
) -> Evaluation {
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
            Evaluation::Yield { step, then } => {
                return Evaluation::Yield {
                    step,
                    then: then.pushed(Frame::SortKey {
                        pending: values,
                        key,
                        environment,
                        keyed,
                        element: value,
                    }),
                };
            }
            Evaluation::Failed(diagnostics) => return Evaluation::Failed(diagnostics),
        }
    }
}
