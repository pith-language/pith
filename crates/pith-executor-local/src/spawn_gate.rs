//! Keeps executable staging out of another action's fork.
//!
//! `execve` fails with `ETXTBSY` while any process holds the executable open
//! for writing. Rust opens files `O_CLOEXEC`, so the handle used to stage an
//! executable is closed at `execve` — but not at `fork`, and a forked child
//! holds a copy of every parent descriptor from the fork until it execs. Two
//! actions staging and spawning at the same time can therefore leave one
//! action's pre-exec child holding a writer on the other's freshly staged
//! executable, and that other spawn fails for a reason that has nothing to do
//! with it.
//!
//! The only writers of a staged executable are [`crate::stage`] and the
//! descriptors a fork copies out of it, so excluding the two from each other
//! removes the condition rather than waiting for it to pass: while a child is
//! being forked, no executable is being written, so there is nothing for the
//! child to inherit.
//!
//! The gate is process-wide because the file-descriptor table is. A gate owned
//! by a [`crate::LocalExecutor`] would be defeated by a second executor in the
//! same process, whose forks would inherit the first one's descriptors just the
//! same.
//!
//! Forking is the exclusive side and writing the shared one, which is the way
//! round that costs least: any number of actions stage at once, and only the
//! fork-to-exec window — microseconds, and over before `spawn` returns — is
//! serialized.

use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

static GATE: RwLock<()> = RwLock::const_new(());

/// Held while an executable is written into a scratch root. Concurrent with
/// other staging; excluded from every fork.
pub(crate) async fn writing_executable() -> RwLockReadGuard<'static, ()> {
    GATE.read().await
}

/// Held across `fork`/`execve`. Excludes every in-progress executable write, so
/// the child inherits no descriptor that would make an executable busy.
pub(crate) async fn forking_child() -> RwLockWriteGuard<'static, ()> {
    GATE.write().await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Which side of the gate is the exclusive one is the whole design, and
    /// swapping the two would still compile, still pass every other test, and
    /// still leave the race open. The probe reads the gate directly rather than
    /// calling the helpers, because a helper that blocked would hang the test
    /// instead of failing it.
    #[tokio::test]
    async fn staging_is_shared_and_forking_is_exclusive() {
        {
            let _writing = writing_executable().await;
            assert!(
                GATE.try_read().is_ok(),
                "two actions could not stage an executable at the same time"
            );
        }
        let _forking = forking_child().await;
        assert!(
            GATE.try_read().is_err(),
            "an executable could be written while a child was being forked"
        );
    }
}
