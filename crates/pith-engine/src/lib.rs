//! The pith kernel engine.
//!
//! Owns the arena dependency graph, the synchronous step machine that
//! evaluates the `Pure` fragment, and the async scheduler that drives
//! `Action` / `Observation` / `Mutation` / `Opaque` (decisions 0021, 0022).
//!
//! M-1 scope: rule registration, interface-match selection (decision 0015),
//! cycle detection as a structured diagnostic (decision 0018), and an
//! in-memory query interface (requirement K-12).

pub mod graph;
pub mod runtime;

pub use graph::{
    ComputationNode, DependencyEdge, Engine, Evaluation, PureRule, PureRuleFrame, PureStep,
};
pub use runtime::Runtime;

use pith_output::{IntoOutput, OutputRecord, Payload, PhaseStatus};

pub struct PhaseEvent {
    pub name: Box<str>,
    pub status: PhaseStatus,
}

impl IntoOutput for PhaseEvent {
    fn to_record(&self) -> OutputRecord {
        OutputRecord {
            kind: pith_output::RecordKind::Phase,
            code: 0,
            payload: Payload::Phase {
                name: self.name.clone(),
                status: self.status,
            },
        }
    }
}
