//! The adapter against the reference model (decision 0025).
//!
//! The normalized schema is a second spelling of the durable record types. This
//! is what binds the two: generated operation sequences must produce the same
//! outcomes and the same reads here as they do in the in-memory adapter.

use pith_core::Value;
use pith_engine::state::conformance;
use pith_state_sqlite::SqliteEngineStateStore;
use proptest::prelude::*;

proptest! {
    #[test]
    fn generated_scenarios_match_the_reference_model(scenario in conformance::scenario()) {
        let store = match SqliteEngineStateStore::open_in_memory() {
            Ok(store) => store,
            Err(error) => unreachable!("could not open an engine-state database: {error}"),
        };
        if let Err(divergence) = conformance::check(&scenario, &store) {
            unreachable!("the sqlite adapter diverged from the reference model: {divergence}");
        }
    }
}

/// Two attempts of one action key, completed in the opposite of their
/// creation order. The reusable index must serve the attempt whose
/// publication came last — the lower identifier here — because 0031's
/// admission test reads "latest recorded", and attempt identifiers order
/// creation, not publication. The generated suite finds this only when a
/// draw completes two same-key attempts out of order; this scenario is the
/// same claim drawn deterministically.
#[test]
fn the_latest_reusable_action_attempt_is_the_latest_published() {
    let scenario = conformance::Scenario {
        steps: Box::new([
            conformance::Step::CreateAction {
                rule: 1,
                executable: 2,
                capabilities: Box::new([]),
                denied: false,
            },
            conformance::Step::CreateAction {
                rule: 1,
                executable: 2,
                capabilities: Box::new([]),
                denied: false,
            },
            // Completes the second-created attempt first.
            conformance::Step::Complete {
                attempt: 1,
                dependencies: Box::new([]),
                result: Value::Int(0),
                corrupt_reuse: false,
            },
            // Completes the first-created attempt last: this is the one the
            // reusable index must now serve.
            conformance::Step::Complete {
                attempt: 0,
                dependencies: Box::new([]),
                result: Value::Int(0),
                corrupt_reuse: false,
            },
        ]),
    };
    let store = match SqliteEngineStateStore::open_in_memory() {
        Ok(store) => store,
        Err(error) => unreachable!("could not open an engine-state database: {error}"),
    };
    if let Err(divergence) = conformance::check(&scenario, &store) {
        unreachable!("the sqlite adapter diverged from the reference model: {divergence}");
    }
}
