//! The Action effect category execution surface (decisions 0019, 0022).
//!
//! Action bodies are async because they perform bounded external work; the
//! engine drives them through the [`crate::Runtime`] trait. `async_trait` is
//! used because the engine stores action bodies as `Box<dyn ActionRule>` for
//! runtime lookup by id, and native `async fn` in traits does not support
//! dyn dispatch.

use async_trait::async_trait;
use pith_core::Value;
use pith_diag::PithResult;

/// Executable body for an `Action` rule. `execute` is driven by the engine's
/// async scheduler when a Pure rule yields a `PureStep::NeedAction`.
///
/// # Errors
/// Returns structured diagnostics when the action cannot produce its value.
#[async_trait]
pub trait ActionRule: Send + Sync {
    async fn execute(&self, inputs: &[Value]) -> PithResult<Value>;
}
