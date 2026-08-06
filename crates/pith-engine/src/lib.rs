//! The pith kernel engine.
//!
//! Owns the arena dependency graph, the synchronous step machine that
//! evaluates the `Pure` fragment, and the async scheduler that drives
//! `Action` / `Observation` / `Mutation` / `Opaque` (decisions 0021, 0022).

pub mod action;
pub mod graph;
pub mod policy;
pub mod runtime;

pub use action::{
    AccessVerification, ActionExecution, ActionInvocation, ActionRule, CapturedActionExecution,
    CapturedExecutionReport, CapturedFileContent, CapturedOutput, CapturedOutputContent,
    CapturedTree, CapturedTreeEntry, CapturedTreeEntryContent, ExecutionPlatform, ExecutionReport,
    Executor, MaterializedActionInput, MaterializedBlob, MaterializedContent,
    MaterializedFileContent, MaterializedTree, MaterializedTreeEntry, MaterializedTreeEntryContent,
    ProducedOutput,
};
pub use graph::{
    ActionPlan, ActionRecord, AttemptState, ComputationKind, ComputationNode, DependencyEdge,
    Engine, EngineQuery, Evaluation, EvaluationSource, PureRule, PureRuleFrame, PureStep,
    ReuseDecision, ReuseReason, RuleSelection,
};
pub use policy::{ActionAuthorization, ActionPolicy, AllowAllActions};
pub use runtime::{Runtime, RuntimeError, TokioRuntime};

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
