//! Typed semantic IR for the pith kernel: values, types, effects, rules.
//!
//! Pure data; evaluation lives in `pith-engine`. Splitting data from behavior
//! lets a future alternate engine, test harness, or doc generator reuse this
//! IR without pulling in the scheduler.

pub mod effect;
pub mod rule;
pub mod value;

pub use effect::{Action, EffectCat, Mutation, Observation, Opaque, Pure};
pub use rule::{Interface, Request, Rule, RuleArena, RuleId, SelectOutcome};
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
