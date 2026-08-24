use pith_core::{BodyExpr, BodyRequest, EffectCategory, Pure, Request, Value};
use pith_diag::Span;

use crate::{PureStep, Resumption};

use super::evaluate::{evaluate, internal};
use super::iteration::evaluate_expressions;
use super::state::{Environment, Evaluation};

pub(super) fn evaluate_request<K>(
    request: BodyRequest,
    environment: Environment,
) -> Evaluation<Request<K>>
where
    K: EffectCategory + Send + 'static,
{
    evaluate_expressions(request.inputs, environment).and_then(move |inputs| {
        Evaluation::Complete(Request::new(
            "represented request",
            request.interface,
            inputs,
            Span::none(),
        ))
    })
}

pub(super) fn evaluate_requests<K>(
    requests: Box<[BodyRequest]>,
    environment: Environment,
) -> Evaluation<Vec<Request<K>>>
where
    K: EffectCategory + Send + 'static,
{
    continue_requests(requests.into_vec().into_iter(), environment, Vec::new())
}

fn continue_requests<K>(
    mut requests: std::vec::IntoIter<BodyRequest>,
    environment: Environment,
    mut evaluated: Vec<Request<K>>,
) -> Evaluation<Vec<Request<K>>>
where
    K: EffectCategory + Send + 'static,
{
    loop {
        let Some(request) = requests.next() else {
            return Evaluation::Complete(evaluated);
        };
        match evaluate_request(request, environment.clone()) {
            Evaluation::Complete(request) => evaluated.push(request),
            Evaluation::Yield { step, resume } => {
                return Evaluation::Yield {
                    step,
                    resume: Box::new(move |resumption| {
                        resume(resumption).and_then(move |request| {
                            evaluated.push(request);
                            continue_requests(requests, environment, evaluated)
                        })
                    }),
                };
            }
            Evaluation::Failed(diagnostics) => return Evaluation::Failed(diagnostics),
        }
    }
}

pub(super) fn evaluate_each_request(
    mut values: std::vec::IntoIter<Value>,
    request: BodyRequest,
    environment: Environment,
    mut requests: Vec<Request<Pure>>,
) -> Evaluation<Vec<Request<Pure>>> {
    loop {
        let Some(value) = values.next() else {
            return Evaluation::Complete(requests);
        };
        match evaluate_request(request.clone(), environment.clone().with(value)) {
            Evaluation::Complete(evaluated) => requests.push(evaluated),
            Evaluation::Yield { step, resume } => {
                return Evaluation::Yield {
                    step,
                    resume: Box::new(move |resumption| {
                        resume(resumption).and_then(move |evaluated| {
                            requests.push(evaluated);
                            evaluate_each_request(values, request, environment, requests)
                        })
                    }),
                };
            }
            Evaluation::Failed(diagnostics) => return Evaluation::Failed(diagnostics),
        }
    }
}

pub(super) fn yield_one(
    step: PureStep,
    resume: BodyExpr,
    environment: Environment,
) -> Evaluation<Value> {
    Evaluation::Yield {
        step,
        resume: Box::new(move |resumption| match resumption {
            Resumption::One(value) => evaluate(resume, environment.with(value)),
            Resumption::Many(_) => internal("single request resumed with a batch"),
        }),
    }
}

pub(super) fn yield_many(
    step: PureStep,
    resume: BodyExpr,
    environment: Environment,
) -> Evaluation<Value> {
    Evaluation::Yield {
        step,
        resume: Box::new(move |resumption| match resumption {
            Resumption::Many(values) => evaluate(
                resume,
                environment.with_all(values.into_vec().into_iter().rev()),
            ),
            Resumption::One(_) => internal("static batch resumed with one value"),
        }),
    }
}
