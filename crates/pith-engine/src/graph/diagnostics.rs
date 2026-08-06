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
    /// A pure step requested a blob/action but the evaluation stack was empty.
    EffectfulStepWithNoFrame(&'static str),
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
    /// The computation node of an in-flight action was missing.
    ActionLostComputationNode,
    /// The action record on an in-flight action computation was missing.
    ActionLostActionRecord,
    /// A tree file entry materialized as a tree instead of a blob.
    TreeFileMaterializedAsTree,
}

impl InternalInvariant {
    fn message(&self) -> String {
        match self {
            InternalInvariant::EffectfulStepWithNoFrame(step) => {
                format!("{step} requested with no frame on the stack")
            }
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
            InternalInvariant::ActionLostComputationNode => {
                "action evaluator lost its computation node".to_string()
            }
            InternalInvariant::ActionLostActionRecord => {
                "action evaluator lost its action record".to_string()
            }
            InternalInvariant::TreeFileMaterializedAsTree => {
                "tree file content materialized as a tree".to_string()
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
        "effectful step (NeedBlob/NeedAction) in a pure-only evaluation; use Engine::run",
    ))
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
            InternalInvariant::EffectfulStepWithNoFrame("blob"),
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
