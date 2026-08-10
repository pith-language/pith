//! The caller's side of cancellation (decision 0022).
//!
//! The engine polls for cancellation at scheduling boundaries rather than being
//! pushed to. That keeps the async runtime out of this signature the same way
//! [`Runtime`](crate::Runtime) keeps it out of evaluation: a host backs this
//! with whatever it already has — an `AtomicBool` a signal handler sets, a flag
//! behind a mutex, a channel it drains — and the engine never names a
//! concurrency primitive it would then be committed to.

/// A caller's standing request to stop a run.
///
/// Polled between scheduling steps, so cancellation takes effect at the next
/// boundary rather than immediately: a chain being stepped runs to its next
/// park point, and an action already handed to an executor is stopped by
/// dropping it. Implementations must be cheap to call and must not block.
pub trait CancelSignal: Send + Sync {
    fn is_cancelled(&self) -> bool;
}

/// The signal for a run that nobody can cancel. What [`Engine::run`] uses.
///
/// [`Engine::run`]: crate::Engine::run
#[derive(Copy, Clone, Debug, Default)]
pub struct NeverCancelled;

impl CancelSignal for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

impl CancelSignal for std::sync::atomic::AtomicBool {
    fn is_cancelled(&self) -> bool {
        self.load(std::sync::atomic::Ordering::Relaxed)
    }
}
