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

/// An engine-internal invariant was violated. These should be unreachable by
/// construction; the message identifies which invariant.
pub(super) fn internal_diag(message: &str) -> DiagnosticSink {
    one_diag(Diag::engine(
        EngineCode::InternalInvariant,
        Span::none(),
        message,
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

pub(super) fn wrong_output_kind_diag(path: &str) -> DiagnosticSink {
    one_diag(Diag::engine(
        EngineCode::UndeclaredOutput,
        Span::none(),
        format!("executor reported output `{path}` with the wrong kind"),
    ))
}

/// Check a rule or action result against its declared output type.
pub(super) fn validate_action_result(
    value: &Value,
    declared_output: &Type,
    rule_label: &str,
    rule_span: Span,
) -> PithResult<()> {
    let actual = value.value_type();
    if actual != *declared_output {
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
