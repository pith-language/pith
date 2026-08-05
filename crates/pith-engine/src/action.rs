//! Planning and execution surfaces for the `Action` effect category.
//!
//! Planning is synchronous and produces inert [`pith_core::ActionSpec`] data.
//! Only an [`Executor`] performs external work. This keeps arbitrary async
//! callbacks from being mislabeled as declared actions.

use async_trait::async_trait;
use pith_core::{ActionOutputKind, ActionSpec, CapabilityRequirement, Value};
use pith_diag::PithResult;
use pith_ids::ContentId;

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
    /// Execute `invocation` and return captured output bytes and a report.
    ///
    /// # Errors
    /// Returns structured diagnostics when the action cannot be executed.
    async fn execute(&self, invocation: &ActionInvocation) -> PithResult<CapturedActionExecution>;
}

/// Least-authority input view passed to an executor for one action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionInvocation {
    pub spec: ActionSpec,
    pub executable: MaterializedContent,
    pub inputs: Box<[MaterializedActionInput]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedActionInput {
    pub path: Box<str>,
    pub content: MaterializedContent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterializedContent {
    Blob { id: ContentId, bytes: Box<[u8]> },
    Tree(MaterializedTree),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedTree {
    pub id: ContentId,
    pub entries: Box<[MaterializedTreeEntry]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedTreeEntry {
    pub name: Box<str>,
    pub content: MaterializedTreeEntryContent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterializedTreeEntryContent {
    File {
        content: ContentId,
        executable: bool,
        bytes: Box<[u8]>,
    },
    Tree(MaterializedTree),
    Symlink {
        target: Box<[u8]>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedActionExecution {
    pub report: CapturedExecutionReport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionExecution {
    pub report: ExecutionReport,
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

/// Concrete platform selected by an executor for one execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionPlatform {
    pub operating_system: Box<str>,
    pub architecture: Box<str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProducedOutput {
    pub path: Box<str>,
    pub kind: ActionOutputKind,
    pub content: ContentId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedOutput {
    pub path: Box<str>,
    pub kind: ActionOutputKind,
    pub content: CapturedOutputContent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapturedOutputContent {
    Blob(Box<[u8]>),
    Tree(CapturedTree),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedTree {
    pub entries: Box<[CapturedTreeEntry]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedTreeEntry {
    pub name: Box<str>,
    pub content: CapturedTreeEntryContent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapturedTreeEntryContent {
    File { bytes: Box<[u8]>, executable: bool },
    Tree(CapturedTree),
    Symlink { target: Box<[u8]> },
}

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
