//! The data types of the dependency graph: steps, edges, nodes, and the
//! evaluator's private frame. Pure data; no engine logic.

use pith_core::{
    Action, ActionSpec, CapabilityRequirement, Interface, Pure, Request, RuleId, Value,
};
use pith_diag::{Diag, PithResult};
use pith_ids::{ActionSpecDigest, ComputationId, ContentId};
use smallvec::SmallVec;

/// One bounded transition made by a pure rule body.
#[derive(Clone, Debug)]
pub enum PureStep {
    /// Request the result of another pure rule.
    Need(Request<Pure>),
    /// Request several pure results that do not depend on one another. This is
    /// the only way a rule body can say "these are independent": a body that
    /// yields [`PureStep::Need`] twice has told the engine the second request
    /// may depend on the first result, so the two must be ordered. The engine
    /// evaluates the requests here as separate chains and resumes with
    /// [`Resumption::Many`], one value per request in the order given.
    NeedAll(Box<[Request<Pure>]>),
    /// Request the bytes of a content-addressed blob. The engine fetches the
    /// blob from its content store and resumes with `Value::Bytes`.
    NeedBlob(ContentId),
    /// Request the result of an action rule. The engine plans an inspectable
    /// contract, gives it to an executor, and resumes with the result.
    NeedAction(Request<Action>),
    /// Finish the rule body with a final value.
    Complete(Value),
}

/// What the engine hands back to a rule body to resume it. The shape mirrors
/// the step that suspended it, so a body that yielded one request cannot be
/// resumed with a batch or the reverse.
#[derive(Clone, Debug)]
pub enum Resumption {
    /// The result of the single [`PureStep::Need`], [`PureStep::NeedBlob`], or
    /// [`PureStep::NeedAction`] the previous step yielded.
    One(Value),
    /// The results of a [`PureStep::NeedAll`], in the order requested.
    Many(Box<[Value]>),
}

impl Resumption {
    /// The single value this resumption carries, or `None` if it is a batch.
    /// A body that never yields [`PureStep::NeedAll`] can read its input with
    /// this instead of matching on a shape it cannot receive.
    pub fn one(self) -> Option<Value> {
        match self {
            Self::One(value) => Some(value),
            Self::Many(_) => None,
        }
    }
}

/// Suspended state for one pure rule application. `Send` so the engine's
/// evaluation future can be driven on a multi-threaded runtime.
pub trait PureRuleFrame: Send {
    /// `input` is the resumption for the step yielded by the preceding call, or
    /// `None` for the first step.
    ///
    /// # Errors
    /// Returns structured diagnostics when the rule cannot produce its value.
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep>;
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
    CapabilityUse {
        capability: CapabilityRequirement,
    },
    Action {
        request: Request<Action>,
        computation: ComputationId,
    },
}

impl DependencyEdge {
    /// The computation this edge points at, if any. `Blob` and `CapabilityUse`
    /// edges point at non-computation dependencies.
    pub fn computation_id(&self) -> Option<ComputationId> {
        match self {
            Self::Request { computation, .. } | Self::Action { computation, .. } => {
                Some(*computation)
            }
            Self::Blob { .. } | Self::CapabilityUse { .. } => None,
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

/// Declared contract, authorization, and executor reports retained as action provenance.
pub struct ActionRecord {
    pub spec_digest: ActionSpecDigest,
    pub spec: ActionSpec,
    pub authorization: crate::ActionAuthorization,
    /// The executor's observation, retained before the engine imports outputs.
    pub executor_report: Option<crate::CapturedExecutionReport>,
    /// The same report after captured outputs have engine-owned content identities.
    pub imported_report: Option<crate::ExecutionReport>,
}

/// Lifecycle of one allocated computation attempt. The three terminal states
/// are distinct on purpose: an ordinary evaluation failure must not stand in
/// for cancellation, and cancellation must not be read as a failure.
#[derive(Clone, Debug)]
pub enum AttemptState {
    Pending,
    Complete {
        result: Value,
        reuse: ReuseDecision,
    },
    /// The computation ran and could not produce its result.
    Failed {
        diagnostics: Box<[Diag]>,
    },
    /// The computation was stopped before it could produce a result: the run
    /// was cancelled, or it ended while this computation was still in flight.
    /// Distinct from `Failed` because nothing about the computation itself is
    /// known to be wrong.
    Cancelled {
        diagnostics: Box<[Diag]>,
    },
}

/// Which terminal state a computation that stopped without a result is
/// recorded under. Carried explicitly rather than inferred, so a caller that
/// stops work says whether it is reporting a fault or a cancellation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum StopReason {
    Failed,
    Cancelled,
}

impl StopReason {
    /// The arena state this reason records.
    pub(crate) fn attempt_state(self, diagnostics: Box<[Diag]>) -> AttemptState {
        match self {
            Self::Failed => AttemptState::Failed { diagnostics },
            Self::Cancelled => AttemptState::Cancelled { diagnostics },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReuseDecision {
    Reusable,
    NotReusable(ReuseReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReuseReason {
    ActionCachingDisabled,
    DependencyPending { computation: ComputationId },
    DependencyNotReusable { computation: ComputationId },
    DependencyMissing { computation: ComputationId },
}

/// One rule application in the in-memory graph.
pub struct ComputationNode {
    pub kind: ComputationKind,
    pub rule: RuleId,
    pub dependencies: SmallVec<[DependencyEdge; 4]>,
    pub state: AttemptState,
    pub action: Option<ActionRecord>,
    pub capabilities: Box<[CapabilityRequirement]>,
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
    /// The rule body ran in this engine instance.
    Computed,
    /// A completed computation already in this engine's arena was reused.
    Reused,
    /// A completed attempt recorded by a previous engine instance was
    /// revalidated and loaded from engine state (decision 0024). The rule body
    /// did not run here, and this instance's arena holds no subgraph for it:
    /// its recorded dependency set lives on the durable attempt, reachable
    /// through [`crate::EngineQuery::durable_attempt_of`].
    Hydrated,
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
    pub(crate) resume_with: Option<Resumption>,
}

/// Why a completed computation is not reusable, as a chain over the live arena
/// graph. The live-graph mirror of the durable
/// [`InvalidationExplanation`](crate::state::InvalidationExplanation), keyed on
/// [`ComputationId`] (arena handles) rather than `DurableAttemptId`.
///
/// Does not derive `PartialEq`/`Eq` because [`DependencyEdge`] carries a
/// [`Request`] that does not implement them; the durable
/// side, which is what the conformance suite compares, does.
#[derive(Clone, Debug)]
pub struct LiveInvalidationExplanation {
    pub computation: ComputationId,
    pub reason: LiveInvalidationReason,
}

/// One node of a live invalidation chain. Built by
/// [`crate::EngineQuery::explain_invalidation`].
#[derive(Clone, Debug)]
pub enum LiveInvalidationReason {
    /// The computation's own reuse reason, when it does not name a dependency
    /// the chain can follow (for example `ActionCachingDisabled`).
    Leaf(ReuseReason),
    /// A dependency the computation named as the cause of its non-reuse, with
    /// the recursive explanation of why that dependency is itself not reusable.
    DependencyInvalidated {
        edge: DependencyEdge,
        child: Box<LiveInvalidationExplanation>,
    },
}
