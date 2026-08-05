//! The async runtime trait (decision 0022). tokio sits behind it; the engine
//! never names tokio in a public signature.

use std::future::Future;

pub trait Runtime: Send + Sync {
    /// # Errors
    /// Returns `RuntimeError` if the underlying runtime could not be
    /// constructed or driven.
    fn block_on<F>(&self, future: F) -> Result<F::Output, RuntimeError>
    where
        F: Future + Send,
        F::Output: Send;
}

#[derive(Debug)]
pub struct RuntimeError(pub Box<str>);

pub struct TokioRuntime;

impl Runtime for TokioRuntime {
    fn block_on<F>(&self, future: F) -> Result<F::Output, RuntimeError>
    where
        F: Future + Send,
        F::Output: Send,
    {
        tokio::runtime::Runtime::new()
            .map(|rt| rt.block_on(future))
            .map_err(|e| RuntimeError(e.to_string().into()))
    }
}
