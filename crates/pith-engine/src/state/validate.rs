//! Publication invariants for durable engine state.
//!
//! Every adapter enforces the same rules, so this is where they live. An
//! adapter that validated independently would let two stores disagree about
//! which graphs are representable, and the engine's mapping would then be
//! correct against one store and wrong against another.
//!
//! The rules are decision 0024's: only completed attempts enter the reusable
//! index, a complete attempt cannot depend on a failed one, a denied action
//! cannot complete or retain a report, capability-use edges match the executor
//! report, and a pure attempt's reuse decision follows from its dependencies.

use std::sync::Arc;

use pith_core::{ActionComputationKey, CapabilityRequirement};

use crate::ActionAuthorization;
use crate::graph::canonical_capabilities;

use super::{
    CompletedAttempt, DurableActionProvenance, DurableAttempt, DurableAttemptId,
    DurableAttemptState, DurableComputation, DurableDependency, DurableProvenance,
    DurableReuseDecision, DurableReuseReason, EngineStateError, ExpectedReuseDecision,
    InvalidActionLifecycleReason, InvalidDependencyReason, StoppedAttempt,
};

/// The terminal state an adapter is being asked to publish.
pub enum TerminalAttemptState {
    Complete(CompletedAttempt),
    Failed(StoppedAttempt),
    Cancelled(StoppedAttempt),
}

impl TerminalAttemptState {
    #[must_use]
    pub fn provenance(&self) -> &DurableProvenance {
        match self {
            Self::Complete(completion) => &completion.provenance,
            Self::Failed(stopped) | Self::Cancelled(stopped) => &stopped.provenance,
        }
    }

    #[must_use]
    pub fn dependencies(&self) -> &[DurableDependency] {
        match self {
            Self::Complete(completion) => &completion.dependencies,
            Self::Failed(stopped) | Self::Cancelled(stopped) => &stopped.dependencies,
        }
    }

    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self, Self::Complete(_))
    }

    #[must_use]
    pub fn into_attempt_state(self) -> DurableAttemptState {
        match self {
            Self::Complete(completion) => DurableAttemptState::Complete(completion),
            Self::Failed(stopped) => DurableAttemptState::Failed(stopped),
            Self::Cancelled(stopped) => DurableAttemptState::Cancelled(stopped),
        }
    }

    /// Whether this publication earns a place in the reusable index: a
    /// completed attempt whose recorded decision is `Reusable`. Which index it
    /// lands in follows from the computation, since decision 0031 gives action
    /// applications an index of their own.
    #[must_use]
    pub const fn is_reusable(&self) -> bool {
        matches!(
            self,
            Self::Complete(CompletedAttempt {
                reuse: DurableReuseDecision::Reusable,
                ..
            })
        )
    }
}

/// How an adapter resolves a referenced attempt during validation.
///
/// A dependency edge names an attempt the adapter already stores, so
/// validation has to read it back. Adapters differ in how — an in-memory map,
/// a `SELECT` inside the publishing transaction — so they supply the lookup.
pub trait AttemptLookup {
    /// # Errors
    /// Returns an adapter error when the record cannot be read.
    fn lookup(
        &self,
        attempt: DurableAttemptId,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError>;
}

/// Validate a publication against every durable-state invariant.
///
/// # Errors
/// Returns the specific [`EngineStateError`] for the first invariant the
/// publication violates, or an adapter error from `lookup`.
pub fn validate_publication(
    lookup: &impl AttemptLookup,
    attempt: DurableAttemptId,
    computation: &DurableComputation,
    terminal_state: &TerminalAttemptState,
) -> Result<(), EngineStateError> {
    validate_provenance_category(attempt, computation, terminal_state.provenance())?;
    validate_action_computation_digest(attempt, computation)?;
    validate_action_lifecycle(attempt, computation, terminal_state)?;

    let mut first_non_reusable_dependency = None;
    let mut required_capabilities = Vec::new();
    for dependency in terminal_state.dependencies() {
        let dependency_record = validate_dependency(lookup, attempt, terminal_state, dependency)?;
        if let Some(dependency_record) = &dependency_record
            && let DurableAttemptState::Complete(completion) = &dependency_record.state
        {
            required_capabilities.extend(completion.capabilities.iter().cloned());
        }
        if first_non_reusable_dependency.is_none()
            && let Some(dependency_record) = dependency_record
            && !matches!(
                &dependency_record.state,
                DurableAttemptState::Complete(CompletedAttempt {
                    reuse: DurableReuseDecision::Reusable,
                    ..
                })
            )
        {
            first_non_reusable_dependency = Some(dependency_record.id);
        }
    }
    validate_capability_dependencies(
        attempt,
        terminal_state.provenance(),
        terminal_state.dependencies(),
    )?;
    validate_capability_requirements(attempt, computation, terminal_state, &required_capabilities)?;
    validate_reuse_decision(
        attempt,
        computation,
        terminal_state,
        first_non_reusable_dependency,
    )?;
    Ok(())
}

fn validate_dependency(
    lookup: &impl AttemptLookup,
    attempt: DurableAttemptId,
    terminal_state: &TerminalAttemptState,
    dependency: &DurableDependency,
) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
    let dependency_attempt = match dependency {
        DurableDependency::Pure { attempt, .. } | DurableDependency::Action { attempt } => *attempt,
        DurableDependency::Blob { .. } | DurableDependency::CapabilityUse { .. } => {
            return Ok(None);
        }
    };
    let invalid = |reason| EngineStateError::InvalidDependency {
        attempt,
        dependency: dependency_attempt,
        reason,
    };
    if attempt == dependency_attempt {
        return Err(invalid(InvalidDependencyReason::SelfReference));
    }
    let Some(dependency_record) = lookup.lookup(dependency_attempt)? else {
        return Err(invalid(InvalidDependencyReason::MissingAttempt));
    };
    match &dependency_record.state {
        DurableAttemptState::Pending => {
            return Err(invalid(InvalidDependencyReason::PendingAttempt));
        }
        // A complete attempt cannot depend on one that produced no result.
        // Cancellation is as disqualifying as failure here: the dependency has
        // no value, whatever the reason.
        DurableAttemptState::Failed(_) | DurableAttemptState::Cancelled(_)
            if terminal_state.is_complete() =>
        {
            return Err(invalid(
                InvalidDependencyReason::FailedDependencyForCompleteAttempt,
            ));
        }
        DurableAttemptState::Complete(_)
        | DurableAttemptState::Failed(_)
        | DurableAttemptState::Cancelled(_) => {}
    }

    match dependency {
        DurableDependency::Pure { computation, .. } => match &dependency_record.computation {
            DurableComputation::Pure(actual) if computation == actual => {
                Ok(Some(dependency_record))
            }
            DurableComputation::Pure(_) => {
                Err(invalid(InvalidDependencyReason::PureComputationMismatch))
            }
            DurableComputation::Action { .. } => {
                Err(invalid(InvalidDependencyReason::ExpectedPureAttempt))
            }
        },
        DurableDependency::Action { .. } => match &dependency_record.computation {
            DurableComputation::Action { .. } => Ok(Some(dependency_record)),
            DurableComputation::Pure(_) => {
                Err(invalid(InvalidDependencyReason::ExpectedActionAttempt))
            }
        },
        DurableDependency::Blob { .. } | DurableDependency::CapabilityUse { .. } => Ok(None),
    }
}

/// An action computation's stored digest has to be the one its own retained
/// material produces. The record holds the key's preimage (decision 0033), so
/// the digest beside it is checkable rather than something an adapter asserts.
fn validate_action_computation_digest(
    attempt: DurableAttemptId,
    computation: &DurableComputation,
) -> Result<(), EngineStateError> {
    let DurableComputation::Action {
        computation_digest,
        request,
        plan,
        ..
    } = computation
    else {
        return Ok(());
    };
    let mismatch = EngineStateError::ActionComputationDigestMismatch { attempt };
    let inputs = request.decoded_inputs().map_err(|_| mismatch.clone())?;
    let derived = ActionComputationKey::from_parts(
        plan.rule().identity(),
        plan.rule().revision(),
        &request.interface,
        &inputs,
        plan.spec_digest(),
    );
    if derived.digest == *computation_digest {
        Ok(())
    } else {
        Err(mismatch)
    }
}

/// The capability requirements a completed attempt records. An action carries
/// what its own contract declares; a pure computation carries the union of what
/// its dependencies carry, which is the propagation the arena does over live
/// nodes and a hydrated node has no subgraph to redo (decision 0033).
fn validate_capability_requirements(
    attempt: DurableAttemptId,
    computation: &DurableComputation,
    terminal_state: &TerminalAttemptState,
    dependency_capabilities: &[CapabilityRequirement],
) -> Result<(), EngineStateError> {
    let TerminalAttemptState::Complete(completion) = terminal_state else {
        return Ok(());
    };
    let expected = match computation {
        DurableComputation::Action { plan, .. } => {
            canonical_capabilities(plan.spec().capabilities.iter())
        }
        DurableComputation::Pure(_) => canonical_capabilities(dependency_capabilities),
    };
    if expected == completion.capabilities {
        Ok(())
    } else {
        Err(EngineStateError::CapabilityRequirementsMismatch { attempt })
    }
}

fn validate_provenance_category(
    attempt: DurableAttemptId,
    computation: &DurableComputation,
    provenance: &DurableProvenance,
) -> Result<(), EngineStateError> {
    if matches!(
        (computation, provenance),
        (DurableComputation::Pure(_), DurableProvenance::Pure)
            | (
                DurableComputation::Action { .. },
                DurableProvenance::Action(_)
            )
    ) {
        Ok(())
    } else {
        Err(EngineStateError::ProvenanceCategoryMismatch { attempt })
    }
}

fn validate_action_lifecycle(
    attempt: DurableAttemptId,
    computation: &DurableComputation,
    terminal_state: &TerminalAttemptState,
) -> Result<(), EngineStateError> {
    let DurableComputation::Action { authorization, .. } = computation else {
        return Ok(());
    };
    let DurableProvenance::Action(action_provenance) = terminal_state.provenance() else {
        return Ok(());
    };
    let invalid = |reason| EngineStateError::InvalidActionLifecycle { attempt, reason };

    if terminal_state.is_complete() && matches!(authorization, ActionAuthorization::Denied { .. }) {
        return Err(invalid(InvalidActionLifecycleReason::DeniedActionCompleted));
    }
    if matches!(authorization, ActionAuthorization::Denied { .. })
        && !matches!(action_provenance, DurableActionProvenance::NotExecuted)
    {
        return Err(invalid(
            InvalidActionLifecycleReason::DeniedActionHasExecutorReport,
        ));
    }
    if terminal_state.is_complete() {
        match action_provenance {
            DurableActionProvenance::NotExecuted => {
                return Err(invalid(
                    InvalidActionLifecycleReason::CompletedActionMissingExecutorReport,
                ));
            }
            DurableActionProvenance::Captured { .. } => {
                return Err(invalid(
                    InvalidActionLifecycleReason::CompletedActionMissingImportedReport,
                ));
            }
            DurableActionProvenance::Imported { .. } => {}
        }
    }
    Ok(())
}

fn validate_capability_dependencies(
    attempt: DurableAttemptId,
    provenance: &DurableProvenance,
    dependencies: &[DurableDependency],
) -> Result<(), EngineStateError> {
    let DurableProvenance::Action(action_provenance) = provenance else {
        return Ok(());
    };
    let expected = canonical_capabilities(action_provenance.capabilities_used());
    let recorded = dependencies
        .iter()
        .filter_map(|dependency| match dependency {
            DurableDependency::CapabilityUse { capability } => Some(capability),
            DurableDependency::Pure { .. }
            | DurableDependency::Action { .. }
            | DurableDependency::Blob { .. } => None,
        });
    if expected.iter().eq(recorded) {
        Ok(())
    } else {
        Err(EngineStateError::CapabilityDependenciesMismatch { attempt })
    }
}

fn validate_reuse_decision(
    attempt: DurableAttemptId,
    computation: &DurableComputation,
    terminal_state: &TerminalAttemptState,
    first_non_reusable_dependency: Option<DurableAttemptId>,
) -> Result<(), EngineStateError> {
    let TerminalAttemptState::Complete(completion) = terminal_state else {
        return Ok(());
    };
    // Refusing to index a result is always sound, so an action attempt may
    // record `ActionCachingDisabled` whatever its dependencies say. The reason
    // describes the engine that published it, and is meaningless on a pure
    // attempt. Claiming reuse the dependencies do not support stays rejected.
    if matches!(
        (computation, &completion.reuse),
        (
            DurableComputation::Action { .. },
            DurableReuseDecision::NotReusable(DurableReuseReason::ActionCachingDisabled)
        )
    ) {
        return Ok(());
    }
    // An action edge no longer stops reuse on its own (decision 0033): the
    // consumer revalidates it by re-planning the recorded request, so the only
    // edge that blocks a completed attempt is one that is not itself reusable.
    let expected = match first_non_reusable_dependency {
        Some(attempt) => ExpectedReuseDecision::DependencyNotReusable { attempt },
        None => ExpectedReuseDecision::Reusable,
    };
    let matches_expected = match (&completion.reuse, expected) {
        (DurableReuseDecision::Reusable, ExpectedReuseDecision::Reusable) => true,
        (
            DurableReuseDecision::NotReusable(DurableReuseReason::DependencyNotReusable {
                attempt: actual,
            }),
            ExpectedReuseDecision::DependencyNotReusable { attempt: expected },
        ) => *actual == expected,
        _ => false,
    };
    if matches_expected {
        Ok(())
    } else {
        Err(EngineStateError::InvalidReuseDecision { attempt, expected })
    }
}
