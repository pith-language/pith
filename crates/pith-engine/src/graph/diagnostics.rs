//! Diagnostic constructors for the graph evaluator, so every engine
//! diagnostic has one code, message shape, and [`EngineCode`] home.

use pith_core::{ActionSpec, EffectKind, PlatformRequirement, Type, Value};
use pith_diag::{Diag, DiagnosticSink, EngineCode, PithResult, Span};
use pith_ids::ContentId;

use crate::ExecutionPlatform;

/// Wrap a single diagnostic in a sink; most engine error paths emit exactly one.
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

/// An engine-internal invariant that should be unreachable. Enumerating them
/// as a type keeps the set compile-checked and ties each diagnostic to a
/// distinct variant.
pub(super) enum InternalInvariant {
    PureLostRequestingFrame,
    PureLostParentComputation,
    PureLostComputationNode,
    PureLostSelectedRuleMetadata,
    PureLostRootFrame,
    PureCompletedWithoutFrame,
    PureLostCapabilityComputation,
    SelectedRuleHasNoBody,
    SelectedRuleHasNoMetadata,
    SelectedActionRuleHasNoBody,
    SelectedActionRuleHasNoMetadata,
    SelectedObservationRuleHasNoBody,
    SelectedObservationRuleHasNoMetadata,
    ActionLostComputationNode,
    ActionLostActionRecord,
    ObservationLostComputationNode,
    ObservationLostObservationRecord,
    TreeFileMaterializedAsTree,
    DurablePublicationForNonCompleteAttempt,
    DurablePublicationForNonFailedAttempt,
    CompletedActionMissingImportedReport,
    DurableAttemptMissingForComputation,
    DurablePureEdgeTargetNotPure,
    DurableObservationEdgeTargetNotObservation,
    CompletedObservationMissingRevision,
    RecordedObservationRequestUndecodable(pith_core::CanonicalDecodeError),
    RecordedObservationSubjectUndecodable(pith_core::CanonicalDecodeError),
    RecordedObservationRevisionUndecodable(pith_core::CanonicalDecodeError),
    /// The engine-state adapter rejected a publication. Store validation is a
    /// safety net, so reaching this means the engine's mapping produced
    /// inconsistent data.
    EngineStateStoreError(crate::state::EngineStateError),
    /// An adapter read failure is an error, never a cache miss: a broken
    /// database must not silently degrade into recomputing everything.
    EngineStateReadFailed(crate::state::EngineStateError),
    /// Only completed attempts may enter the reusable index.
    ReusableIndexEntryNotComplete,
    ReusableIndexEntryKeyMismatch,
    ReusableIndexEntryNotReusable,
    /// Only a run reaches the action pipeline; the pure driver rejects the
    /// step that would ask for one.
    ActionStartedOutsideARun,
    /// The store rejects such publications, so an edge like this reaching
    /// revalidation means corruption after the fact.
    DurableActionEdgeTargetNotAction,
    /// Inputs were validated when they entered the store, so this means the
    /// retained encoding is unreadable under the current semantic encoding
    /// version.
    RecordedActionRequestUndecodable(pith_core::CanonicalDecodeError),
    /// The store rejects such publications, so this means corruption after
    /// the fact.
    DurableDependencyAttemptNotComplete,
    /// Retained bytes were validated when they entered the store, so this
    /// means the encoding is unreadable under the current semantic encoding
    /// version.
    HydratedResultUndecodable(pith_core::CanonicalDecodeError),
    /// The computation key covers the interface, so a value of another type
    /// contradicts the key the result was indexed under.
    HydratedResultTypeMismatch {
        expected: Type,
        actual: Type,
    },
    SchedulerLostChain,
    SchedulerLostGroup,
    SchedulerLostFanOutFrame,
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

/// The diagnostic a pure-only evaluation raises on an effectful step. The
/// listing derives its category half from the kinds, so a new kind joins it
/// without a second edit here.
pub(super) fn effectful_in_pure_diag() -> DiagnosticSink {
    let refused: Vec<&str> = EffectKind::ALL
        .iter()
        .filter(|kind| !kind.is_pure())
        .map(|kind| kind.step_name())
        .collect();
    one_diag(Diag::engine(
        EngineCode::EffectfulStepInPure,
        Span::none(),
        format!(
            "effectful step (NeedBlob/{}) in a pure-only evaluation; use Engine::run",
            refused.join("/")
        ),
    ))
}

pub(super) fn observer_missing_diag(span: Span) -> DiagnosticSink {
    one_diag(Diag::engine(
        EngineCode::ObserverMissing,
        span,
        "observation requested but no observer is configured",
    ))
}

/// The diagnostic a cancelled run carries: the caller stopped the work, which
/// is why the attempts it stopped record as cancelled rather than failed.
pub(super) fn cancelled_diag() -> DiagnosticSink {
    one_diag(Diag::engine(
        EngineCode::RunCancelled,
        Span::none(),
        "the run was cancelled by its caller",
    ))
}

/// The diagnostic a run carries after passing its declared wall clock, checked
/// at a scheduling boundary. The stopped work is not known to be wrong, so its
/// attempts record as cancelled rather than failed.
pub(super) fn wall_bound_diag() -> DiagnosticSink {
    one_diag(Diag::engine(
        EngineCode::RunBoundExceeded,
        Span::none(),
        "the run passed the wall-clock deadline its caller declared; the work in \
         flight was stopped, not broken, and a larger bound changes the answer",
    ))
}

/// The diagnostic a run carries after spending its declared step budget,
/// raised inside the step machine where the budget ran out. Names the request
/// that was stepping.
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
/// a bound stopped as cancelled while an ordinary failure stays failed.
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
    /// code and a non-empty message.
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

    /// The refused-step listing is derived from the kinds rather
    /// than spelled out here. The test pins its spelling, which is the text
    /// a caller reads when a pure-only evaluation hits an effect.
    #[test]
    fn effectful_step_listing_names_every_refused_step() {
        let diagnostics = effectful_in_pure_diag();
        let diag = diagnostics
            .into_inner()
            .first()
            .cloned()
            .expect("effectful_in_pure_diag always emits one diagnostic");
        assert_eq!(diag.code, EngineCode::EffectfulStepInPure.into());
        assert_eq!(
            &*diag.message.0,
            "effectful step (NeedBlob/NeedAction/NeedObservation) in a pure-only evaluation; use Engine::run",
        );
    }
}
