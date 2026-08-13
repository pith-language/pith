//! Rewriting an adapter's records and errors into the model's identifier space.
//!
//! Attempt identifiers are store-local, so two conforming adapters may allocate
//! them in different orders. Everything that names an attempt is translated
//! before it is compared.

use std::collections::BTreeMap;

use crate::state::{
    CompletedAttempt, DurableAttempt, DurableAttemptId, DurableAttemptState, DurableDependency,
    DurableReuseDecision, DurableReuseReason, EngineStateError, ExpectedReuseDecision,
    InvalidationExplanation, InvalidationReason, StoppedAttempt,
};

use super::run::Tracked;

pub(super) type Translation = BTreeMap<DurableAttemptId, DurableAttemptId>;

pub(super) fn subject_to_model(tracked: &[Tracked]) -> Translation {
    tracked
        .iter()
        .map(|entry| (entry.subject, entry.model))
        .collect()
}

pub(super) fn translate(attempt: DurableAttemptId, translation: &Translation) -> DurableAttemptId {
    translation.get(&attempt).copied().unwrap_or(attempt)
}

/// Rewrite a record into the model's identifier space so the two can be
/// compared directly.
pub(super) fn translate_attempt(
    attempt: &DurableAttempt,
    translation: &Translation,
) -> DurableAttempt {
    DurableAttempt {
        id: translate(attempt.id, translation),
        computation: attempt.computation.clone(),
        state: match &attempt.state {
            DurableAttemptState::Pending => DurableAttemptState::Pending,
            DurableAttemptState::Complete(completion) => {
                DurableAttemptState::Complete(CompletedAttempt {
                    dependencies: translate_dependencies(&completion.dependencies, translation),
                    result: completion.result.clone(),
                    provenance: completion.provenance.clone(),
                    reuse: match &completion.reuse {
                        DurableReuseDecision::Reusable => DurableReuseDecision::Reusable,
                        DurableReuseDecision::NotReusable(reason) => {
                            DurableReuseDecision::NotReusable(translate_reuse_reason(
                                reason,
                                translation,
                            ))
                        }
                    },
                    capabilities: completion.capabilities.clone(),
                })
            }
            DurableAttemptState::Failed(stopped) => {
                DurableAttemptState::Failed(translate_stopped(stopped, translation))
            }
            DurableAttemptState::Cancelled(stopped) => {
                DurableAttemptState::Cancelled(translate_stopped(stopped, translation))
            }
        },
    }
}

fn translate_stopped(stopped: &StoppedAttempt, translation: &Translation) -> StoppedAttempt {
    StoppedAttempt {
        dependencies: translate_dependencies(&stopped.dependencies, translation),
        diagnostics: stopped.diagnostics.clone(),
        provenance: stopped.provenance.clone(),
    }
}

pub(super) fn translate_dependencies(
    dependencies: &[DurableDependency],
    translation: &Translation,
) -> Box<[DurableDependency]> {
    dependencies
        .iter()
        .map(|dependency| match dependency {
            DurableDependency::Pure {
                computation,
                attempt,
            } => DurableDependency::Pure {
                computation: *computation,
                attempt: translate(*attempt, translation),
            },
            DurableDependency::Action { attempt } => DurableDependency::Action {
                attempt: translate(*attempt, translation),
            },
            blob_or_capability @ (DurableDependency::Blob { .. }
            | DurableDependency::CapabilityUse { .. }) => blob_or_capability.clone(),
        })
        .collect()
}

pub(super) fn translate_reuse_reason(
    reason: &DurableReuseReason,
    translation: &Translation,
) -> DurableReuseReason {
    match reason {
        DurableReuseReason::EffectfulDependency { attempt } => {
            DurableReuseReason::EffectfulDependency {
                attempt: translate(*attempt, translation),
            }
        }
        DurableReuseReason::DependencyPending { attempt } => {
            DurableReuseReason::DependencyPending {
                attempt: translate(*attempt, translation),
            }
        }
        DurableReuseReason::DependencyNotReusable { attempt } => {
            DurableReuseReason::DependencyNotReusable {
                attempt: translate(*attempt, translation),
            }
        }
        without_attempt @ (DurableReuseReason::ActionCachingDisabled
        | DurableReuseReason::DependencyMissing { .. }) => without_attempt.clone(),
    }
}

/// Errors name attempts by the identifiers of the store that produced them, so
/// they are translated before comparison for the same reason records are.
pub(super) fn translate_error(
    error: &EngineStateError,
    translation: &Translation,
) -> EngineStateError {
    match error {
        EngineStateError::AttemptNotFound { attempt } => EngineStateError::AttemptNotFound {
            attempt: translate(*attempt, translation),
        },
        EngineStateError::AttemptNotPending { attempt, status } => {
            EngineStateError::AttemptNotPending {
                attempt: translate(*attempt, translation),
                status: *status,
            }
        }
        EngineStateError::InvalidDependency {
            attempt,
            dependency,
            reason,
        } => EngineStateError::InvalidDependency {
            attempt: translate(*attempt, translation),
            dependency: translate(*dependency, translation),
            reason: *reason,
        },
        EngineStateError::InvalidActionLifecycle { attempt, reason } => {
            EngineStateError::InvalidActionLifecycle {
                attempt: translate(*attempt, translation),
                reason: *reason,
            }
        }
        EngineStateError::InvalidReuseDecision { attempt, expected } => {
            EngineStateError::InvalidReuseDecision {
                attempt: translate(*attempt, translation),
                expected: match expected {
                    ExpectedReuseDecision::DependencyNotReusable { attempt } => {
                        ExpectedReuseDecision::DependencyNotReusable {
                            attempt: translate(*attempt, translation),
                        }
                    }
                    without_attempt => *without_attempt,
                },
            }
        }
        EngineStateError::CapabilityDependenciesMismatch { attempt } => {
            EngineStateError::CapabilityDependenciesMismatch {
                attempt: translate(*attempt, translation),
            }
        }
        EngineStateError::CapabilityRequirementsMismatch { attempt } => {
            EngineStateError::CapabilityRequirementsMismatch {
                attempt: translate(*attempt, translation),
            }
        }
        EngineStateError::ActionComputationDigestMismatch { attempt } => {
            EngineStateError::ActionComputationDigestMismatch {
                attempt: translate(*attempt, translation),
            }
        }
        EngineStateError::ProvenanceCategoryMismatch { attempt } => {
            EngineStateError::ProvenanceCategoryMismatch {
                attempt: translate(*attempt, translation),
            }
        }
        without_attempt @ (EngineStateError::AttemptIdentifierExhausted
        | EngineStateError::Adapter { .. }) => without_attempt.clone(),
    }
}

/// Rewrite an invalidation explanation into the model's identifier space. The
/// explanation carries attempt ids at the root, inside each dependency edge,
/// and recursively in the child; all three are translated so a subject and the
/// model can be compared directly.
pub(super) fn translate_explanation(
    explanation: &InvalidationExplanation,
    translation: &Translation,
) -> InvalidationExplanation {
    InvalidationExplanation {
        attempt: translate(explanation.attempt, translation),
        computation: explanation.computation.clone(),
        reason: translate_reason(&explanation.reason, translation),
    }
}

fn translate_reason(reason: &InvalidationReason, translation: &Translation) -> InvalidationReason {
    match reason {
        InvalidationReason::Leaf(leaf) => {
            InvalidationReason::Leaf(translate_reuse_reason(leaf, translation))
        }
        InvalidationReason::DependencyInvalidated { edge, child } => {
            InvalidationReason::DependencyInvalidated {
                edge: translate_single_edge(edge, translation),
                child: Box::new(translate_explanation(child, translation)),
            }
        }
    }
}

/// Translate a single dependency edge — the one-element case of
/// [`translate_dependencies`], which the explanation carries one edge at a
/// time. Mirrors that function's per-variant rewrite so the two cannot drift.
fn translate_single_edge(edge: &DurableDependency, translation: &Translation) -> DurableDependency {
    match edge {
        DurableDependency::Pure {
            computation,
            attempt,
        } => DurableDependency::Pure {
            computation: *computation,
            attempt: translate(*attempt, translation),
        },
        DurableDependency::Action { attempt } => DurableDependency::Action {
            attempt: translate(*attempt, translation),
        },
        blob_or_capability @ (DurableDependency::Blob { .. }
        | DurableDependency::CapabilityUse { .. }) => blob_or_capability.clone(),
    }
}
