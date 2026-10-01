use pith_core::{
    BodyExpr, BodyRequest, EffectCategory, EffectKind, Interface, Pure, Request, Value,
};
use pith_diag::Span;

use crate::PureStep;

use super::evaluate::evaluate;
use super::state::{Cont, Environment, Evaluation, Frame, RequestDraft, ResumeFrame};

/// The step that asks the engine for a request of `kind`: the kind's `Need`
/// construct, carrying the freshly built request.
fn request_step(kind: EffectKind, interface: Interface, inputs: Vec<Value>) -> PureStep {
    match kind {
        EffectKind::Pure => PureStep::Need(make_request(interface, inputs)),
        EffectKind::Action => PureStep::NeedAction(make_request(interface, inputs)),
        EffectKind::Observation => PureStep::NeedObservation(make_request(interface, inputs)),
    }
}

/// Build the request every yield construct makes: the same label everywhere,
/// since the label exists for diagnostics and selection uses the interface
/// alone.
fn make_request<K: EffectCategory>(interface: Interface, inputs: Vec<Value>) -> Request<K> {
    Request::new("represented request", interface, inputs, Span::none())
}

/// The request a batch instantiates for one element: the template's
/// interface and inputs, fresh for the element's bindings.
fn draft_for(template: &BodyRequest) -> RequestDraft {
    RequestDraft {
        interface: template.interface.clone(),
        pending: template.inputs.clone().into_iter(),
        values: Vec::new(),
    }
}

/// Evaluate one request's inputs, then yield for the request's result, which
/// binds at `Bound(0)` of `resume`.
pub(super) fn await_request(
    kind: EffectKind,
    request: BodyRequest,
    resume: Box<BodyExpr>,
    environment: Environment,
) -> Evaluation {
    let BodyRequest { interface, inputs } = request;
    drive_request_inputs(
        kind,
        resume,
        environment,
        RequestDraft {
            interface,
            pending: inputs.into_iter(),
            values: Vec::new(),
        },
    )
}

/// Drive the inputs of the single request being built, one suspension at a
/// time.
pub(super) fn drive_request_inputs(
    kind: EffectKind,
    resume: Box<BodyExpr>,
    environment: Environment,
    mut draft: RequestDraft,
) -> Evaluation {
    loop {
        let Some(input) = draft.pending.next() else {
            let RequestDraft {
                interface, values, ..
            } = draft;
            return Evaluation::Yield {
                step: request_step(kind, interface, values),
                then: Cont::awaiting(ResumeFrame::One {
                    body: resume,
                    environment,
                }),
            };
        };
        match evaluate(input, environment.clone()) {
            Evaluation::Complete(value) => draft.values.push(value),
            Evaluation::Yield { step, then } => {
                return Evaluation::Yield {
                    step,
                    then: then.pushed(Frame::RequestInputs {
                        kind,
                        resume,
                        environment,
                        draft,
                    }),
                };
            }
            Evaluation::Failed(diagnostics) => return Evaluation::Failed(diagnostics),
        }
    }
}

/// Evaluate the inputs of every request a static batch (`NeedAll`) declares,
/// then yield the batch, whose results bind one binder per request.
pub(super) fn await_all(
    requests: Box<[BodyRequest]>,
    resume: Box<BodyExpr>,
    environment: Environment,
) -> Evaluation {
    let mut pending_requests = requests.into_iter();
    let Some(BodyRequest { interface, inputs }) = pending_requests.next() else {
        return yield_batch(Vec::new(), resume, environment);
    };
    drive_static_batch(
        pending_requests,
        environment,
        resume,
        Vec::new(),
        RequestDraft {
            interface,
            pending: inputs.into_iter(),
            values: Vec::new(),
        },
    )
}

/// Drive the request a static batch is currently building, then the batch's
/// remaining requests, one suspension at a time.
pub(super) fn drive_static_batch(
    mut pending_requests: std::vec::IntoIter<BodyRequest>,
    environment: Environment,
    resume: Box<BodyExpr>,
    mut evaluated: Vec<Request<Pure>>,
    mut draft: RequestDraft,
) -> Evaluation {
    loop {
        let Some(input) = draft.pending.next() else {
            let RequestDraft {
                interface, values, ..
            } = draft;
            evaluated.push(make_request(interface, values));
            let Some(BodyRequest { interface, inputs }) = pending_requests.next() else {
                return yield_batch(evaluated, resume, environment);
            };
            draft = RequestDraft {
                interface,
                pending: inputs.into_iter(),
                values: Vec::new(),
            };
            continue;
        };
        match evaluate(input, environment.clone()) {
            Evaluation::Complete(value) => draft.values.push(value),
            Evaluation::Yield { step, then } => {
                return Evaluation::Yield {
                    step,
                    then: then.pushed(Frame::StaticBatch {
                        pending_requests,
                        environment,
                        evaluated,
                        resume,
                        draft,
                    }),
                };
            }
            Evaluation::Failed(diagnostics) => return Evaluation::Failed(diagnostics),
        }
    }
}

/// Evaluate the inputs of the request a dynamic batch (`NeedEach`)
/// instantiates for each element of `values`, then yield the batch, whose
/// results bind as one list at `Bound(0)` of `resume`.
pub(super) fn await_each(
    values: std::vec::IntoIter<Value>,
    template: BodyRequest,
    environment: Environment,
    resume: Box<BodyExpr>,
) -> Evaluation {
    let mut pending_values = values;
    let Some(element) = pending_values.next() else {
        return finish_dynamic(Vec::new(), resume, environment);
    };
    let element_environment = environment.clone().with(element);
    let draft = draft_for(&template);
    drive_dynamic_batch(
        pending_values,
        template,
        environment,
        element_environment,
        resume,
        Vec::new(),
        draft,
    )
}

/// Drive the request a dynamic batch is building for its current element,
/// then the requests for the elements still pending, one suspension at a
/// time.
pub(super) fn drive_dynamic_batch(
    mut pending_values: std::vec::IntoIter<Value>,
    template: BodyRequest,
    environment: Environment,
    mut element_environment: Environment,
    resume: Box<BodyExpr>,
    mut evaluated: Vec<Request<Pure>>,
    mut draft: RequestDraft,
) -> Evaluation {
    loop {
        let Some(input) = draft.pending.next() else {
            let RequestDraft {
                interface, values, ..
            } = draft;
            evaluated.push(make_request(interface, values));
            let Some(element) = pending_values.next() else {
                return finish_dynamic(evaluated, resume, environment);
            };
            element_environment = environment.clone().with(element);
            draft = draft_for(&template);
            continue;
        };
        match evaluate(input, element_environment.clone()) {
            Evaluation::Complete(value) => draft.values.push(value),
            Evaluation::Yield { step, then } => {
                return Evaluation::Yield {
                    step,
                    then: then.pushed(Frame::DynamicBatch {
                        pending_values,
                        template,
                        environment,
                        element_environment,
                        evaluated,
                        resume,
                        draft,
                    }),
                };
            }
            Evaluation::Failed(diagnostics) => return Evaluation::Failed(diagnostics),
        }
    }
}

/// Yield a static batch's requests, handing the engine one request per
/// binder the resumption will bind.
fn yield_batch(
    evaluated: Vec<Request<Pure>>,
    resume: Box<BodyExpr>,
    environment: Environment,
) -> Evaluation {
    Evaluation::Yield {
        step: PureStep::NeedAll(evaluated.into_boxed_slice()),
        then: Cont::awaiting(ResumeFrame::PerRequest {
            body: resume,
            environment,
        }),
    }
}

/// Finish a dynamic batch: an empty batch has nothing to ask the engine, so
/// its resumption is the empty list. Otherwise the batch is yielded and the
/// resumption binds the results as one list.
fn finish_dynamic(
    evaluated: Vec<Request<Pure>>,
    resume: Box<BodyExpr>,
    environment: Environment,
) -> Evaluation {
    if evaluated.is_empty() {
        return evaluate(*resume, environment.with(Value::List(Box::new([]))));
    }
    Evaluation::Yield {
        step: PureStep::NeedAll(evaluated.into_boxed_slice()),
        then: Cont::awaiting(ResumeFrame::AsList {
            body: resume,
            environment,
        }),
    }
}
