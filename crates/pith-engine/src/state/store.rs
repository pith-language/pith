use std::sync::Arc;

use pith_core::{ActionComputationKey, PureComputationKey};

use super::{
    CompletedAttempt, DurableAttempt, DurableAttemptId, DurableAttemptStatus, DurableComputation,
    StoppedAttempt,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineStateError {
    AttemptIdentifierExhausted,
    AttemptNotFound {
        attempt: DurableAttemptId,
    },
    AttemptNotPending {
        attempt: DurableAttemptId,
        status: DurableAttemptStatus,
    },
    InvalidDependency {
        attempt: DurableAttemptId,
        dependency: DurableAttemptId,
        reason: InvalidDependencyReason,
    },
    InvalidActionLifecycle {
        attempt: DurableAttemptId,
        reason: InvalidActionLifecycleReason,
    },
    InvalidReuseDecision {
        attempt: DurableAttemptId,
        expected: ExpectedReuseDecision,
    },
    CapabilityDependenciesMismatch {
        attempt: DurableAttemptId,
    },
    ProvenanceCategoryMismatch {
        attempt: DurableAttemptId,
    },
    Adapter {
        message: Box<str>,
    },
}

impl std::fmt::Display for EngineStateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AttemptIdentifierExhausted => {
                formatter.write_str("engine-state attempt identifiers are exhausted")
            }
            Self::AttemptNotFound { attempt } => {
                write!(formatter, "engine-state attempt {attempt} does not exist")
            }
            Self::AttemptNotPending { attempt, status } => {
                write!(
                    formatter,
                    "engine-state attempt {attempt} is {status:?}, not Pending"
                )
            }
            Self::InvalidDependency {
                attempt,
                dependency,
                reason,
            } => write!(
                formatter,
                "engine-state attempt {attempt} has invalid dependency {dependency}: {}",
                reason.description()
            ),
            Self::InvalidActionLifecycle { attempt, reason } => write!(
                formatter,
                "engine-state action attempt {attempt} has an invalid lifecycle: {}",
                reason.description()
            ),
            Self::InvalidReuseDecision { attempt, expected } => write!(
                formatter,
                "engine-state attempt {attempt} has a reuse decision inconsistent with its dependencies; expected {expected}"
            ),
            Self::CapabilityDependenciesMismatch { attempt } => write!(
                formatter,
                "engine-state action attempt {attempt} has capability-use edges inconsistent with its executor report"
            ),
            Self::ProvenanceCategoryMismatch { attempt } => write!(
                formatter,
                "engine-state attempt {attempt} has provenance for a different effect category"
            ),
            Self::Adapter { message } => formatter.write_str(message),
        }
    }
}

impl std::error::Error for EngineStateError {}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum InvalidDependencyReason {
    SelfReference,
    MissingAttempt,
    PendingAttempt,
    FailedDependencyForCompleteAttempt,
    ExpectedPureAttempt,
    ExpectedActionAttempt,
    PureComputationMismatch,
}

impl InvalidDependencyReason {
    const fn description(self) -> &'static str {
        match self {
            Self::SelfReference => "an attempt cannot depend on itself",
            Self::MissingAttempt => "the referenced attempt does not exist",
            Self::PendingAttempt => "the referenced attempt is still pending",
            Self::FailedDependencyForCompleteAttempt => {
                "a complete attempt cannot depend on a failed attempt"
            }
            Self::ExpectedPureAttempt => "a pure edge must reference a pure attempt",
            Self::ExpectedActionAttempt => "an action edge must reference an action attempt",
            Self::PureComputationMismatch => {
                "the pure edge key does not match the referenced attempt"
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum InvalidActionLifecycleReason {
    DeniedActionCompleted,
    DeniedActionHasExecutorReport,
    CompletedActionMissingExecutorReport,
    CompletedActionMissingImportedReport,
}

/// The decision a completed attempt's dependencies support. An action attempt
/// may also record `ActionCachingDisabled`, which the dependencies never imply
/// and the validator always accepts, so it never appears here.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ExpectedReuseDecision {
    Reusable,
    EffectfulDependency { attempt: DurableAttemptId },
    DependencyNotReusable { attempt: DurableAttemptId },
}

impl std::fmt::Display for ExpectedReuseDecision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reusable => formatter.write_str("Reusable"),
            Self::EffectfulDependency { attempt } => {
                write!(formatter, "NotReusable(EffectfulDependency({attempt}))")
            }
            Self::DependencyNotReusable { attempt } => {
                write!(formatter, "NotReusable(DependencyNotReusable({attempt}))")
            }
        }
    }
}

impl InvalidActionLifecycleReason {
    const fn description(self) -> &'static str {
        match self {
            Self::DeniedActionCompleted => "a denied action cannot complete",
            Self::DeniedActionHasExecutorReport => {
                "a denied action cannot retain a report because execution is forbidden"
            }
            Self::CompletedActionMissingExecutorReport => {
                "a completed action requires the executor's captured report"
            }
            Self::CompletedActionMissingImportedReport => {
                "a completed action requires an imported report"
            }
        }
    }
}

/// Persistence boundary for durable engine attempts and reusable pure results.
pub trait EngineStateStore: Send + Sync {
    fn versions(&self) -> super::EngineStateVersions;

    /// Create a new attempt in the `Pending` state.
    ///
    /// Writes take `&self`: an adapter serializes them itself, and requiring
    /// `&mut` would make the store an exclusive resource for the whole engine.
    ///
    /// # Errors
    /// Returns an adapter error or an identifier-exhaustion error.
    fn create_pending_attempt(
        &self,
        computation: DurableComputation,
    ) -> Result<DurableAttemptId, EngineStateError>;

    /// Atomically replace `Pending` with `Complete`, including final edges,
    /// result, provenance, and reuse decision.
    ///
    /// # Errors
    /// Returns an error when the attempt is missing, already terminal, has an
    /// invalid completion, or the adapter cannot commit.
    fn publish_complete(
        &self,
        attempt: DurableAttemptId,
        completion: CompletedAttempt,
    ) -> Result<(), EngineStateError>;

    /// Atomically replace `Pending` with `Failed`, including available edges,
    /// diagnostics, and provenance.
    ///
    /// # Errors
    /// Returns an error when the attempt is missing, already terminal, has an
    /// invalid failure record, or the adapter cannot commit.
    fn publish_failed(
        &self,
        attempt: DurableAttemptId,
        failure: StoppedAttempt,
    ) -> Result<(), EngineStateError>;

    /// Atomically replace `Pending` with `Cancelled`, including available edges,
    /// diagnostics, and provenance.
    ///
    /// Cancellation is a distinct terminal state, not a flavour of failure: the
    /// attempt was stopped before it could produce a result, and nothing about
    /// the computation itself is known to be wrong. A reader can tell "this
    /// cannot work" from "this did not get to run."
    ///
    /// # Errors
    /// Returns an error when the attempt is missing, already terminal, has an
    /// invalid cancellation record, or the adapter cannot commit.
    fn publish_cancelled(
        &self,
        attempt: DurableAttemptId,
        cancellation: StoppedAttempt,
    ) -> Result<(), EngineStateError>;

    /// # Errors
    /// Returns an adapter error when the record cannot be read.
    fn attempt(
        &self,
        attempt: DurableAttemptId,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError>;

    /// Return attempts for one pure computation in creation order.
    ///
    /// # Errors
    /// Returns an adapter error when history cannot be read.
    fn attempt_history(
        &self,
        computation: PureComputationKey,
    ) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError>;

    /// Return the newest completed attempt marked reusable for the exact key.
    /// Dependency revalidation remains the engine's responsibility.
    ///
    /// # Errors
    /// Returns an adapter error when the reusable index cannot be read.
    fn latest_completed_reusable_attempt(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError>;

    /// Return the newest completed attempt marked reusable for the exact action
    /// key (decision 0031). The action mirror of
    /// [`Self::latest_completed_reusable_attempt`].
    ///
    /// Dependency revalidation remains the engine's responsibility, and so does
    /// the admission test over the recorded execution. An adapter answers only
    /// which reusable attempt a key has.
    ///
    /// # Errors
    /// Returns an adapter error when the reusable index cannot be read.
    fn latest_completed_reusable_action_attempt(
        &self,
        computation: ActionComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError>;

    /// Explain why the latest completed attempt for `computation` is not
    /// reusable, as a chain over the recorded dependency graph. `Ok(None)` when
    /// there is no completed attempt for the key, or when that attempt is
    /// reusable (there is nothing to explain).
    ///
    /// The chain follows the single dependency the attempt's own
    /// [`DurableReuseReason`](super::DurableReuseReason) names, so the
    /// explanation matches the reuse decision the attempt was published with
    /// rather than re-deriving one.
    ///
    /// # Errors
    /// Returns an adapter error when the attempt, its dependencies, or the
    /// dependencies' attempts cannot be read.
    fn explain_invalidation(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<super::InvalidationExplanation>, EngineStateError>;

    /// Enumerate all unfinished attempts in creation order. After reopening a
    /// store following interruption, callers use this operation to identify
    /// stale `Pending` attempts; the adapter never resumes them implicitly.
    ///
    /// # Errors
    /// Returns an adapter error when pending attempts cannot be read.
    fn pending_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError>;
}
