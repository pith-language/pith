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

/// A multi-threaded tokio runtime, owned by whoever built it.
///
/// The caller holds it rather than the crate holding one globally: a run drives
/// every action it started on this runtime, so the runtime outliving a single
/// `run` call is load-bearing, and a process-wide one would make that lifetime
/// implicit, unconfigurable, and impossible to tear down. Owning it also keeps
/// the adapter boundary honest — the runtime is a thing the host supplies, the
/// same as a content store or an executor.
pub struct TokioRuntime {
    runtime: tokio::runtime::Runtime,
}

impl TokioRuntime {
    /// Build a runtime for the engine to drive its runs on.
    ///
    /// # Errors
    /// Returns `RuntimeError` if the thread pool could not be created.
    pub fn new() -> Result<Self, RuntimeError> {
        tokio::runtime::Runtime::new()
            .map(|runtime| Self { runtime })
            .map_err(|error| RuntimeError(error.to_string().into()))
    }
}

impl Runtime for TokioRuntime {
    fn block_on<F>(&self, future: F) -> Result<F::Output, RuntimeError>
    where
        F: Future + Send,
        F::Output: Send,
    {
        Ok(self.runtime.block_on(future))
    }
}
