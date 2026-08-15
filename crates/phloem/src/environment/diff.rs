use pith_core::Value;
use pith_engine::ExecutionPlatform;

use crate::document::{LockChange, diff as diff_locks};

use super::EnvironmentDocument;

/// One moved input of an environment diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnvironmentChange {
    /// A moved input or entry of the lock the environment holds, reported
    /// by the lock's own diff.
    Lock(LockChange),
    Platform {
        from: ExecutionPlatform,
        to: ExecutionPlatform,
    },
    Toolchain {
        from: Value,
        to: Value,
    },
    /// The set of served substitutions moved, which happens when offers or
    /// the admission policy moved and the selection did not.
    Substitutions,
}

/// What moved between two revisions of one environment: each moved lock
/// input or entry, and each moved realization coordinate. The staleness
/// check a caller runs before resolving again, on the same shape as the
/// lock's own.
#[must_use]
pub fn diff(before: &EnvironmentDocument, after: &EnvironmentDocument) -> Box<[EnvironmentChange]> {
    let mut changes = Vec::new();
    for change in diff_locks(&before.lock, &after.lock).changes.iter() {
        changes.push(EnvironmentChange::Lock(change.clone()));
    }
    if before.platform != after.platform {
        changes.push(EnvironmentChange::Platform {
            from: before.platform.clone(),
            to: after.platform.clone(),
        });
    }
    if before.toolchain != after.toolchain {
        changes.push(EnvironmentChange::Toolchain {
            from: before.toolchain.clone(),
            to: after.toolchain.clone(),
        });
    }
    if before.substitutions != after.substitutions {
        changes.push(EnvironmentChange::Substitutions);
    }
    changes.into()
}
