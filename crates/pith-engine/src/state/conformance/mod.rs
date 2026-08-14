//! A cross-adapter conformance suite for [`EngineStateStore`] (decision 0025).
//!
//! Adapters choose their own representation, so a generated sequence of store
//! operations is applied both to the adapter under test and to
//! [`MemoryEngineStateStore`] as a reference model. Every outcome and every
//! subsequent read must agree.
//!
//! Attempt identifiers are store-local. Records and errors are compared after
//! translating the adapter's identifiers into the model's, so an adapter that
//! allocates in a different order still conforms.
//!
//! ```ignore
//! proptest! {
//!     #[test]
//!     fn matches_the_reference_model(scenario in conformance::scenario()) {
//!         let mut store = MyStore::open_in_memory()?;
//!         conformance::check(&scenario, &mut store)?;
//!     }
//! }
//! ```

mod compare;
mod fixtures;
mod record;
mod run;
mod scenario;
mod translate;

pub use compare::{Divergence, DivergenceDetail};
pub use scenario::{GeneratedDependency, Scenario, Step, scenario};

use crate::MemoryEngineStateStore;
use crate::state::EngineStateStore;

use compare::compare_reads;
use run::{Tracked, run_step};

/// Apply `scenario` to `subject` and to a fresh reference model, and check that
/// they agree at every step and on every read afterwards.
///
/// # Errors
/// Returns the first [`Divergence`] observed.
pub fn check(scenario: &Scenario, subject: &dyn EngineStateStore) -> Result<(), Divergence> {
    let model = MemoryEngineStateStore::default();
    let mut tracked: Vec<Tracked> = Vec::new();

    for (index, step) in scenario.steps.iter().enumerate() {
        run_step(index, step, &model, subject, &mut tracked)?;
    }
    compare_reads(scenario.steps.len(), &model, subject, &tracked)
}

#[cfg(test)]
mod tests {
    use super::fixtures::pure_key;
    use super::*;
    use pith_core::{RecordField, Value};
    use proptest::prelude::*;

    /// The generator has to produce records shared validation accepts, or the
    /// suite would conform two adapters on nothing but rejections.
    #[test]
    fn the_generator_produces_publishable_records() {
        let scenario = Scenario {
            steps: Box::new([
                Step::CreatePure { rule: 0, input: 0 },
                Step::Complete {
                    attempt: 0,
                    dependencies: Box::new([]),
                    result: Value::Int(1),
                    corrupt_reuse: false,
                },
                Step::CreatePure { rule: 1, input: 0 },
                Step::Complete {
                    attempt: 0,
                    dependencies: Box::new([GeneratedDependency::Pure(0)]),
                    result: Value::Record(
                        [RecordField {
                            name: "result".into(),
                            payload: Value::Int(2),
                        }]
                        .into(),
                    ),
                    corrupt_reuse: false,
                },
                Step::CreateAction {
                    rule: 0,
                    executable: 1,
                    capabilities: Box::new([2, 1]),
                    denied: false,
                },
                Step::Complete {
                    attempt: 0,
                    dependencies: Box::new([GeneratedDependency::Blob(3)]),
                    result: Value::Int(3),
                    corrupt_reuse: false,
                },
            ]),
        };

        let model = MemoryEngineStateStore::default();
        let subject = MemoryEngineStateStore::default();
        let mut tracked = Vec::new();
        for (index, step) in scenario.steps.iter().enumerate() {
            if let Err(divergence) = run_step(index, step, &model, &subject, &mut tracked) {
                unreachable!("the fixture scenario diverged: {divergence}");
            }
        }

        assert_eq!(tracked.len(), 3);
        assert_eq!(
            model.pending_attempts().map(|pending| pending.len()).ok(),
            Some(0),
            "a fixture attempt never left Pending, so its publication was rejected"
        );
        let reusable = model.latest_completed_reusable_attempt(pure_key(0, 0));
        assert!(
            reusable.ok().flatten().is_some(),
            "a completed reusable pure attempt did not enter the reusable index"
        );
    }

    proptest! {
        /// Anything flagged when both stores are the reference model is a bug
        /// in the harness.
        #[test]
        fn the_reference_model_conforms_to_itself(scenario in scenario()) {
            let subject = MemoryEngineStateStore::default();
            if let Err(divergence) = check(&scenario, &subject) {
                unreachable!("the reference model diverged from itself: {divergence}");
            }
        }
    }
}
