//! Durable records and persistence adapter boundary for engine metadata.
//!
//! The live [`crate::Engine`] publishes every computation that leaves the
//! `Pending` state through this interface and revalidates recorded pure reuse
//! before re-evaluation (decision 0024). A sqlite adapter, durable hydration,
//! crash recovery, and cross-process reuse remain unimplemented.

mod memory;
mod records;
mod store;

pub use memory::MemoryEngineStateStore;
pub use records::*;
pub use store::{
    EngineStateError, EngineStateStore, ExpectedReuseDecision, InvalidActionLifecycleReason,
    InvalidDependencyReason,
};
