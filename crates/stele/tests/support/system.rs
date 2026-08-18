//! The composed-system fixture both engine suites drive: one machine's /etc,
//! user table, service unit, and boot entry, small enough to read whole.

#![allow(
    dead_code,
    reason = "each integration-test binary uses the subset of this fixture its suites need"
)]

use std::path::PathBuf;

use pith_core::Value;
use pith_engine::{Engine, Evaluation};
use pith_ids::ContentId;
use pith_store::{ContentStore, TreeEntryContent};
use stele::types::{self, FileBody, UserEntry};

/// The machine every fixture composes.
pub(crate) const MACHINE: &str = "pith";

/// The unit file the fixture's service renders into.
pub(crate) const UNIT_NAME: &str = "example.service";

/// The expected unit text, once `after` has been concatenated and
/// canonicalized (the shorter spelling sorts first in canonical order).
pub(crate) const UNIT_TEXT: &str = "[Unit]\nDescription=an example service\nAfter=time.target network.target\nWants=network.target\n\n[Service]\nExecStart=/bin/serve --foreground\n";

/// The expected passwd text, in the account order the table carries.
pub(crate) const PASSWD_TEXT: &str = "daemon:x:1:1::/var/lib/daemon:/usr/sbin/nologin\ndeploy:x:1000:100::/home/deploy:/bin/sh\nroot:x:0:0::/root:/bin/sh\n";

/// The expected boot entry text.
pub(crate) const BOOT_TEXT: &str = "title pith\nlinux /boot/vmlinuz\ninitrd /boot/initrd\n";

pub(crate) const HOSTS: &[u8] = b"127.0.0.1 localhost\n";
pub(crate) const WELCOME: &[u8] = b"#!/bin/sh\necho welcome\n";

/// Where `find_tool` looks when `PATH` has nothing to say.
const FALLBACK_DIRS: [&str; 2] = ["/bin", "/usr/bin"];

/// The first existing `name` binary on this host, by `PATH` then the usual
/// directories. A host without it skips, honestly; a tool that is present
/// but broken fails.
pub(crate) fn find_tool(name: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|dir| dir.join(name)));
    }
    for dir in FALLBACK_DIRS {
        candidates.push(PathBuf::from(dir).join(name));
    }
    candidates.into_iter().find(|candidate| candidate.is_file())
}

/// The five host programs assembly needs, or `None` on a host without them.
/// The closure is empty: the portable fixture claims no confinement, so there
/// is nothing for it to declare. The confined linux suite builds its own tools
/// value with the closure discovery.
pub(crate) fn tools_value() -> Option<Value> {
    confined_tools_value(&[])
}

/// The five programs plus the closure a confined child opens to run them.
pub(crate) fn confined_tools_value(closure: &[&str]) -> Option<Value> {
    let shell = find_tool("sh")?;
    let mkdir = find_tool("mkdir")?;
    let cat = find_tool("cat")?;
    let chmod = find_tool("chmod")?;
    let ln = find_tool("ln")?;
    let spell = |path: &PathBuf| {
        path.to_str()
            .map(|path| path.to_string())
            .unwrap_or_default()
    };
    Some(types::tools_value(
        &spell(&shell),
        &spell(&mkdir),
        &spell(&cat),
        &spell(&chmod),
        &spell(&ln),
        closure,
    ))
}

/// The five programs' absolute paths, for closure discovery.
pub(crate) fn tool_paths() -> Option<Vec<String>> {
    let found: Vec<PathBuf> = ["sh", "mkdir", "cat", "chmod", "ln"]
        .iter()
        .map(|tool| find_tool(tool))
        .collect::<Option<Vec<_>>>()?;
    found
        .iter()
        .map(|path| path.to_str().map(str::to_owned))
        .collect()
}

/// Everything a compose-system request supplies, over content already in the
/// engine's store.
pub(crate) struct SystemFixture {
    pub tools: Value,
    pub boot: Value,
    pub etc: Value,
    pub users: Value,
    pub policy: Value,
    pub units: Value,
    pub replacements: Value,
}

pub(crate) fn fixture(engine: &mut Engine, tools: Value) -> SystemFixture {
    let hosts = match engine.put_blob(HOSTS) {
        Ok(id) => id,
        Err(error) => unreachable!("the store failed to hold the hosts file: {error:?}"),
    };
    let welcome = match engine.put_blob(WELCOME) {
        Ok(id) => id,
        Err(error) => unreachable!("the store failed to hold the welcome script: {error:?}"),
    };

    let base_files = types::file_set_value([
        (
            "etc/hosts",
            FileBody::File {
                content: hosts,
                executable: false,
            },
        ),
        (
            "etc/profile.d/welcome.sh",
            FileBody::File {
                content: welcome,
                executable: true,
            },
        ),
    ]);
    let site_files = types::file_set_value([
        (
            "etc/localtime",
            FileBody::Symlink {
                target: "../pool/zoneinfo/UTC".into(),
            },
        ),
        (
            "etc/hosts-link",
            FileBody::Symlink {
                target: "hosts".into(),
            },
        ),
    ]);
    let etc = types::etc_contributions(&[("base", base_files), ("site", site_files)]);

    let base_users = types::user_table_value(&[
        UserEntry {
            name: "root".into(),
            uid: 0,
            gid: 0,
            home: "/root".into(),
            shell: "/bin/sh".into(),
        },
        UserEntry {
            name: "daemon".into(),
            uid: 1,
            gid: 1,
            home: "/var/lib/daemon".into(),
            shell: "/usr/sbin/nologin".into(),
        },
    ]);
    let site_users = types::user_table_value(&[UserEntry {
        name: "deploy".into(),
        uid: 1000,
        gid: 100,
        home: "/home/deploy".into(),
        shell: "/bin/sh".into(),
    }]);
    let users = types::user_contributions(&[("base", base_users), ("site", site_users)]);

    let base_unit = types::unit_value(
        UNIT_NAME,
        "an example service",
        "/bin/serve --foreground",
        &["network.target"],
        &[],
    );
    let site_unit = types::unit_value(
        UNIT_NAME,
        "an example service",
        "/bin/serve --foreground",
        &["time.target"],
        &["network.target"],
    );
    let units = types::unit_contributions(&[("base", base_unit), ("site", site_unit)]);

    SystemFixture {
        tools,
        boot: types::boot_value(MACHINE, "/boot/vmlinuz", "/boot/initrd"),
        etc,
        users,
        policy: types::unit_policy_value(&[
            ("after", types::Behavior::Concat),
            ("wants", types::Behavior::Concat),
        ]),
        units,
        replacements: types::unit_replacements(&[]),
    }
}

impl SystemFixture {
    #[must_use]
    pub fn request(&self) -> pith_core::Request<pith_core::Pure> {
        types::compose_system_request(
            self.tools.clone(),
            self.boot.clone(),
            self.etc.clone(),
            self.users.clone(),
            self.policy.clone(),
            self.units.clone(),
            self.replacements.clone(),
        )
    }
}

/// One flattened view of a stored tree, for asserting what an artifact
/// carries at every path.
#[derive(Debug)]
pub(crate) enum ArtifactEntry {
    File { bytes: Vec<u8>, executable: bool },
    Symlink { target: Vec<u8> },
}

pub(crate) fn artifact_id(evaluation: &Evaluation) -> ContentId {
    match &evaluation.value {
        Value::Nominal {
            name,
            representation,
        } if name.as_ref() == "stele.SystemTree" => match representation.as_ref() {
            Value::Blob(id) => *id,
            other => unreachable!("a system tree carries content, not {other:?}"),
        },
        other => unreachable!("a compose completes with a system tree, not {other:?}"),
    }
}

/// Flatten the tree `root` names into `path -> entry`, reading file bytes
/// through a second handle on the store, since the engine owns the one it was
/// built with.
pub(crate) fn artifact_entries(
    store: &dyn ContentStore,
    root: ContentId,
) -> std::collections::BTreeMap<String, ArtifactEntry> {
    let mut entries = std::collections::BTreeMap::new();
    walk_artifact(store, root, String::new(), &mut entries);
    entries
}

fn walk_artifact(
    store: &dyn ContentStore,
    root: ContentId,
    prefix: String,
    entries: &mut std::collections::BTreeMap<String, ArtifactEntry>,
) {
    let tree = match store.get_tree(root) {
        Ok(Some(tree)) => tree,
        Ok(None) => unreachable!("an artifact subtree was not in the store"),
        Err(error) => unreachable!("the store failed to read an artifact subtree: {error:?}"),
    };
    for entry in tree.entries() {
        let path = if prefix.is_empty() {
            entry.name().to_string()
        } else {
            format!("{prefix}/{}", entry.name())
        };
        match entry.content() {
            TreeEntryContent::File(file) => {
                let bytes = match store.get_blob(file.content) {
                    Ok(Some(blob)) => blob.as_bytes().to_vec(),
                    Ok(None) => unreachable!("an artifact file was not in the store"),
                    Err(error) => unreachable!("the store failed to read a file: {error:?}"),
                };
                entries.insert(
                    path,
                    ArtifactEntry::File {
                        bytes,
                        executable: file.executable,
                    },
                );
            }
            TreeEntryContent::Tree(child) => {
                walk_artifact(store, *child, path, entries);
            }
            TreeEntryContent::Symlink { target } => {
                entries.insert(
                    path,
                    ArtifactEntry::Symlink {
                        target: target.to_vec(),
                    },
                );
            }
        }
    }
}

/// Open the content store at `root` for the reading the engine's own handle
/// cannot do.
pub(crate) fn open_store(root: &std::path::Path) -> pith_store::FilesystemContentStore {
    match pith_store::FilesystemContentStore::open(root) {
        Ok(store) => store,
        Err(error) => unreachable!("the store failed to reopen: {error:?}"),
    }
}
