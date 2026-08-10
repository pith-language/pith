//! Durable records and persistence adapter boundary for engine metadata.
//!
//! The live [`crate::Engine`] publishes every computation that leaves the
//! `Pending` state through this interface and revalidates recorded pure reuse
//! before re-evaluation (decision 0024).
//!
//! An adapter chooses its own representation of these records and is held to
//! the in-memory one by [`conformance`] (decision 0025). The sqlite adapter
//! marks attempts left `Pending` by an interrupted owner as failed on reopen
//! (decision 0024).

/// Cross-adapter conformance suite (decision 0025). Available to other crates
/// through the `testing` feature; compiled unconditionally for this crate's own
/// tests.
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
    EngineStateError, EngineStateStore, ExpectedReuseDecision, InvalidActionLifecycleReason,
    InvalidDependencyReason,
};
