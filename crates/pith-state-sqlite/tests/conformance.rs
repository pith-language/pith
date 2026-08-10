//! The adapter against the reference model (decision 0025).
//!
//! The normalized schema is a second spelling of the durable record types. This
//! is what binds the two: generated operation sequences must produce the same
//! outcomes and the same reads here as they do in the in-memory adapter.

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
