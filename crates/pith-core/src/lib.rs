//! Typed semantic IR for the pith kernel: values, types, effects, rules.
//!
//! Pure data; evaluation lives in `pith-engine`. Splitting data from behavior
//! lets a future alternate engine, test harness, or doc generator reuse this
//! IR without pulling in the scheduler.

pub mod action;
mod action_codec;
pub mod codec;
pub mod effect;
mod manifest;
pub mod rule;
pub mod value;
mod value_codec;

pub use action::{
    ActionInput, ActionInputContent, ActionOutput, ActionProgram, ActionSpec,
    CapabilityRequirement, Content, EnvironmentVariable, ExitStatusContract, NetworkPolicy,
    OutputKind, PlatformRequirement,
};
pub use effect::{Action, EffectCategory, Mutation, Observation, Opaque, Pure};
pub use pith_ids::{RuleIdentity, RuleRevision};
pub use rule::{
    ActionComputationKey, Interface, PureComputationKey, Request, Rule, RuleArena, RuleId,
    SelectOutcome, select_rule,
};
pub use value::{DuplicateFieldError, RecordField, Type, Value, ValueArena, ValueId};
pub use value_codec::{CanonicalDecodeError, MAX_NOMINAL_NESTING};

use pith_output::{IntoOutput, OutputRecord};

impl IntoOutput for Value {
    fn to_record(&self) -> OutputRecord {
        OutputRecord::result(self.describe())
    }
}
