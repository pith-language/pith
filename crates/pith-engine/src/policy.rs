use crate::ActionPlan;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionAuthorization {
    Allowed { policy: Box<str> },
    Denied { policy: Box<str>, reason: Box<str> },
}

pub trait ActionPolicy: Send + Sync {
    fn authorize(&self, plan: &ActionPlan) -> ActionAuthorization;
}

#[derive(Copy, Clone, Debug, Default)]
pub struct AllowAllActions;

impl ActionPolicy for AllowAllActions {
    fn authorize(&self, _plan: &ActionPlan) -> ActionAuthorization {
        ActionAuthorization::Allowed {
            policy: "allow-all-actions".into(),
        }
    }
}
