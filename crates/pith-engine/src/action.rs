//! Planning and execution surfaces for the `Action` effect category.
//!
//! Planning is synchronous and produces inert [`pith_core::ActionSpec`] data.
//! Only an [`Executor`] performs external work. This keeps arbitrary async
//! callbacks from being mislabeled as declared actions.

use async_trait::async_trait;
use pith_core::{ActionSpec, CapabilityRequirement, Content, Value};
use pith_diag::PithResult;
use pith_ids::ContentId;
use pith_store::TreeEntry;

/// Deterministically turn typed request inputs into an inspectable contract.
pub trait ActionRule: Send + Sync {
    /// # Errors
    /// Returns structured diagnostics when the inputs cannot form a contract.
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec>;

    /// Convert captured execution outputs into the rule's typed result.
    ///
    /// # Errors
    /// Returns structured diagnostics when the execution cannot produce the
    /// declared semantic result.
    fn complete(&self, inputs: &[Value], execution: &ActionExecution) -> PithResult<Value>;
}

/// Adapter boundary for local, remote, or test action execution.
#[async_trait]
pub trait Executor: Send + Sync {
    /// Who this executor is and where it runs, before it has run anything.
    ///
    /// The engine asks this before consulting the reusable action index
    /// (decision 0031): an attempt recorded by a different executor, or on a
    /// different platform, does not answer this run's question. Every report
    /// this executor produces carries the same two values.
    fn identity(&self) -> ExecutorIdentity;

    /// Execute `invocation` and return captured output bytes and a report.
    ///
    /// # Errors
    /// Returns structured diagnostics when the action cannot be executed.
    async fn execute(&self, invocation: &ActionInvocation) -> PithResult<CapturedActionExecution>;
}

/// The executor half of an execution report, knowable before execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutorIdentity {
    pub executor: Box<str>,
    pub platform: ExecutionPlatform,
}

/// Least-authority input view passed to an executor for one action.
///
/// The executable is a host path carried in [`ActionSpec::executable`] (decision
/// 0030); the executor `execve`s it directly and does not receive materialized
/// bytes for it. Only declared source inputs are materialized here, since they
/// are content the engine resolves from its store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionInvocation {
    pub spec: ActionSpec,
    pub inputs: Box<[MaterializedActionInput]>,
    /// The bytes of the program, when the contract named content rather than a
    /// host path (`ActionProgram::Content`). The engine resolves it from the
    /// content store on the same terms as an input, so the executor never
    /// touches the store. `None` for a host-path program, whose bytes belong to
    /// the host.
    pub program: Option<MaterializedBlob>,
    /// The wall-clock deadline of the run starting this action, when that run
    /// declared one (decision 0059). An executor that receives one ends the
    /// child at it and refuses with the bound's code rather than capturing
    /// anything. Authority for an execution, not content of the contract: it
    /// participates in no computation key, and a recorded attempt serves a run
    /// under any bound, because reuse serves it without running anything.
    pub deadline: Option<std::time::Instant>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedActionInput {
    pub path: Box<str>,
    pub content: MaterializedContent,
}

/// A materialized blob: its content identity and the local bytes that identity
/// resolves to. The payload of [`MaterializedContent::Blob`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedBlob {
    pub id: ContentId,
    pub bytes: Box<[u8]>,
}

/// Top-level materialized content: a blob (id + bytes) or a materialized tree.
/// A specialization of [`pith_core::Content`]; the Blob/Tree discriminator is
/// the single source of truth shared with every other phase.
pub type MaterializedContent = Content<MaterializedBlob, MaterializedTree>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedTree {
    pub id: ContentId,
    pub entries: Box<[MaterializedTreeEntry]>,
}

/// A materialized tree entry. An instantiation of the store's generic
/// [`pith_store::TreeEntry`] over the materialized file payload and the
/// materialized tree recursion.
pub type MaterializedTreeEntry = TreeEntry<MaterializedFileContent, MaterializedTree>;

/// A materialized file: its content identity, executability, and local bytes.
/// Carries everything the canonical store form does, plus the bytes a blob
/// materializes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedFileContent {
    pub content: ContentId,
    pub executable: bool,
    pub bytes: Box<[u8]>,
}

/// Materialized tree-entry content. An instantiation of the store's generic
/// [`pith_store::TreeEntryContent`]; `Symlink` is inherited unchanged from the
/// canonical form.
pub type MaterializedTreeEntryContent =
    pith_store::TreeEntryContent<MaterializedFileContent, MaterializedTree>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedActionExecution {
    pub report: CapturedExecutionReport,
    /// How the program ended, when the executor observed it. `None` from an
    /// executor that does not report one.
    pub exit: Option<ActionExit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionExecution {
    pub report: ExecutionReport,
    /// How the program ended, carried through from the executor so
    /// [`ActionRule::complete`] can read a verdict out of it under
    /// `ExitStatusContract::Reported`.
    ///
    /// [`ExecutionReport`] is the part that persists, and this is not on it.
    /// `complete` runs only when an action actually executes, and a later run is
    /// served the value it derived, so the derived value is what the graph keeps.
    pub exit: Option<ActionExit>,
}

/// How a child stopped. A signal is not an exit code, and a rule reading a
/// verdict needs to tell "the test reported failures" from "the test crashed".
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ActionExit {
    /// The program returned this status from `main` or `exit`.
    Code(i32),
    /// The program was killed by this signal and returned no status.
    Signal(i32),
}

/// Access-control mechanism reported by an executor.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AccessVerification {
    /// The executor reports that it prevented access outside the contract.
    Prevented,
    /// The executor reports that it observed access and checked the contract.
    Observed,
    /// The executor did not verify access.
    Unverified,
}

impl AccessVerification {
    /// Whether this level is at least as strong a claim as `minimum`. Used to
    /// decide whether a recorded attempt's confinement is good enough for a run
    /// that demands more (decision 0031).
    #[must_use]
    pub const fn satisfies(self, minimum: Self) -> bool {
        self.strength() >= minimum.strength()
    }

    /// Ordering of the three claims. The variants are declared strongest
    /// first, so a derived `Ord` would mean the opposite of this.
    const fn strength(self) -> u8 {
        match self {
            Self::Unverified => 0,
            Self::Observed => 1,
            Self::Prevented => 2,
        }
    }
}

/// Concrete platform selected by an executor for one execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionPlatform {
    pub operating_system: Box<str>,
    pub architecture: Box<str>,
}

/// One output the engine imported from the executor's report. The Blob/Tree
/// discriminator lives in the [`Content`] payload; there is no separate `kind`
/// field to drift out of agreement with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProducedOutput {
    pub path: Box<str>,
    pub content: Content<ContentId, ContentId>,
}

/// One output an executor reports capturing. The Blob/Tree discriminator lives
/// in the [`CapturedOutputContent`] payload; the engine no longer hand-matches
/// a separate kind against it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedOutput {
    pub path: Box<str>,
    pub content: CapturedOutputContent,
}

/// Top-level captured content: raw blob bytes or a captured tree. A
/// specialization of [`pith_core::Content`].
pub type CapturedOutputContent = Content<Box<[u8]>, CapturedTree>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedTree {
    pub entries: Box<[CapturedTreeEntry]>,
}

/// A captured tree entry. An instantiation of the store's generic
/// [`pith_store::TreeEntry`] over the captured file payload and the captured
/// tree recursion.
pub type CapturedTreeEntry = TreeEntry<CapturedFileContent, CapturedTree>;

/// A captured file: its bytes and executability, before the engine has
/// content-addressed it. Carries no `ContentId`; that is assigned on import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedFileContent {
    pub bytes: Box<[u8]>,
    pub executable: bool,
}

/// Captured tree-entry content. An instantiation of the store's generic
/// [`pith_store::TreeEntryContent`]; `Symlink` is inherited unchanged.
pub type CapturedTreeEntryContent = pith_store::TreeEntryContent<CapturedFileContent, CapturedTree>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionReport {
    pub executor: Box<str>,
    pub platform: ExecutionPlatform,
    pub access: AccessVerification,
    pub outputs: Box<[ProducedOutput]>,
    pub capabilities_used: Box<[CapabilityRequirement]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedExecutionReport {
    pub executor: Box<str>,
    pub platform: ExecutionPlatform,
    pub access: AccessVerification,
    pub outputs: Box<[CapturedOutput]>,
    pub capabilities_used: Box<[CapabilityRequirement]>,
}

impl ExecutionReport {
    pub fn unverified(executor: impl Into<Box<str>>, platform: ExecutionPlatform) -> Self {
        Self {
            executor: executor.into(),
            platform,
            access: AccessVerification::Unverified,
            outputs: Box::new([]),
            capabilities_used: Box::new([]),
        }
    }
}
