//! The pith kernel engine.
//!
//! Owns the arena dependency graph, the synchronous step machine that
//! evaluates the `Pure` fragment, and the async scheduler that drives
//! `Action` / `Observation` / `Mutation` / `Opaque` (decisions 0021, 0022).

pub mod action;
pub mod cancel;
pub mod graph;
pub mod policy;
pub mod runtime;
pub mod state;

pub use action::{
    AccessVerification, ActionExecution, ActionInvocation, ActionRule, CapturedActionExecution,
    CapturedExecutionReport, CapturedFileContent, CapturedOutput, CapturedOutputContent,
    CapturedTree, CapturedTreeEntry, CapturedTreeEntryContent, ExecutionPlatform, ExecutionReport,
    Executor, ExecutorIdentity, MaterializedActionInput, MaterializedBlob, MaterializedContent,
    MaterializedFileContent, MaterializedTree, MaterializedTreeEntry, MaterializedTreeEntryContent,
    ProducedOutput,
};
pub use cancel::{CancelSignal, NeverCancelled};
pub use graph::{
    ActionPlan, ActionRecord, AttemptState, ComputationKind, ComputationNode, DependencyEdge,
    Engine, EngineQuery, Evaluation, EvaluationSource, LiveInvalidationExplanation,
    LiveInvalidationReason, PureRule, PureRuleFrame, PureStep, Resumption, ReuseDecision,
    ReuseReason, RuleSelection,
};
pub use policy::{ActionAuthorization, ActionPolicy, AllowAllActions};
pub use runtime::{Runtime, RuntimeError, TokioRuntime};
pub use state::{EngineStateStore, MemoryEngineStateStore};

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
