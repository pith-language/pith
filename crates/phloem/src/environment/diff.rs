use pith_core::Value;
use pith_engine::ExecutionPlatform;

use crate::document::{LockChange, diff as diff_locks};

use super::EnvironmentDocument;

/// One moved input of an environment diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnvironmentChange {
    /// A change reported by the lock diff.
    Lock(LockChange),
    Platform {
        from: ExecutionPlatform,
        to: ExecutionPlatform,
    },
    Toolchain {
        from: Value,
        to: Value,
    },
    /// The admitted substitutions changed.
    Substitutions,
}

/// Compares two environment documents.
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
