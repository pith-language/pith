mod evaluate;
mod iteration;
mod request;
mod state;

use pith_core::{BodyExpr, RuleBody, Value};
use pith_diag::{Diag, DiagnosticSink, EngineCode, PithResult, Span};

use crate::{PureRule, PureRuleFrame, PureStep, Resumption};
use evaluate::evaluate;
use state::{Environment, Evaluation, Resume};

pub(crate) struct RepresentedRule {
    body: RuleBody,
}

impl RepresentedRule {
    pub(crate) fn new(body: RuleBody) -> Self {
        Self { body }
    }
}

impl PureRule for RepresentedRule {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(RepresentedFrame {
            state: FrameState::Ready {
                expression: self.body.expression().clone(),
                environment: Environment::from_inputs(inputs),
            },
        })
    }
}

enum FrameState {
    Ready {
        expression: BodyExpr,
        environment: Environment,
    },
    Suspended(Resume<Value>),
    Complete,
}

struct RepresentedFrame {
    state: FrameState,
}

impl PureRuleFrame for RepresentedFrame {
    fn step(&mut self, resumption: Option<Resumption>) -> PithResult<PureStep> {
        let state = std::mem::replace(&mut self.state, FrameState::Complete);
        let evaluation = match (state, resumption) {
            (
                FrameState::Ready {
                    expression,
                    environment,
                },
                None,
            ) => evaluate(expression, environment),
            (FrameState::Suspended(resume), Some(resumption)) => resume(resumption),
            (FrameState::Ready { .. }, Some(_)) => {
                return Err(internal_failure(
                    "represented rule received a resumption before yielding",
                ));
            }
            (FrameState::Suspended(_), None) => {
                return Err(internal_failure(
                    "represented rule was stepped without its resumption",
                ));
            }
            (FrameState::Complete, _) => {
                return Err(internal_failure(
                    "represented rule was stepped after completion",
                ));
            }
        };

        match evaluation {
            Evaluation::Complete(value) => Ok(PureStep::Complete(value)),
            Evaluation::Yield { step, resume } => {
                self.state = FrameState::Suspended(resume);
                Ok(step)
            }
            Evaluation::Failed(diagnostics) => Err(diagnostics),
        }
    }
}

pub(crate) fn body_failure(message: impl Into<Box<str>>) -> DiagnosticSink {
    one_diagnostic(Diag::engine(
        EngineCode::RepresentedBodyFailed,
        Span::none(),
        message,
    ))
}

pub(crate) fn internal_failure(message: impl Into<Box<str>>) -> DiagnosticSink {
    one_diagnostic(Diag::engine(
        EngineCode::InternalInvariant,
        Span::none(),
        message,
    ))
}

fn one_diagnostic(diagnostic: Diag) -> DiagnosticSink {
    let mut diagnostics = DiagnosticSink::new();
    diagnostics.push(diagnostic);
    diagnostics
}
