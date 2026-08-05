//! The data types of the dependency graph: steps, edges, nodes, and the
//! evaluator's private frame. Pure data; no engine logic.

use pith_core::{
    Action, ActionSpec, CapabilityRequirement, Interface, Pure, Request, RuleId, Value,
};
use pith_diag::PithResult;
use pith_ids::{ActionSpecDigest, ComputationId, ContentId};
use smallvec::SmallVec;

/// One bounded transition made by a pure rule body.
#[derive(Clone, Debug)]
pub enum PureStep {
    /// Request the result of another pure rule.
    Need(Request<Pure>),
    /// Request the bytes of a content-addressed blob. The engine fetches the
    /// blob from its content store and resumes with `Value::Bytes`.
    NeedBlob(ContentId),
    /// Request the result of an action rule. The engine plans an inspectable
    /// contract, gives it to an executor, and resumes with the result.
    NeedAction(Request<Action>),
    /// Finish the rule body with a final value.
    Complete(Value),
}

/// Suspended state for one pure rule application. `Send` so the engine's
/// evaluation future can be driven on a multi-threaded runtime.
pub trait PureRuleFrame: Send {
    /// `input` is the value returned by the request yielded by the preceding
    /// step, or `None` for the first step.
    ///
    /// # Errors
    /// Returns structured diagnostics when the rule cannot produce its value.
    fn step(&mut self, input: Option<Value>) -> PithResult<PureStep>;
}

/// Executable body for a semantic rule. A fresh frame is created for every
/// rule application. `Send + Sync` keeps the engine usable from a
/// multi-threaded runtime.
pub trait PureRule: Send + Sync {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame>;
}

/// A dependency recorded while evaluating a computation.
#[derive(Clone, Debug)]
pub enum DependencyEdge {
    Request {
        request: Request<Pure>,
        computation: ComputationId,
    },
    Blob {
        id: ContentId,
    },
    Action {
        request: Request<Action>,
        computation: ComputationId,
    },
}

impl DependencyEdge {
    /// The computation this edge points at, if any. `Blob` edges point at
    /// content, not a computation.
    pub fn computation_id(&self) -> Option<ComputationId> {
        match self {
            Self::Request { computation, .. } | Self::Action { computation, .. } => {
                Some(*computation)
            }
            Self::Blob { .. } => None,
        }
    }
}

/// Which kind of rule produced a computation node.
#[derive(Clone, Debug)]
pub enum ComputationKind {
    Pure(Request<Pure>),
    Action(Request<Action>),
}

/// An action selected and planned without executing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionPlan {
    pub rule: RuleId,
    pub spec_digest: ActionSpecDigest,
    pub spec: ActionSpec,
}

/// Declared contract, authorization, and executor report retained as action provenance.
pub struct ActionRecord {
    pub spec_digest: ActionSpecDigest,
    pub spec: ActionSpec,
    pub authorization: crate::ActionAuthorization,
    pub report: Option<crate::ExecutionReport>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReuseDecision {
    Pending,
    Reusable,
    NotReusable(ReuseReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReuseReason {
    ActionCachingDisabled,
    PolicyDenied,
    DependencyPending { computation: ComputationId },
    DependencyNotReusable { computation: ComputationId },
    DependencyMissing { computation: ComputationId },
    FailedExecution,
}

/// One rule application in the in-memory graph.
pub struct ComputationNode {
    pub kind: ComputationKind,
    pub rule: RuleId,
    pub dependencies: SmallVec<[DependencyEdge; 4]>,
    pub result: Option<Value>,
    pub action: Option<ActionRecord>,
    pub capabilities: Box<[CapabilityRequirement]>,
    pub reuse: ReuseDecision,
}

/// A completed evaluation and the graph node that produced it.
#[derive(Clone, Debug)]
pub struct Evaluation {
    pub value: Value,
    pub computation: ComputationId,
    pub source: EvaluationSource,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EvaluationSource {
    Computed,
    Reused,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleSelection {
    pub rule: RuleId,
    pub interface: Interface,
}

/// The evaluator's private stack frame: one per in-flight rule application.
pub(crate) struct EvalFrame {
    pub(crate) computation: ComputationId,
    pub(crate) rule: RuleId,
    pub(crate) request: Request<Pure>,
    pub(crate) body: Box<dyn PureRuleFrame>,
    pub(crate) resume_with: Option<Value>,
}
