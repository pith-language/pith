use std::sync::Arc;

use pith_core::PureComputationKey;

use super::{
    CompletedAttempt, DurableAttempt, DurableAttemptId, DurableAttemptStatus, DurableComputation,
    FailedAttempt,
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
    ActionMarkedReusable,
    DeniedActionCompleted,
    DeniedActionHasExecutorReport,
    CompletedActionMissingExecutorReport,
    CompletedActionMissingImportedReport,
}

impl InvalidActionLifecycleReason {
    const fn description(self) -> &'static str {
        match self {
            Self::ActionMarkedReusable => "action caching is disabled",
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
    /// # Errors
    /// Returns an adapter error or an identifier-exhaustion error.
    fn create_pending_attempt(
        &mut self,
        computation: DurableComputation,
    ) -> Result<DurableAttemptId, EngineStateError>;

    /// Atomically replace `Pending` with `Complete`, including final edges,
    /// result, provenance, and reuse decision.
    ///
    /// # Errors
    /// Returns an error when the attempt is missing, already terminal, has an
    /// invalid completion, or the adapter cannot commit.
    fn publish_complete(
        &mut self,
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
        &mut self,
        attempt: DurableAttemptId,
        failure: FailedAttempt,
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

    /// Enumerate all unfinished attempts in creation order. After reopening a
    /// store following interruption, callers use this operation to identify
    /// stale `Pending` attempts; the adapter never resumes them implicitly.
    ///
    /// # Errors
    /// Returns an adapter error when pending attempts cannot be read.
    fn pending_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError>;
}
