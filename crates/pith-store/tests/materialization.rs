use std::fs;
use std::path::Path;

use pith_ids::ContentId;
use pith_store::{
    ContentStore, FileContent, MemoryContentStore, Tree, TreeEntry, TreeEntryContent,
    materialize_tree,
};

fn file(name: &str, content: ContentId, executable: bool) -> TreeEntry<FileContent, ContentId> {
    match TreeEntry::new(
        name,
        TreeEntryContent::File(FileContent {
            content,
            executable,
        }),
    ) {
        Ok(entry) => entry,
        Err(error) => unreachable!("the fixture entry is valid: {error}"),
    }
}

#[test]
fn materializes_files_child_trees_executability_and_symlinks() {
    let Ok(directory) = tempfile::tempdir() else {
        return;
    };
    let mut store = MemoryContentStore::default();
    let regular = match store.put_blob(b"regular bytes") {
        Ok(id) => id,
        Err(error) => unreachable!("memory store rejected a blob: {error}"),
    };
    let executable = match store.put_blob(b"#!/bin/sh\nexit 0\n") {
        Ok(id) => id,
        Err(error) => unreachable!("memory store rejected a blob: {error}"),
    };
    let nested = match store.put_blob(b"nested bytes") {
        Ok(id) => id,
        Err(error) => unreachable!("memory store rejected a blob: {error}"),
    };
    let child = match Tree::new([file("nested", nested, false)]) {
        Ok(tree) => tree,
        Err(error) => unreachable!("the fixture tree is valid: {error}"),
    };
    assert!(store.put_tree(child.clone()).is_ok());
    let root = match Tree::new([
        file("regular", regular, false),
        file("run", executable, true),
        match TreeEntry::new("child", TreeEntryContent::Tree(child.id())) {
            Ok(entry) => entry,
            Err(error) => unreachable!("the fixture entry is valid: {error}"),
        },
        match TreeEntry::new(
            "regular-link",
            TreeEntryContent::Symlink {
                target: b"regular".as_slice().into(),
            },
        ) {
            Ok(entry) => entry,
            Err(error) => unreachable!("the fixture entry is valid: {error}"),
        },
    ]) {
        Ok(tree) => tree,
        Err(error) => unreachable!("the fixture tree is valid: {error}"),
    };
    assert!(store.put_tree(root.clone()).is_ok());
    let output = directory.path().join("output");

    assert!(materialize_tree(&store, root.id(), &output).is_ok());
    assert_eq!(
        fs::read(output.join("regular")).ok().as_deref(),
        Some(b"regular bytes".as_slice())
    );
    assert_eq!(
        fs::read(output.join("child/nested")).ok().as_deref(),
        Some(b"nested bytes".as_slice())
    );
    assert_eq!(
        fs::read(output.join("regular-link")).ok().as_deref(),
        Some(b"regular bytes".as_slice())
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(output.join("run"))
            .map(|metadata| metadata.permissions().mode())
            .unwrap_or_default();
        assert_ne!(mode & 0o111, 0);
        assert_eq!(
            fs::read_link(output.join("regular-link")).ok().as_deref(),
            Some(Path::new("regular"))
        );
    }
}

#[test]
fn missing_referenced_content_leaves_no_destination() {
    let Ok(directory) = tempfile::tempdir() else {
        return;
    };
    let missing = ContentId::of_blob(b"not stored");
    let tree = match Tree::new([file("missing", missing, false)]) {
        Ok(tree) => tree,
        Err(error) => unreachable!("the fixture tree is valid: {error}"),
    };
    let mut store = MemoryContentStore::default();
    assert!(store.put_tree(tree.clone()).is_ok());
    let output = directory.path().join("output");

    let error = materialize_tree(&store, tree.id(), &output).err();

    assert!(error.is_some_and(|error| error.to_string().contains("is not available")));
    assert!(!output.exists());
    let staging_count = fs::read_dir(directory.path())
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".pith-materialize-")
        })
        .count();
    assert_eq!(staging_count, 0);
}

#[test]
fn refuses_to_replace_an_existing_destination() {
    let Ok(directory) = tempfile::tempdir() else {
        return;
    };
    let output = directory.path().join("output");
    assert!(fs::create_dir(&output).is_ok());
    assert!(fs::write(output.join("keep"), b"untouched").is_ok());
    let tree = match Tree::new([]) {
        Ok(tree) => tree,
        Err(error) => unreachable!("the empty tree is valid: {error}"),
    };
    let mut store = MemoryContentStore::default();
    assert!(store.put_tree(tree.clone()).is_ok());

    let error = materialize_tree(&store, tree.id(), &output).err();

    assert!(error.is_some_and(|error| error.to_string().contains("already exists")));
    assert_eq!(
        fs::read(output.join("keep")).ok().as_deref(),
        Some(b"untouched".as_slice())
    );
}
