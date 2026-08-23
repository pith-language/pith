//! Diagnostic constructors for the graph evaluator.
//!
//! Every engine diagnostic is built here so the code, its message shape, and
//! its [`EngineCode`] stay in one place. The pure evaluator and the action
//! pipeline both call these instead of constructing `Diag`s inline.

use pith_core::{ActionSpec, PlatformRequirement, Type, Value};
use pith_diag::{Diag, DiagnosticSink, EngineCode, PithResult, Span};
use pith_ids::ContentId;

use crate::ExecutionPlatform;

/// Wrap a single diagnostic in a sink. Most engine error paths emit exactly
/// one diagnostic; this is the boilerplate that turns a `Diag` into the
/// `PithResult` they return.
pub(super) fn one_diag(diag: Diag) -> DiagnosticSink {
    let mut sink = DiagnosticSink::new();
    sink.push(diag);
    sink
}

pub(super) fn cycle_diag(chain: &[&str], span: Span) -> DiagnosticSink {
    one_diag(Diag::engine(
        EngineCode::DependencyCycle,
        span,
        format!("dependency cycle: {}", chain.join(" -> ")),
    ))
}

/// An engine-internal invariant that should be unreachable by construction.
/// Enumerating them makes the set of "impossible" states inspectable and
/// compile-checked: adding or removing one is a visible event, and a typo in a
/// free-text message cannot silently change which invariant fired.
pub(super) enum InternalInvariant {
    /// The parent frame of a dependency request was missing from the stack.
    PureLostRequestingFrame,
    /// The parent computation of a dependency request was missing from the arena.
    PureLostParentComputation,
    /// A completed computation's node was missing from the arena.
    PureLostComputationNode,
    /// The metadata of a selected pure rule was missing.
    PureLostSelectedRuleMetadata,
    /// The step machine had no frame to step.
    PureLostRootFrame,
    /// A frame completed but the stack was empty.
    PureCompletedWithoutFrame,
    /// A capability-bearing dependency's computation was missing.
    PureLostCapabilityComputation,
    /// The body of a selected pure rule was missing.
    SelectedRuleHasNoBody,
    /// The metadata of a selected rule was missing.
    SelectedRuleHasNoMetadata,
    /// The body of a selected action rule was missing.
    SelectedActionRuleHasNoBody,
    /// The metadata of a selected action rule was missing.
    SelectedActionRuleHasNoMetadata,
    /// The body of a selected observation rule was missing.
    SelectedObservationRuleHasNoBody,
    /// The metadata of a selected observation rule was missing.
    SelectedObservationRuleHasNoMetadata,
    /// The computation node of an in-flight action was missing.
    ActionLostComputationNode,
    /// The action record on an in-flight action computation was missing.
    ActionLostActionRecord,
    /// The computation node of an observation attempt was missing.
    ObservationLostComputationNode,
    /// A completed observation node had no observation record.
    ObservationLostObservationRecord,
    /// A tree file entry materialized as a tree instead of a blob.
    TreeFileMaterializedAsTree,
    /// A durable publication was requested for an attempt that is not Complete.
    DurablePublicationForNonCompleteAttempt,
    /// A durable publication was requested for an attempt that is not Failed.
    DurablePublicationForNonFailedAttempt,
    /// A completed action's durable provenance needs an imported report.
    CompletedActionMissingImportedReport,
    /// A computation has no durable attempt id recorded for it.
    DurableAttemptMissingForComputation,
    /// A pure durable dependency edge targeted a non-pure computation.
    DurablePureEdgeTargetNotPure,
    /// An observation dependency edge targeted a non-observation computation.
    DurableObservationEdgeTargetNotObservation,
    /// A completed observation has no recorded observer revision.
    CompletedObservationMissingRevision,
    /// A recorded observation request could not be decoded.
    RecordedObservationRequestUndecodable(pith_core::CanonicalDecodeError),
    /// A recorded observation subject could not be decoded.
    RecordedObservationSubjectUndecodable(pith_core::CanonicalDecodeError),
    /// A recorded observation revision could not be decoded.
    RecordedObservationRevisionUndecodable(pith_core::CanonicalDecodeError),
    /// The engine-state adapter rejected a publication (adapter validation or
    /// commit failure). The store's validation is a safety net; reaching this
    /// means the engine's mapping fed it inconsistent data.
    EngineStateStoreError(crate::state::EngineStateError),
    /// An engine-state read failed. Decision 0024 treats adapter failure as an
    /// error, never a cache miss: a broken database must not silently degrade
    /// into "recompute everything".
    EngineStateReadFailed(crate::state::EngineStateError),
    /// The reusable index returned an attempt that is not `Complete`. Only
    /// completed attempts may enter the index (decision 0024).
    ReusableIndexEntryNotComplete,
    /// The reusable index returned an attempt belonging to another computation.
    ReusableIndexEntryKeyMismatch,
    /// The reusable index returned a completed attempt whose reuse decision is
    /// not `Reusable`.
    ReusableIndexEntryNotReusable,
    /// An action was started by an evaluation that carries no policy and no
    /// executor. The pure driver rejects the step that asks for one, so only a
    /// run reaches the action pipeline.
    ActionStartedOutsideARun,
    /// An action dependency edge resolved to an attempt that is not an action
    /// computation. The store rejects such publications, so an edge that
    /// reaches revalidation naming one has been corrupted since.
    DurableActionEdgeTargetNotAction,
    /// A recorded action request could not be decoded. Its inputs were
    /// validated when they entered the store, so this means the retained
    /// encoding is unreadable under the current semantic encoding version.
    RecordedActionRequestUndecodable(pith_core::CanonicalDecodeError),
    /// A completed attempt's recorded dependency resolved to an attempt that is
    /// not itself complete. The store rejects such publications.
    DurableDependencyAttemptNotComplete,
    /// A retained durable result could not be decoded. Its bytes were validated
    /// when they entered the store, so this means the retained encoding is
    /// unreadable under the current semantic encoding version.
    HydratedResultUndecodable(pith_core::CanonicalDecodeError),
    /// A hydrated result does not inhabit the requested interface's output
    /// type. The computation key covers the interface, so a decoded value of
    /// another type contradicts the key it was indexed under.
    HydratedResultTypeMismatch { expected: Type, actual: Type },
    /// The scheduler was asked for an evaluation chain it no longer holds.
    SchedulerLostChain,
    /// A completed chain named a fan-out group the scheduler no longer holds.
    SchedulerLostGroup,
    /// A fan-out group completed but the frame that opened it was gone.
    SchedulerLostFanOutFrame,
    /// A root chain completed but its result slot was gone.
    SchedulerLostRootSlot,
}

impl InternalInvariant {
    fn message(&self) -> String {
        match self {
            InternalInvariant::PureLostRequestingFrame => {
                "pure evaluator lost a requesting frame".to_string()
            }
            InternalInvariant::PureLostParentComputation => {
                "pure evaluator lost a parent computation".to_string()
            }
            InternalInvariant::PureLostComputationNode => {
                "pure evaluator lost a computation node".to_string()
            }
            InternalInvariant::PureLostSelectedRuleMetadata => {
                "pure evaluator lost selected rule metadata".to_string()
            }
            InternalInvariant::PureLostRootFrame => {
                "pure evaluator lost its root frame".to_string()
            }
            InternalInvariant::PureCompletedWithoutFrame => {
                "pure evaluator completed without a frame".to_string()
            }
            InternalInvariant::PureLostCapabilityComputation => {
                "pure evaluator lost a capability dependency computation".to_string()
            }
            InternalInvariant::SelectedRuleHasNoBody => {
                "selected rule has no executable body".to_string()
            }
            InternalInvariant::SelectedRuleHasNoMetadata => {
                "selected rule has no metadata".to_string()
            }
            InternalInvariant::SelectedActionRuleHasNoBody => {
                "selected action rule has no body".to_string()
            }
            InternalInvariant::SelectedActionRuleHasNoMetadata => {
                "selected action rule has no metadata".to_string()
            }
            InternalInvariant::SelectedObservationRuleHasNoBody => {
                "selected observation rule has no body".to_string()
            }
            InternalInvariant::SelectedObservationRuleHasNoMetadata => {
                "selected observation rule has no metadata".to_string()
            }
            InternalInvariant::ActionLostComputationNode => {
                "action evaluator lost its computation node".to_string()
            }
            InternalInvariant::ActionLostActionRecord => {
                "action evaluator lost its action record".to_string()
            }
            InternalInvariant::ObservationLostComputationNode => {
                "observation evaluator lost its computation node".to_string()
            }
            InternalInvariant::ObservationLostObservationRecord => {
                "completed observation has no observation record".to_string()
            }
            InternalInvariant::TreeFileMaterializedAsTree => {
                "tree file content materialized as a tree".to_string()
            }
            InternalInvariant::DurablePublicationForNonCompleteAttempt => {
                "durable publication requested for a non-complete attempt".to_string()
            }
            InternalInvariant::DurablePublicationForNonFailedAttempt => {
                "durable publication requested for a non-failed attempt".to_string()
            }
            InternalInvariant::CompletedActionMissingImportedReport => {
                "completed action missing the imported executor report".to_string()
            }
            InternalInvariant::DurableAttemptMissingForComputation => {
                "computation has no durable attempt recorded".to_string()
            }
            InternalInvariant::DurablePureEdgeTargetNotPure => {
                "a pure durable dependency edge targets a non-pure computation".to_string()
            }
            InternalInvariant::DurableObservationEdgeTargetNotObservation => {
                "an observation durable dependency edge targets a non-observation computation"
                    .to_string()
            }
            InternalInvariant::CompletedObservationMissingRevision => {
                "a completed observation has no recorded observer revision".to_string()
            }
            InternalInvariant::RecordedObservationRequestUndecodable(error) => {
                format!("a recorded observation request could not be decoded: {error}")
            }
            InternalInvariant::RecordedObservationSubjectUndecodable(error) => {
                format!("a recorded observation subject could not be decoded: {error}")
            }
            InternalInvariant::RecordedObservationRevisionUndecodable(error) => {
                format!("a recorded observation revision could not be decoded: {error}")
            }
            InternalInvariant::EngineStateStoreError(error) => {
                format!("engine-state adapter rejected a publication: {error}")
            }
            InternalInvariant::EngineStateReadFailed(error) => {
                format!("engine-state adapter read failed: {error}")
            }
            InternalInvariant::ReusableIndexEntryNotComplete => {
                "the reusable index references a non-complete attempt".to_string()
            }
            InternalInvariant::ReusableIndexEntryKeyMismatch => {
                "the reusable index returned an attempt for another computation".to_string()
            }
            InternalInvariant::ReusableIndexEntryNotReusable => {
                "the reusable index references an attempt that is not reusable".to_string()
            }
            InternalInvariant::ActionStartedOutsideARun => {
                "an action was started outside a run".to_string()
            }
            InternalInvariant::DurableActionEdgeTargetNotAction => {
                "an action durable dependency edge targets a non-action computation".to_string()
            }
            InternalInvariant::RecordedActionRequestUndecodable(error) => {
                format!("a recorded action request could not be decoded: {error}")
            }
            InternalInvariant::DurableDependencyAttemptNotComplete => {
                "a completed attempt depends on a non-complete attempt".to_string()
            }
            InternalInvariant::HydratedResultUndecodable(error) => {
                format!("a retained durable result could not be decoded: {error}")
            }
            InternalInvariant::HydratedResultTypeMismatch { expected, actual } => {
                format!("a hydrated result is {actual}, expected {expected}")
            }
            InternalInvariant::SchedulerLostChain => {
                "the scheduler lost an evaluation chain".to_string()
            }
            InternalInvariant::SchedulerLostGroup => {
                "the scheduler lost a fan-out group".to_string()
            }
            InternalInvariant::SchedulerLostFanOutFrame => {
                "the scheduler lost the frame that opened a fan-out group".to_string()
            }
            InternalInvariant::SchedulerLostRootSlot => {
                "the scheduler lost a root result slot".to_string()
            }
        }
    }
}

/// Build the diagnostic for a violated [`InternalInvariant`].
pub(super) fn internal_diag(invariant: InternalInvariant) -> DiagnosticSink {
    one_diag(Diag::engine(
        EngineCode::InternalInvariant,
        Span::none(),
        invariant.message(),
    ))
}

pub(super) fn effectful_in_pure_diag() -> DiagnosticSink {
    one_diag(Diag::engine(
        EngineCode::EffectfulStepInPure,
        Span::none(),
        "effectful step (NeedBlob/NeedAction/NeedObservation) in a pure-only evaluation; use Engine::run",
    ))
}

pub(super) fn observer_missing_diag(span: Span) -> DiagnosticSink {
    one_diag(Diag::engine(
        EngineCode::ObserverMissing,
        span,
        "observation requested but no observer is configured",
    ))
}

/// The diagnostic a cancelled run carries. Not a fault: it reports that the
/// caller stopped the work, which is why the attempts it stopped are recorded
/// as cancelled rather than failed.
pub(super) fn cancelled_diag() -> DiagnosticSink {
    one_diag(Diag::engine(
        EngineCode::RunCancelled,
        Span::none(),
        "the run was cancelled by its caller",
    ))
}

/// The diagnostic a run that passed its declared wall clock carries, checked
/// at a scheduling boundary. Stopped, not broken: nothing about the work in
/// flight is known to be wrong, which is why the attempts it stops are
/// recorded as cancelled rather than failed (decision 0059).
pub(super) fn wall_bound_diag() -> DiagnosticSink {
    one_diag(Diag::engine(
        EngineCode::RunBoundExceeded,
        Span::none(),
        "the run passed the wall-clock deadline its caller declared; the work in \
         flight was stopped, not broken, and a larger bound changes the answer",
    ))
}

/// The diagnostic a run that spent its declared step budget carries, raised
/// from inside the step machine where the budget ran out. Names the request
/// whose body was stepping, which is the runaway (decision 0059).
pub(super) fn step_budget_diag(budget: u64, label: &str) -> DiagnosticSink {
    one_diag(Diag::engine(
        EngineCode::RunBoundExceeded,
        Span::none(),
        format!(
            "the run spent its caller-declared step budget of {budget} while `{label}` was \
             stepping; the budget is generous or the body yields without bound"
        ),
    ))
}

/// Whether `diagnostics` carries the bound's code, so the driver records what
/// a bound stopped as cancelled while an ordinary failure stays failed
/// (decision 0059).
pub(super) fn is_bound_stop(diagnostics: &pith_diag::DiagnosticSink) -> bool {
    let code = pith_diag::StableCode::from(EngineCode::RunBoundExceeded);
    diagnostics.iter().any(|diag| diag.code == code)
}

pub(super) fn store_error_diag(error: pith_store::StoreError) -> DiagnosticSink {
    one_diag(Diag::engine(
        EngineCode::StoreError,
        Span::none(),
        format!("content store error: {error}"),
    ))
}

pub(super) fn content_unavailable_diag(id: ContentId) -> DiagnosticSink {
    one_diag(Diag::engine(
        EngineCode::ContentUnavailable,
        Span::none(),
        format!("content {id:?} is not available locally"),
    ))
}

/// Check a rule or action result against its declared output type.
pub(super) fn validate_action_result(
    value: &Value,
    declared_output: &Type,
    rule_label: &str,
    rule_span: Span,
) -> PithResult<()> {
    if !value.is_type(declared_output) {
        let actual = value.value_type();
        return Err(one_diag(Diag::engine(
            EngineCode::ResultTypeMismatch,
            rule_span,
            format!("action `{rule_label}` returned {actual}, expected {declared_output}"),
        )));
    }
    Ok(())
}

/// Check the executor-reported platform against the action's requirement.
pub(super) fn validate_execution_platform(
    spec: &ActionSpec,
    actual: &ExecutionPlatform,
) -> PithResult<()> {
    if actual.operating_system.is_empty() || actual.architecture.is_empty() {
        return Err(one_diag(Diag::engine(
            EngineCode::PlatformMismatch,
            Span::none(),
            "executor did not report a concrete execution platform",
        )));
    }

    match &spec.platform {
        PlatformRequirement::Exact {
            operating_system,
            architecture,
        } if operating_system != &actual.operating_system
            || architecture != &actual.architecture =>
        {
            Err(one_diag(Diag::engine(
                EngineCode::PlatformMismatch,
                Span::none(),
                format!(
                    "executor selected platform `{}-{}`, expected `{}-{}`",
                    actual.operating_system, actual.architecture, operating_system, architecture
                ),
            )))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every internal-invariant diagnostic must carry the InternalInvariant
    /// code and a non-empty message. The exhaustive match already forces a
    /// message arm per variant at compile time; this guards that the arm is
    /// not accidentally emptied and that the code routing stays correct.
    #[test]
    fn every_internal_invariant_carries_the_right_code_and_message() {
        let invariants = [
            InternalInvariant::PureLostRequestingFrame,
            InternalInvariant::PureLostParentComputation,
            InternalInvariant::PureLostComputationNode,
            InternalInvariant::PureLostSelectedRuleMetadata,
            InternalInvariant::PureLostRootFrame,
            InternalInvariant::PureCompletedWithoutFrame,
            InternalInvariant::PureLostCapabilityComputation,
            InternalInvariant::SelectedRuleHasNoBody,
            InternalInvariant::SelectedRuleHasNoMetadata,
            InternalInvariant::SelectedActionRuleHasNoBody,
            InternalInvariant::SelectedActionRuleHasNoMetadata,
            InternalInvariant::ActionLostComputationNode,
            InternalInvariant::ActionLostActionRecord,
            InternalInvariant::TreeFileMaterializedAsTree,
            InternalInvariant::DurablePublicationForNonCompleteAttempt,
            InternalInvariant::DurablePublicationForNonFailedAttempt,
            InternalInvariant::CompletedActionMissingImportedReport,
            InternalInvariant::DurableAttemptMissingForComputation,
            InternalInvariant::DurablePureEdgeTargetNotPure,
            InternalInvariant::EngineStateStoreError(crate::state::EngineStateError::Adapter {
                message: "fixture".into(),
            }),
            InternalInvariant::EngineStateReadFailed(crate::state::EngineStateError::Adapter {
                message: "fixture".into(),
            }),
            InternalInvariant::ReusableIndexEntryNotComplete,
            InternalInvariant::ReusableIndexEntryKeyMismatch,
            InternalInvariant::ReusableIndexEntryNotReusable,
            InternalInvariant::ActionStartedOutsideARun,
            InternalInvariant::DurableActionEdgeTargetNotAction,
            InternalInvariant::RecordedActionRequestUndecodable(
                pith_core::CanonicalDecodeError::Truncated,
            ),
            InternalInvariant::DurableDependencyAttemptNotComplete,
            InternalInvariant::HydratedResultUndecodable(
                pith_core::CanonicalDecodeError::Truncated,
            ),
            InternalInvariant::HydratedResultTypeMismatch {
                expected: Type::Int,
                actual: Type::Bool,
            },
            InternalInvariant::SchedulerLostChain,
            InternalInvariant::SchedulerLostGroup,
            InternalInvariant::SchedulerLostFanOutFrame,
            InternalInvariant::SchedulerLostRootSlot,
        ];
        for invariant in invariants {
            let sink = internal_diag(invariant);
            let diag = sink.into_inner().first().cloned();
            let diag = diag.expect("internal_diag always emits one diagnostic");
            assert_eq!(
                diag.code,
                EngineCode::InternalInvariant.into(),
                "internal invariant did not route to EngineCode::InternalInvariant"
            );
            assert!(
                !diag.message.0.is_empty(),
                "internal invariant produced an empty message"
            );
        }
    }
}
