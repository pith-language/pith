//! The other half of the U-10 claim: carrying this domain took no change
//! anywhere else.
//!
//! The peerhood suite shows the domain works through the public engine. That
//! is only evidence of peerhood if nothing was added elsewhere to make it work,
//! so these tests read the tree: no crate in the workspace names this one, and
//! this crate depends on no other domain. Both are derived — the crate's own
//! name comes from cargo, and the set of domains comes from the crates
//! directory — so a crate added later is covered without editing a list here.

use std::path::{Path, PathBuf};

/// The workspace root, from this crate's manifest directory.
fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    match manifest.parent().and_then(Path::parent) {
        Some(root) => root.to_path_buf(),
        None => unreachable!("this crate lives two levels under the workspace root"),
    }
}

/// Every rust source and manifest under `directory`.
fn sources(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(directory) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(sources(&path));
            continue;
        }
        let is_source = path.extension().is_some_and(|extension| extension == "rs");
        let is_manifest = path.file_name().is_some_and(|name| name == "Cargo.toml");
        if is_source || is_manifest {
            found.push(path);
        }
    }
    found
}

/// The crate directories under `crates/`, which is where every library in the
/// workspace lives.
fn crate_directories(root: &Path) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    let Ok(entries) = std::fs::read_dir(root.join("crates")) else {
        unreachable!("the workspace has a crates directory");
    };
    for entry in entries.flatten() {
        if entry.path().is_dir() {
            directories.push(entry.path());
        }
    }
    directories.sort();
    directories
}

fn own_directory_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[test]
fn no_other_crate_in_the_workspace_names_this_domain() {
    let root = workspace_root();
    let spellings = [
        own_directory_name().to_owned(),
        own_directory_name().replace('-', "_"),
    ];

    let mut searched = 0_usize;
    let mut offenders = Vec::new();
    let mut directories = crate_directories(&root);
    directories.push(root.join("xtask"));
    for directory in directories {
        if directory
            .file_name()
            .is_some_and(|name| name == own_directory_name())
        {
            continue;
        }
        for path in sources(&directory) {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            searched = searched.saturating_add(1);
            if spellings
                .iter()
                .any(|spelling| text.contains(spelling.as_str()))
            {
                offenders.push(path);
            }
        }
    }

    assert!(searched > 0, "the search found no files to read");
    assert!(
        offenders.is_empty(),
        "carrying this domain required a change in {offenders:?}, so it is not \
         registered through the public interface alone"
    );
}

#[test]
fn this_domain_depends_on_no_other_domain() {
    let root = workspace_root();
    let manifest =
        match std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        {
            Ok(manifest) => manifest,
            Err(error) => unreachable!("this crate has a manifest: {error:?}"),
        };

    // A domain is a crate that is neither a kernel crate nor this one. Deriving
    // the set means a third domain added later is covered here without an edit.
    let domains: Vec<String> = crate_directories(&root)
        .iter()
        .filter_map(|directory| directory.file_name()?.to_str().map(str::to_owned))
        .filter(|name| !name.starts_with("pith-") && name != own_directory_name())
        .collect();

    assert!(
        !domains.is_empty(),
        "the workspace has first-party domains for this test to be about"
    );
    for domain in domains {
        assert!(
            !manifest.contains(&domain),
            "this domain depends on `{domain}`, so it is not independent evidence"
        );
    }
}
