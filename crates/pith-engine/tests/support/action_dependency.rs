use pith_core::{Action, Request, Value};
use pith_diag::PithResult;
use pith_engine::{PureRule, PureRuleFrame, PureStep, Resumption};

pub struct ActionDepRule {
    pub dependency: Request<Action>,
}

impl PureRule for ActionDepRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(ActionDepFrame {
            dependency: self.dependency.clone(),
            requested: false,
        })
    }
}

struct ActionDepFrame {
    dependency: Request<Action>,
    requested: bool,
}

impl PureRuleFrame for ActionDepFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::NeedAction(self.dependency.clone()));
        }
        input
            .and_then(Resumption::one)
            .map(PureStep::Complete)
            .ok_or_else(|| {
                super::diagnostic::fixture_error("action dependency completed without a value")
            })
    }
}
