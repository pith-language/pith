//! The sqlite adapter for durable engine state (decisions 0024 and 0025).
//!
//! `pith-engine` owns the [`EngineStateStore`] interface and the durable record
//! types; this crate is one implementation of that interface and nothing else.
//! No sqlite or diesel type appears in a kernel signature, and the engine does
//! not depend on this crate — a host chooses it.
//!
//! Records are stored as normalized relations. A completed pure result and a
//! declared action contract keep the canonical bytes `pith-core` produced,
//! because each carries a digest those bytes must reproduce; everything else is
//! columns and rows, so dependency traversal, interrupted-attempt recovery, and
//! reachability are queries rather than scans.
//!
//! Creating an attempt writes `Pending` in one transaction. Completion or
//! failure writes the final state, dependency set, provenance, and reusable
//! index entry in one transaction, so a reader never sees a half-published
//! attempt. Publication validation is shared with the in-memory adapter, and
//! [`pith_engine::state::conformance`] checks that the two behave alike.
//!
//! [`EngineStateStore`]: pith_engine::state::EngineStateStore

mod columns;
mod rows;
mod schema;
mod store;

pub use store::{SqliteEngineStateStore, SqliteStateError};
