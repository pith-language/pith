//! Durable records and persistence adapter boundary for engine metadata.
//!
//! The live [`crate::Engine`] is deliberately not wired to this interface yet.

mod memory;
mod records;
mod store;

pub use memory::MemoryEngineStateStore;
pub use records::*;
pub use store::{
    EngineStateError, EngineStateStore, ExpectedReuseDecision, InvalidActionLifecycleReason,
    InvalidDependencyReason,
};
