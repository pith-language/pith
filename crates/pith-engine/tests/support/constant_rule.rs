use pith_core::Value;
use pith_diag::PithResult;
use pith_engine::{PureRule, PureRuleFrame, PureStep, Resumption};

pub struct ConstantRule(pub Value);

impl PureRule for ConstantRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(ConstantFrame(self.0.clone()))
    }
}

pub struct ConstantFrame(pub Value);

impl PureRuleFrame for ConstantFrame {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        Ok(PureStep::Complete(self.0.clone()))
    }
}
