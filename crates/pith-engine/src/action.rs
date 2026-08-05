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
pub trait ActionPlanner: Send + Sync {
    /// # Errors
    /// Returns structured diagnostics when the inputs cannot form a contract.
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec>;
}

/// Adapter boundary for local, remote, or test action execution.
#[async_trait]
pub trait Executor: Send + Sync {
    /// Execute `spec` and return its typed result and execution evidence.
    ///
    /// # Errors
    /// Returns structured diagnostics when the action cannot be executed.
    async fn execute(&self, spec: &ActionSpec) -> PithResult<ActionExecution>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionExecution {
    pub value: Value,
    pub evidence: ExecutionEvidence,
}

/// How strongly an executor verified the declared contract.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ContractVerification {
    /// The executor prevented access outside the declared contract.
    Enforced,
    /// The executor observed access and reported whether it matched.
    Observed,
    /// The executor could not verify access. This remains visible in provenance.
    Unverified,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProducedOutput {
    pub path: Box<str>,
    pub kind: ActionOutputKind,
    pub content: ContentId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionEvidence {
    pub executor: Box<str>,
    pub contract: ContractVerification,
    pub outputs: Box<[ProducedOutput]>,
    pub capabilities_used: Box<[CapabilityRequirement]>,
}

impl ExecutionEvidence {
    /// Evidence for adapters that can run work but cannot yet enforce or trace
    /// the declared contract. The weaker guarantee is explicit in provenance.
    pub fn unverified(executor: impl Into<Box<str>>) -> Self {
        Self {
            executor: executor.into(),
            contract: ContractVerification::Unverified,
            outputs: Box::new([]),
            capabilities_used: Box::new([]),
        }
    }
}
