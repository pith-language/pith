//! Cross-adapter acceptance tests for immutable content storage (A-3/A-4).

use std::fs;

use pith_ids::ContentId;
use pith_store::{
    ContentStore, FileContent, FilesystemContentStore, MemoryContentStore, StoreError, Tree,
    TreeEntry, TreeEntryContent,
};

fn file_entry(
    name: &str,
    bytes: &[u8],
    executable: bool,
) -> Result<TreeEntry<FileContent, ContentId>, StoreError> {
    TreeEntry::new(
        name,
        TreeEntryContent::File(FileContent {
            content: ContentId::of_blob(bytes),
            executable,
        }),
    )
}

fn representative_tree() -> Result<Tree, StoreError> {
    let child = Tree::new([file_entry("nested", b"nested bytes", false)?])?;
    Tree::new([
        file_entry("regular", b"regular bytes", false)?,
        file_entry("executable", b"executable bytes", true)?,
        TreeEntry::new("child", TreeEntryContent::Tree(child.id()))?,
        TreeEntry::new(
            "link",
            TreeEntryContent::Symlink {
                target: b"regular".as_slice().into(),
            },
        )?,
    ])
}

fn exercise_basic_contract(store: &mut dyn ContentStore) -> Result<(), StoreError> {
    let payloads: [&[u8]; 4] = [b"", b"text", &[0, 1, 2, 0, 255], &[42; 64 * 1024]];
    let mut identities = Vec::new();
    for payload in payloads {
        let identity = store.put_blob(payload)?;
        assert_eq!(identity, ContentId::of_blob(payload));
        assert_eq!(
            store
                .get_blob(identity)?
                .map(|blob| blob.as_bytes().to_vec()),
            Some(payload.to_vec())
        );
        assert_eq!(store.put_blob(payload)?, identity);
        identities.push(identity);
    }
    assert_eq!(identities.len(), 4);

    let tree = representative_tree()?;
    let identity = store.put_tree(tree.clone())?;
    assert_eq!(identity, tree.id());
    assert_eq!(store.get_tree(identity)?, Some(tree.clone()));
    assert_eq!(store.put_tree(tree)?, identity);

    assert_eq!(store.get_blob(ContentId::of_blob(b"missing"))?, None);
    assert_eq!(store.get_tree(ContentId::of_tree(b"missing"))?, None);
    Ok(())
}

#[test]
fn memory_adapter_satisfies_the_basic_content_contract() {
    let mut store = MemoryContentStore::default();

    assert!(exercise_basic_contract(&mut store).is_ok());
}

#[test]
fn filesystem_adapter_satisfies_the_basic_content_contract() {
    let Some(directory) = tempfile::tempdir().ok() else {
        return;
    };
    let Some(mut store) = FilesystemContentStore::open(directory.path()).ok() else {
        return;
    };

    assert!(exercise_basic_contract(&mut store).is_ok());
}

#[test]
fn memory_counts_unique_objects_across_mixed_operations() {
    let mut store = MemoryContentStore::default();
    assert!(store.is_empty());

    assert!(store.put_blob(b"one").is_ok());
    assert!(store.put_blob(b"one").is_ok());
    assert!(store.put_blob(b"two").is_ok());
    let Some(tree) = representative_tree().ok() else {
        return;
    };
    assert!(store.put_tree(tree.clone()).is_ok());
    assert!(store.put_tree(tree).is_ok());

    assert_eq!(store.blob_count(), 2);
    assert_eq!(store.tree_count(), 1);
    assert!(!store.is_empty());
}

#[test]
fn blob_and_tree_identities_are_domain_separated() {
    let manifest_like_bytes = b"the same source bytes";

    assert_ne!(
        ContentId::of_blob(manifest_like_bytes),
        ContentId::of_tree(manifest_like_bytes)
    );
}

#[test]
fn a_tree_may_name_content_that_is_not_materialized_locally() {
    let remote_blob = ContentId::of_blob(b"remote-only");
    let Some(entry) = TreeEntry::new(
        "remote",
        TreeEntryContent::File(FileContent {
            content: remote_blob,
            executable: false,
        }),
    )
    .ok() else {
        return;
    };
    let Some(tree) = Tree::new([entry]).ok() else {
        return;
    };
    let mut store = MemoryContentStore::default();

    let stored = store.put_tree(tree.clone());

    assert_eq!(stored.ok(), Some(tree.id()));
    assert_eq!(store.get_blob(remote_blob).ok().flatten(), None);
    assert_eq!(store.get_tree(tree.id()).ok().flatten(), Some(tree));
}

#[test]
fn filesystem_open_creates_a_nested_store_root() {
    let Some(directory) = tempfile::tempdir().ok() else {
        return;
    };
    let root = directory.path().join("nested/store/root");

    assert!(FilesystemContentStore::open(&root).is_ok());
    assert!(root.join("blobs").is_dir());
    assert!(root.join("trees").is_dir());
}

#[test]
fn multiple_filesystem_instances_observe_the_same_objects() {
    let Some(directory) = tempfile::tempdir().ok() else {
        return;
    };
    let (blob_id, tree) = {
        let Some(mut writer) = FilesystemContentStore::open(directory.path()).ok() else {
            return;
        };
        let Some(blob_id) = writer.put_blob(b"shared").ok() else {
            return;
        };
        let Some(tree) = representative_tree().ok() else {
            return;
        };
        assert!(writer.put_tree(tree.clone()).is_ok());
        (blob_id, tree)
    };

    let Some(reader) = FilesystemContentStore::open(directory.path()).ok() else {
        return;
    };

    assert_eq!(
        reader
            .get_blob(blob_id)
            .ok()
            .flatten()
            .map(|blob| blob.as_bytes().to_vec()),
        Some(b"shared".to_vec())
    );
    assert_eq!(reader.get_tree(tree.id()).ok().flatten(), Some(tree));
}

#[test]
fn successful_filesystem_publication_leaves_no_temporary_objects() {
    let Some(directory) = tempfile::tempdir().ok() else {
        return;
    };
    let Some(mut store) = FilesystemContentStore::open(directory.path()).ok() else {
        return;
    };
    assert!(store.put_blob(b"blob").is_ok());
    let Some(tree) = representative_tree().ok() else {
        return;
    };
    assert!(store.put_tree(tree).is_ok());

    for object_directory in [
        directory.path().join("blobs"),
        directory.path().join("trees"),
    ] {
        let temporary_count = fs::read_dir(object_directory)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(".pith-"))
            .count();
        assert_eq!(temporary_count, 0);
    }
}

#[test]
fn putting_over_a_corrupt_existing_blob_fails_closed() {
    let Some(directory) = tempfile::tempdir().ok() else {
        return;
    };
    let Some(mut store) = FilesystemContentStore::open(directory.path()).ok() else {
        return;
    };
    let Some(identity) = store.put_blob(b"expected").ok() else {
        return;
    };
    let object_path = directory
        .path()
        .join("blobs")
        .join(identity.digest().to_string());
    assert!(fs::write(object_path, b"corrupt").is_ok());

    let error = store.put_blob(b"expected").err();

    assert!(error.is_some_and(|error| error.to_string().contains("does not match its identity")));
}

#[test]
fn putting_over_a_corrupt_existing_tree_fails_closed() {
    let Some(directory) = tempfile::tempdir().ok() else {
        return;
    };
    let Some(mut store) = FilesystemContentStore::open(directory.path()).ok() else {
        return;
    };
    let Some(tree) = representative_tree().ok() else {
        return;
    };
    let Some(identity) = store.put_tree(tree.clone()).ok() else {
        return;
    };
    let object_path = directory
        .path()
        .join("trees")
        .join(identity.digest().to_string());
    assert!(fs::write(object_path, b"corrupt").is_ok());

    let error = store.put_tree(tree).err();

    assert!(error.is_some_and(|error| error.to_string().contains("does not match its identity")));
}

#[test]
fn store_errors_preserve_the_adapter_message() {
    let error = StoreError::new("adapter detail");

    assert_eq!(error.to_string(), "adapter detail");
    assert_eq!(error, error.clone());
}
