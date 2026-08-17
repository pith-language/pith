//! Durable engine-state records.
//!
//! These types use stable digests and store-local attempt identifiers. Arena
//! handles never cross this boundary.

use pith_core::{
    ActionComputationKey, ActionSpec, CanonicalDecodeError, CapabilityRequirement, Interface,
    OutputKind, PureComputationKey, Value,
};
use pith_diag::{Diag, Severity, Span, StableCode};
use pith_ids::{ActionComputationDigest, ActionSpecDigest, ContentId, RuleIdentity, RuleRevision};

use crate::{
    AccessVerification, ActionAuthorization, CapturedExecutionReport, ExecutionPlatform,
    ExecutionReport,
};

/// Version of the canonical payload encodings durable records embed: a typed
/// [`Value`] result and an [`ActionSpec`] contract. Reported as the
/// `semantic_encoding` half of an adapter's [`EngineStateVersions`]. Adapters
/// choose their own table layout and report it as `schema` (decision 0025).
///
/// Pinned at 1 until the first release (decision 0048), on the same terms the
/// sqlite adapter pins its schema: no database outside a working tree needs to
/// survive a change, so a pre-release database is moved aside and rebuilt
/// rather than versioned around, and a stale one is deleted. It reached 5 and
/// was returned to 1 before 0048 made the rule general, which is why 0026's
/// account of four bumps describes history rather than the tree.
///
/// The first tag is the trigger. After it, this starts moving for any change a
/// prior build would misread — a new constructor in the value grammar as much
/// as a changed meaning of existing bytes — and engine state stays in 0048's
/// derived-cache class, where discard-and-rebuild remains a legal answer.
pub const RECORD_ENCODING_VERSION: SemanticEncodingVersion = SemanticEncodingVersion::new(1);

pub const CURRENT_ENGINE_STATE_VERSIONS: EngineStateVersions = EngineStateVersions {
    schema: SchemaVersion::new(1),
    semantic_encoding: RECORD_ENCODING_VERSION,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SchemaVersion(u32);

impl SchemaVersion {
    pub const fn new(version: u32) -> Self {
        Self(version)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SemanticEncodingVersion(u32);

impl SemanticEncodingVersion {
    pub const fn new(version: u32) -> Self {
        Self(version)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct EngineStateVersions {
    pub schema: SchemaVersion,
    pub semantic_encoding: SemanticEncodingVersion,
}

/// Stable rule identity and the revision of its executable semantics.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DurableRule {
    revision: RuleRevision,
}

impl DurableRule {
    pub fn new(revision: RuleRevision) -> Self {
        Self { revision }
    }

    pub fn identity(self) -> RuleIdentity {
        self.revision.rule_identity()
    }

    pub const fn revision(self) -> RuleRevision {
        self.revision
    }
}

/// A validated action plan whose rule is identified without an arena handle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurableActionPlan {
    rule: DurableRule,
    spec_digest: ActionSpecDigest,
    spec: ActionSpec,
}

impl DurableActionPlan {
    /// Build a durable plan from a stable rule revision and a valid action spec.
    ///
    /// # Errors
    /// Returns the action-spec diagnostic when the declared contract is invalid.
    pub fn new(rule: DurableRule, spec: ActionSpec) -> Result<Self, Diag> {
        let spec_digest = spec.digest()?;
        Ok(Self {
            rule,
            spec_digest,
            spec,
        })
    }

    pub const fn rule(&self) -> DurableRule {
        self.rule
    }

    pub const fn spec_digest(&self) -> ActionSpecDigest {
        self.spec_digest
    }

    pub fn spec(&self) -> &ActionSpec {
        &self.spec
    }
}

/// Canonical, versioned bytes for a typed Pith value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EncodedValue(Box<[u8]>);

impl EncodedValue {
    pub fn from_value(value: &Value) -> Self {
        Self(value.encode_canonical().into_boxed_slice())
    }

    /// Validate bytes read from an engine-state adapter before retaining them.
    ///
    /// # Errors
    /// Returns a canonical decoding error for malformed or unsupported data.
    pub fn from_bytes(encoded: impl Into<Box<[u8]>>) -> Result<Self, CanonicalDecodeError> {
        let encoded = encoded.into();
        Value::decode_canonical(&encoded)?;
        Ok(Self(encoded))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Decode the stored canonical value.
    ///
    /// # Errors
    /// Returns a canonical decoding error if the retained bytes are unsupported.
    pub fn decode(&self) -> Result<Value, CanonicalDecodeError> {
        Value::decode_canonical(&self.0)
    }
}

/// Store-local durable identity for one evaluation attempt.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DurableAttemptId(u64);

impl DurableAttemptId {
    pub const fn from_raw(identifier: u64) -> Self {
        Self(identifier)
    }

    pub const fn to_raw(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for DurableAttemptId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

/// The durable computation to which an attempt belongs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DurableComputation {
    Pure(PureComputationKey),
    Action {
        /// The digest half of the reusable-index key (decision 0031). The rule
        /// identity and revision the key also carries belong to the plan, and
        /// [`Self::action_key`] rebuilds the whole key from both halves.
        computation_digest: ActionComputationDigest,
        /// The request the key was built over. Retained because revalidating a
        /// consumer's action edge re-selects a rule from the interface and
        /// re-plans from the inputs (decision 0033), and because it makes
        /// `computation_digest` derivable from the rest of the record instead
        /// of a value the store takes on trust.
        request: DurableActionRequest,
        /// Boxed because a declared contract is by far the largest thing a
        /// durable computation holds, and every pure attempt would otherwise
        /// carry room for one.
        plan: Box<DurableActionPlan>,
        authorization: ActionAuthorization,
    },
}

/// The typed request one action computation was made from, without the
/// diagnostic label and span a [`Request`](pith_core::Request) also carries.
/// Neither participates in the key, and neither survives a store round trip in
/// any other record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurableActionRequest {
    pub interface: Interface,
    pub inputs: Box<[EncodedValue]>,
}

impl DurableActionRequest {
    /// Decode the request inputs so the key they were digested under can be
    /// rebuilt.
    ///
    /// # Errors
    /// Returns a canonical decoding error if a retained input is unsupported.
    pub fn decoded_inputs(&self) -> Result<Box<[Value]>, CanonicalDecodeError> {
        self.inputs.iter().map(EncodedValue::decode).collect()
    }
}

impl DurableComputation {
    pub const fn pure_key(&self) -> Option<PureComputationKey> {
        match self {
            Self::Pure(key) => Some(*key),
            Self::Action { .. } => None,
        }
    }

    /// The key the reusable action index is read and written under. Rebuilt
    /// from the plan's rule and the stored digest rather than stored whole.
    pub fn action_key(&self) -> Option<ActionComputationKey> {
        match self {
            Self::Action {
                computation_digest,
                plan,
                ..
            } => Some(ActionComputationKey {
                rule_identity: plan.rule().identity(),
                rule_revision: plan.rule().revision(),
                digest: *computation_digest,
            }),
            Self::Pure(_) => None,
        }
    }
}

/// One dependency edge. Its position in the containing slice is semantic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DurableDependency {
    Pure {
        computation: PureComputationKey,
        attempt: DurableAttemptId,
    },
    Action {
        attempt: DurableAttemptId,
    },
    Blob {
        content: ContentId,
    },
    CapabilityUse {
        capability: CapabilityRequirement,
    },
}

/// Executor data is retained as an observation, not as proof of enforcement or
/// determinism. Action plans and authorization remain on the attempt's
/// [`DurableComputation`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DurableProvenance {
    Pure,
    Action(DurableActionProvenance),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DurableActionProvenance {
    /// Execution did not begin or failed before producing a report.
    NotExecuted,
    /// The executor returned captured output, but the engine did not import it.
    Captured {
        executor_report: DurableCapturedExecutionReport,
    },
    /// The engine imported captured output into content-addressed storage.
    Imported { imported_report: ExecutionReport },
}

/// Executor metadata retained when captured artifacts could not be imported.
/// Artifact bytes remain outside engine-state metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurableCapturedExecutionReport {
    pub executor: Box<str>,
    pub platform: ExecutionPlatform,
    pub access: AccessVerification,
    pub outputs: Box<[DurableCapturedOutput]>,
    pub capabilities_used: Box<[CapabilityRequirement]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurableCapturedOutput {
    pub path: Box<str>,
    pub kind: OutputKind,
}

impl From<&CapturedExecutionReport> for DurableCapturedExecutionReport {
    fn from(report: &CapturedExecutionReport) -> Self {
        Self {
            executor: report.executor.clone(),
            platform: report.platform.clone(),
            access: report.access,
            outputs: report
                .outputs
                .iter()
                .map(|output| DurableCapturedOutput {
                    path: output.path.clone(),
                    kind: output.content.kind(),
                })
                .collect(),
            capabilities_used: report.capabilities_used.clone(),
        }
    }
}

impl DurableActionProvenance {
    pub(super) fn capabilities_used(&self) -> &[CapabilityRequirement] {
        match self {
            Self::NotExecuted => &[],
            Self::Captured { executor_report } => &executor_report.capabilities_used,
            Self::Imported { imported_report } => &imported_report.capabilities_used,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurableDiagnosticNote {
    pub span: Span,
    pub message: Box<str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurableDiagnostic {
    pub severity: Severity,
    pub code: StableCode,
    pub span: Span,
    pub message: Box<str>,
    pub notes: Box<[DurableDiagnosticNote]>,
}

impl From<&Diag> for DurableDiagnostic {
    fn from(diagnostic: &Diag) -> Self {
        Self {
            severity: diagnostic.severity,
            code: diagnostic.code,
            span: diagnostic.span,
            message: diagnostic.message.0.clone(),
            notes: diagnostic
                .notes
                .iter()
                .map(|note| DurableDiagnosticNote {
                    span: note.span,
                    message: note.message.0.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DurableReuseDecision {
    Reusable,
    NotReusable(DurableReuseReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DurableReuseReason {
    /// The engine that published this attempt had action caching switched off
    /// (decision 0031).
    ActionCachingDisabled,
    /// A pure attempt that depends on an action. A pure key does not carry the
    /// action rule's identity, so decision 0031 keeps the consumer out of the
    /// reusable index even when the action itself is in it.
    EffectfulDependency {
        attempt: DurableAttemptId,
    },
    DependencyPending {
        attempt: DurableAttemptId,
    },
    DependencyNotReusable {
        attempt: DurableAttemptId,
    },
    DependencyMissing {
        computation: PureComputationKey,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletedAttempt {
    pub dependencies: Box<[DurableDependency]>,
    pub result: EncodedValue,
    pub provenance: DurableProvenance,
    pub reuse: DurableReuseDecision,
    /// The capability requirements this computation carries, canonically
    /// ordered: an action's own declared requirements, or the union of what a
    /// pure computation's dependencies require.
    ///
    /// Propagation happens over the arena, which a hydrated node does not have
    /// (decision 0033). Recorded rather than walked back out of the store on
    /// every hydration, and checked against the recorded dependencies when the
    /// attempt is published so the two cannot disagree.
    pub capabilities: Box<[CapabilityRequirement]>,
}

/// What an attempt that stopped without a result retains. `Failed` and
/// `Cancelled` differ in *why* the attempt stopped, not in what is kept, so
/// they share this record rather than carrying two structures that would have
/// to be changed together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoppedAttempt {
    pub dependencies: Box<[DurableDependency]>,
    pub diagnostics: Box<[DurableDiagnostic]>,
    pub provenance: DurableProvenance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DurableAttemptState {
    Pending,
    Complete(CompletedAttempt),
    /// The attempt ran and could not produce its result.
    Failed(StoppedAttempt),
    /// The attempt was stopped before it could produce a result — the run it
    /// belonged to was cancelled, or a sibling's failure ended that run while
    /// this attempt was still in flight. Distinct from `Failed` because nothing
    /// about the computation itself is known to be wrong: re-running it is
    /// reasonable, where re-running a failure is not.
    Cancelled(StoppedAttempt),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurableAttempt {
    pub id: DurableAttemptId,
    pub computation: DurableComputation,
    pub state: DurableAttemptState,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DurableAttemptStatus {
    Pending,
    Complete,
    Failed,
    Cancelled,
}

impl DurableAttemptState {
    pub const fn status(&self) -> DurableAttemptStatus {
        match self {
            Self::Pending => DurableAttemptStatus::Pending,
            Self::Complete(_) => DurableAttemptStatus::Complete,
            Self::Failed(_) => DurableAttemptStatus::Failed,
            Self::Cancelled(_) => DurableAttemptStatus::Cancelled,
        }
    }
}

/// Why a completed attempt is not reusable, as a chain over the dependency
/// graph. Built by [`crate::state::EngineStateStore::explain_invalidation`] and
/// the live mirror [`crate::EngineQuery::explain_invalidation`].
///
/// The chain follows the single dependency each [`DurableReuseReason`] names.
/// When reuse is blocked by more than one dependency simultaneously, the
/// explanation records the first such edge the store reports (the validator at
/// `state::validate` derives its `first_non_reusable_dependency` the same way),
/// so the explanation matches the reuse decision the attempt was published with
/// rather than re-deriving a different one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidationExplanation {
    pub attempt: DurableAttemptId,
    pub computation: DurableComputation,
    pub reason: InvalidationReason,
}

/// One node of an invalidation chain. A leaf reason carries the
/// [`DurableReuseReason`] recorded on the attempt; an edge reason names the
/// specific dependency that is itself not reusable and recurses into its
/// explanation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvalidationReason {
    /// The attempt's own recorded reuse reason, when it does not name a
    /// dependency the chain can follow (for example `ActionCachingDisabled`).
    Leaf(DurableReuseReason),
    /// A dependency the attempt named as the cause of its non-reuse, with the
    /// recursive explanation of why that dependency is itself not reusable.
    DependencyInvalidated {
        edge: DurableDependency,
        child: Box<InvalidationExplanation>,
    },
}
