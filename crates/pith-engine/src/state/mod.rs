//! Durable records and persistence adapters for engine metadata. Adapters are
//! checked against the in-memory reference model by the conformance suite.

/// Cross-adapter conformance suite, exported by the `testing` feature.
#[cfg(any(test, feature = "testing"))]
pub mod conformance;
pub mod explain;
mod memory;
mod records;
mod store;
pub mod validate;

pub use memory::MemoryEngineStateStore;
pub use records::*;
pub use store::{
    AttemptStatistics, EngineStateError, EngineStateReader, EngineStateStore,
    ExpectedReuseDecision, InvalidActionLifecycleReason, InvalidDependencyReason,
};
