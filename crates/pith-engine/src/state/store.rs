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
    /// Recorded capability requirements disagree with the dependencies.
    CapabilityRequirementsMismatch {
        attempt: DurableAttemptId,
    },
    /// The retained action inputs do not reproduce the stored digest.
    ActionComputationDigestMismatch {
        attempt: DurableAttemptId,
    },
    /// The observation computation's stored digest is not the one its
    /// retained request, rule, and subject produce.
    ObservationComputationDigestMismatch {
        attempt: DurableAttemptId,
    },
    ObservationObserverMismatch {
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
            Self::CapabilityRequirementsMismatch { attempt } => write!(
                formatter,
                "engine-state attempt {attempt} records capability requirements its dependencies do not support"
            ),
            Self::ActionComputationDigestMismatch { attempt } => write!(
                formatter,
                "engine-state action attempt {attempt} has a computation digest its retained request does not produce"
            ),
            Self::ObservationComputationDigestMismatch { attempt } => write!(
                formatter,
                "engine-state observation attempt {attempt} has a computation digest its retained request does not produce"
            ),
            Self::ObservationObserverMismatch { attempt } => write!(
                formatter,
                "engine-state observation attempt {attempt} has provenance from a different observer"
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
    ExpectedObservationAttempt,
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
            Self::ExpectedObservationAttempt => {
                "an observation edge must reference an observation attempt"
            }
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

/// Record and reusable-index counts obtained without decoding payloads.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct AttemptStatistics {
    pub attempts: u64,
    pub pending: u64,
    pub complete: u64,
    pub failed: u64,
    pub cancelled: u64,
    /// Computation keys that currently name a completed reusable attempt.
    pub reusable_index: u64,
}

impl AttemptStatistics {
    /// Fold one attempt's status into the counts. Adapters share this so a
    /// status added upstream is counted by both or fails to compile in both.
    pub fn record(&mut self, status: DurableAttemptStatus) {
        self.attempts = self.attempts.saturating_add(1);
        match status {
            DurableAttemptStatus::Pending => self.pending = self.pending.saturating_add(1),
            DurableAttemptStatus::Complete => self.complete = self.complete.saturating_add(1),
            DurableAttemptStatus::Failed => self.failed = self.failed.saturating_add(1),
            DurableAttemptStatus::Cancelled => self.cancelled = self.cancelled.saturating_add(1),
        }
    }
}

/// Read authority over durable engine state. Read-only adapters implement this
/// trait without exposing the methods on [`EngineStateStore`].
pub trait EngineStateReader: Send + Sync {
    fn versions(&self) -> super::EngineStateVersions;

    /// Count the records this store holds, reading no payloads.
    ///
    /// # Errors
    /// Returns an adapter error when the counts cannot be read.
    fn attempt_statistics(&self) -> Result<AttemptStatistics, EngineStateError>;

    /// Every attempt in creation order, each read through the same decode
    /// validation an individual lookup applies. This is the integrity walk:
    /// a store that answers it has decoded every record it holds.
    ///
    /// # Errors
    /// Returns an adapter error when any record cannot be read or decoded.
    fn all_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError>;

    /// Attempts named by the reusable index, in creation order.
    ///
    /// # Errors
    /// Returns an adapter error when the index or the attempts it names
    /// cannot be read.
    fn reusable_index_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError>;

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

    /// Return the newest attempt for one pure computation, whatever its
    /// terminal state, without reading the history that precedes it.
    ///
    /// The default answers from [`Self::attempt_history`], which reads every
    /// record under the key; an adapter whose history is stored out of line
    /// overrides this with an indexed read instead.
    ///
    /// # Errors
    /// Returns an adapter error when the attempt cannot be read.
    fn latest_attempt(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        Ok(self.attempt_history(computation)?.into_iter().last())
    }

    /// Return the newest completed attempt marked reusable for the exact key.
    /// Dependency revalidation remains the engine's responsibility.
    ///
    /// # Errors
    /// Returns an adapter error when the reusable index cannot be read.
    fn latest_completed_reusable_attempt(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError>;

    /// Return the newest completed reusable attempt for an exact action key.
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

/// Persistence boundary for durable engine attempts and reusable pure results.
///
/// The writing half: an engine evaluating a graph holds one of these, and a
/// read-only reader holds only the [`EngineStateReader`] half. Reads arrive
/// through the supertrait, so read-write implies read-only as a lattice and
/// not as a convention.
pub trait EngineStateStore: EngineStateReader {
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
}
