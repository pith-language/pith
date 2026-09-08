//! The frontend's dependency closure: no kernel crate links a first-party
//! domain, which the compiler and query drivers would otherwise reach
//! through this crate. The domain set is derived from the crates directory,
//! so a domain added later is covered without editing a list here.

use std::path::{Path, PathBuf};

/// The workspace root, from this crate's manifest directory.
fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(std::borrow::ToOwned::to_owned)
        .unwrap_or_else(|| unreachable!("this crate lives two levels under the workspace root"))
}

/// Every crate directory under `crates/`.
fn crate_directories(root: &Path) -> Vec<PathBuf> {
    let mut directories = std::fs::read_dir(root.join("crates"))
        .unwrap_or_else(|error| unreachable!("the workspace has a crates directory: {error:?}"))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    directories.sort();
    directories
}

/// The first-party domains: crate directories that are not kernel crates.
/// Derived, so a new domain is covered without editing this test.
fn domains(root: &Path) -> Vec<String> {
    crate_directories(root)
        .into_iter()
        .filter_map(|directory| directory.file_name()?.to_str().map(str::to_owned))
        .filter(|name| !name.starts_with("pith-"))
        .collect()
}

/// The dependency names a manifest links: the `[dependencies]` and
/// `[build-dependencies]` tables, never the `[dev-dependencies]` a test
/// fixture reaches for.
fn linked_dependencies(manifest: &str) -> Vec<String> {
    let mut linked = Vec::new();
    let mut in_link_table = false;
    for line in manifest.lines() {
        if line.starts_with('[') {
            let table = line.trim();
            in_link_table = table.eq_ignore_ascii_case("[dependencies]")
                || table.eq_ignore_ascii_case("[build-dependencies]");
            continue;
        }
        if in_link_table && let Some((name, _)) = line.split_once('=') {
            linked.push(name.trim().to_owned());
        }
    }
    linked
}

#[test]
fn no_kernel_crate_links_a_first_party_domain() {
    let root = workspace_root();
    let domains = domains(&root);
    assert!(
        !domains.is_empty(),
        "the workspace has first-party domains for this test to be about"
    );
    let kernel: Vec<PathBuf> = crate_directories(&root)
        .into_iter()
        .filter(|directory| {
            directory
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("pith-"))
        })
        .collect();
    assert!(
        !kernel.is_empty(),
        "the workspace has kernel crates for this test to be about"
    );
    for crate_directory in kernel {
        let manifest = crate_directory.join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest)
            .unwrap_or_else(|error| unreachable!("{} is readable: {error:?}", manifest.display()));
        let linked = linked_dependencies(&text);
        let linked_domains = linked
            .iter()
            .filter(|dependency| domains.contains(dependency))
            .cloned()
            .collect::<Vec<_>>();
        assert!(
            linked_domains.is_empty(),
            "`{}` links the first-party domains {linked_domains:?}; the driver reaches module \
             resolution and the frontend through kernel crates only",
            crate_directory.display(),
        );
    }
}
