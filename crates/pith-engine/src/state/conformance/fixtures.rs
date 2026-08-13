//! The rules, contracts, and reports a generated step is built from.

use pith_core::{
    ActionComputationKey, ActionSpec, CapabilityRequirement, Content, Interface,
    PureComputationKey, RuleIdentity, RuleRevision, Type, Value,
};
use pith_diag::{Severity, Span, StableCode};
use pith_ids::{ContentDigest, ContentId, DIGEST_LEN, PureComputationDigest};

use crate::AccessVerification;
use crate::action::{ExecutionPlatform, ExecutionReport, ProducedOutput};
use crate::policy::ActionAuthorization;
use crate::state::{
    DurableActionPlan, DurableActionRequest, DurableComputation, DurableDiagnostic,
    DurableDiagnosticNote, DurableRule, EncodedValue,
};

pub(super) fn diagnostics(message_len: u8, notes: u8) -> Box<[DurableDiagnostic]> {
    if message_len == 0 {
        return Box::new([]);
    }
    let notes = (0..notes)
        .map(|note| DurableDiagnosticNote {
            span: Span::none(),
            message: format!("note {note}").into(),
        })
        .collect();
    Box::new([DurableDiagnostic {
        severity: Severity::Error,
        code: StableCode(1207),
        span: Span::none(),
        message: std::iter::repeat_n('d', usize::from(message_len))
            .collect::<String>()
            .into(),
        notes,
    }])
}

pub(super) fn pure_key(rule: u8, input: u8) -> PureComputationKey {
    let identity = RuleIdentity::of_module_declaration("pith-conformance", &format!("rule-{rule}"));
    PureComputationKey {
        rule_identity: identity,
        rule_revision: RuleRevision::of_manifest(identity, &[rule]),
        digest: PureComputationDigest::from_digest(ContentDigest::from_bytes([input; DIGEST_LEN])),
    }
}

pub(super) fn action_computation(
    rule: u8,
    executable: u8,
    capabilities: &[u8],
    denied: bool,
) -> Option<DurableComputation> {
    let identity =
        RuleIdentity::of_module_declaration("pith-conformance", &format!("action-{rule}"));
    let mut spec = ActionSpec::isolated(host_path(executable));
    spec.capabilities = capabilities.iter().copied().map(capability).collect();
    let revision = RuleRevision::of_manifest(identity, &[rule]);
    let plan = DurableActionPlan::new(DurableRule::new(revision), spec).ok()?;
    // The digest is derived from the retained request the way a real one is,
    // because the publication validator rederives it (decision 0033). Distinct
    // generated actions differ in the rule, the contract, or the input, so
    // their keys differ without anything here arranging it.
    let interface = Interface {
        inputs: Box::new([Type::Int]),
        output: Type::Blob,
    };
    let inputs = [Value::Int(i64::from(executable))];
    let computation_digest = ActionComputationKey::from_parts(
        identity,
        revision,
        &interface,
        &inputs,
        plan.spec_digest(),
    )
    .digest;
    let request = DurableActionRequest {
        interface,
        inputs: inputs.iter().map(EncodedValue::from_value).collect(),
    };
    let authorization = if denied {
        ActionAuthorization::Denied {
            policy: "conformance".into(),
            reason: "generated denial".into(),
        }
    } else {
        ActionAuthorization::Allowed {
            policy: "conformance".into(),
        }
    };
    Some(DurableComputation::Action {
        computation_digest,
        request,
        plan: Box::new(plan),
        authorization,
    })
}

/// The capability list mirrors the contract's, which is what an executor that
/// used its granted authority reports.
pub(super) fn imported_report(computation: &DurableComputation) -> ExecutionReport {
    let capabilities_used = match computation {
        DurableComputation::Action { plan, .. } => plan.spec().capabilities.clone(),
        DurableComputation::Pure(_) => Box::new([]),
    };
    ExecutionReport {
        executor: "conformance".into(),
        platform: ExecutionPlatform {
            operating_system: "linux".into(),
            architecture: "x86_64".into(),
        },
        access: AccessVerification::Prevented,
        outputs: Box::new([ProducedOutput {
            path: "out".into(),
            content: Content::Blob(content_id(7)),
        }]),
        capabilities_used,
    }
}

pub(super) fn capability(seed: u8) -> CapabilityRequirement {
    CapabilityRequirement {
        name: format!("capability-{seed}").into(),
        scope: format!("scope-{seed}").into(),
    }
}

pub(super) fn content_id(seed: u8) -> ContentId {
    ContentId::from_digest(ContentDigest::from_bytes([seed; DIGEST_LEN]))
}

/// A host path for an action executable, varied by `seed` so distinct conformance
/// actions get distinct identities (decision 0030: `executable` is a host path).
pub(super) fn host_path(seed: u8) -> &'static str {
    const PATHS: &[&str] = &[
        "/bin/a", "/bin/b", "/bin/c", "/bin/d", "/bin/e", "/bin/f", "/bin/g", "/bin/h",
    ];
    PATHS
        .get(seed as usize)
        .copied()
        .unwrap_or(PATHS.first().copied().unwrap_or("/bin/a"))
}
