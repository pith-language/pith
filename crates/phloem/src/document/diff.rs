//! Structural comparison of lock document revisions.

use std::collections::BTreeMap;

use pith_ids::ContentId;

use crate::identity::{PackageIdentity, PackageVersion};
use crate::lock::LockEntry;

use super::Lock;

/// One moved header input, or one moved entry, of a lock diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockChange {
    Resolver,
    Scheme,
    Universe(ContentId, ContentId),
    Preferences,
    Added(LockEntry),
    Removed(LockEntry),
    /// The same package moved to a new version, whether or not its feature
    /// set moved with it.
    Upgraded {
        package: PackageIdentity,
        from: Box<str>,
        to: Box<str>,
    },
    /// The same package moved to a new feature set: features are coordinates
    /// (0040), so this is one selection moving rather than a removal and an
    /// addition.
    Features {
        package: PackageIdentity,
        from: Box<[Box<str>]>,
        to: Box<[Box<str>]>,
    },
    /// The same coordinates resolved to different content: 0039's drift.
    Drifted {
        package: PackageVersion,
        from: ContentId,
        to: ContentId,
    },
}

/// What moved between two lock documents: each header input that changed,
/// and each entry added, removed, upgraded, or drifted. This is the
/// staleness check a caller runs before resolving again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LockDiff {
    pub changes: Box<[LockChange]>,
}

/// Diff two lock documents, naming every moved input and entry.
#[must_use]
pub fn diff(before: &Lock, after: &Lock) -> LockDiff {
    let mut changes = Vec::new();
    if before.resolver != after.resolver {
        changes.push(LockChange::Resolver);
    }
    if before.scheme != after.scheme {
        changes.push(LockChange::Scheme);
    }
    if before.universe != after.universe {
        changes.push(LockChange::Universe(before.universe, after.universe));
    }
    if before.preferences != after.preferences {
        changes.push(LockChange::Preferences);
    }
    diff_entries(&mut changes, before, after);
    LockDiff {
        changes: changes.into(),
    }
}

type EntryKey = PackageIdentity;

fn key_of(entry: &LockEntry) -> EntryKey {
    entry.package.identity().clone()
}

fn diff_entries(changes: &mut Vec<LockChange>, before: &Lock, after: &Lock) {
    let before_by_key: BTreeMap<EntryKey, &LockEntry> = before
        .entries
        .iter()
        .map(|entry| (key_of(entry), entry))
        .collect();
    let after_by_key: BTreeMap<EntryKey, &LockEntry> = after
        .entries
        .iter()
        .map(|entry| (key_of(entry), entry))
        .collect();
    for (key, old) in &before_by_key {
        match after_by_key.get(key) {
            None => changes.push(LockChange::Removed((*old).clone())),
            Some(new) => {
                if old.package.version() != new.package.version() {
                    changes.push(LockChange::Upgraded {
                        package: key.clone(),
                        from: old.package.version().into(),
                        to: new.package.version().into(),
                    });
                }
                if old.features != new.features {
                    changes.push(LockChange::Features {
                        package: key.clone(),
                        from: old.features.clone(),
                        to: new.features.clone(),
                    });
                }
                // Content under moved coordinates is a new selection's content,
                // not drift; drift is content that moved while no coordinate did.
                if old.package.version() == new.package.version()
                    && old.features == new.features
                    && old.source != new.source
                {
                    changes.push(LockChange::Drifted {
                        package: old.package.clone(),
                        from: old.source,
                        to: new.source,
                    });
                }
            }
        }
    }
    for (key, new) in &after_by_key {
        if !before_by_key.contains_key(key) {
            changes.push(LockChange::Added((*new).clone()));
        }
    }
}
