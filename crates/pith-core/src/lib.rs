//! Typed semantic IR for the pith kernel: values, types, effects, rules.
//!
//! Pure data; evaluation lives in `pith-engine`. Splitting data from behavior
//! lets a future alternate engine, test harness, or doc generator reuse this
//! IR without pulling in the scheduler.

pub mod action;
pub mod effect;
mod manifest;
pub mod rule;
pub mod value;

pub use action::{
    ActionInput, ActionInputContent, ActionOutput, ActionOutputKind, ActionSpec,
    CapabilityRequirement, EnvironmentVariable, NetworkPolicy, PlatformRequirement,
};
pub use effect::{Action, EffectCategory, Mutation, Observation, Opaque, Pure};
pub use pith_ids::{RuleIdentity, RuleRevision};
pub use rule::{
    Interface, PureComputationKey, Request, Rule, RuleArena, RuleId, SelectOutcome, select_rule,
};
pub use value::{Type, Value, ValueArena, ValueId};

use pith_output::{IntoOutput, OutputRecord};

impl IntoOutput for Value {
    fn to_record(&self) -> OutputRecord {
        OutputRecord {
            kind: pith_output::RecordKind::Result,
            code: 0,
            payload: pith_output::Payload::Result {
                summary: self.describe().into(),
            },
        }
    }
}
