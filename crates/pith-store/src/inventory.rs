//! Inventory of immutable objects held by a filesystem content store.

use std::fs;
use std::path::Path;
use std::str::FromStr;

use pith_ids::ContentId;

use crate::StoreError;

/// One object the store holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InventoryEntry {
    pub id: ContentId,
    pub kind: InventoryKind,
    /// The object file's size on disk, which is the number a budget or a
    /// reclaim estimate adds up.
    pub size: u64,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum InventoryKind {
    Blob,
    Tree,
}

/// Walk one object directory in digest order. In-progress store writes are
/// ignored; every other entry must be a regular file named by its digest.
///
/// # Errors
/// Returns [`StoreError`] when a directory cannot be read, a name is not a
/// digest, or an object's size cannot be observed.
pub(super) fn directory_inventory(
    directory: &Path,
    kind: InventoryKind,
) -> Result<Vec<InventoryEntry>, StoreError> {
    let mut entries = Vec::new();
    let objects = match fs::read_dir(directory) {
        Ok(objects) => objects,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(entries),
        Err(error) => return Err(io_error("read directory", error)),
    };
    for object in objects {
        let object = object.map_err(|error| io_error("read directory entry", error))?;
        let name = object.file_name();
        let Some(name) = name.to_str() else {
            return Err(foreign(&name.to_string_lossy()));
        };
        if is_temporary_name(name) {
            continue;
        }
        let id = ContentId::from_str(name).map_err(|_| foreign(name))?;
        let file_type = object
            .file_type()
            .map_err(|error| io_error("read object type", error))?;
        if !file_type.is_file() {
            return Err(StoreError::new(format!(
                "the content object `{name}` is not a regular file"
            )));
        }
        let size = object
            .metadata()
            .map_err(|error| io_error("read object size", error))?
            .len();
        entries.push(InventoryEntry { id, kind, size });
    }
    entries.sort_by_key(|entry| entry.id.digest());
    Ok(entries)
}

fn is_temporary_name(name: &str) -> bool {
    let Some(body) = name
        .strip_prefix(".pith-")
        .and_then(|name| name.strip_suffix(".tmp"))
    else {
        return false;
    };
    let Some((process, sequence)) = body.split_once('.') else {
        return false;
    };
    !process.is_empty()
        && !sequence.is_empty()
        && process.bytes().all(|byte| byte.is_ascii_digit())
        && sequence.bytes().all(|byte| byte.is_ascii_digit())
}

fn foreign(name: &str) -> StoreError {
    StoreError::new(format!(
        "the content store holds `{name}`, which is not a content identity"
    ))
}

fn io_error(operation: &str, error: std::io::Error) -> StoreError {
    StoreError::new(format!("{operation}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContentStore, FilesystemContentStore, Tree, TreeEntry, TreeEntryContent};
    use std::path::PathBuf;

    fn scratch(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "pith-store-inventory-{}-{label}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap_or_else(|error| unreachable!("scratch: {error}"));
        path
    }

    #[test]
    fn the_walk_names_every_object_with_its_kind_and_size() {
        let root = scratch("walk");
        let tree = Tree::new([TreeEntry::new(
            "payload",
            TreeEntryContent::File(crate::FileContent {
                content: ContentId::of_blob(b"payload"),
                executable: false,
            }),
        )
        .unwrap_or_else(|error| unreachable!("entry: {error}"))])
        .unwrap_or_else(|error| unreachable!("tree: {error}"));
        let mut store = FilesystemContentStore::open(&root)
            .unwrap_or_else(|error| unreachable!("open: {error}"));
        let blob = store
            .put_blob(b"one blob")
            .unwrap_or_else(|error| unreachable!("blob: {error}"));
        let manifest = store
            .put_tree(tree)
            .unwrap_or_else(|error| unreachable!("tree: {error}"));

        let inventory = store
            .inventory()
            .unwrap_or_else(|error| unreachable!("inventory: {error}"));

        let named: Vec<(ContentId, InventoryKind)> = inventory
            .iter()
            .map(|entry| (entry.id, entry.kind))
            .collect();
        let blob_size = inventory
            .iter()
            .find(|entry| entry.kind == InventoryKind::Blob)
            .map(|entry| entry.size)
            .unwrap_or_default();
        let mut digest_ordered = named.clone();
        digest_ordered.sort_by_key(|(id, _)| id.digest());
        assert_eq!(named, digest_ordered, "the walk is in digest order");
        assert!(
            named.contains(&(blob, InventoryKind::Blob)),
            "the blob is absent: {named:?}"
        );
        assert!(
            named.contains(&(manifest, InventoryKind::Tree)),
            "the tree is absent: {named:?}"
        );
        assert_eq!(
            blob_size,
            u64::try_from("one blob".len()).unwrap_or_default()
        );
    }

    #[test]
    fn temporary_files_are_not_content() {
        let root = scratch("temporary");
        let mut store = FilesystemContentStore::open(&root)
            .unwrap_or_else(|error| unreachable!("open: {error}"));
        let _ = store
            .put_blob(b"kept")
            .unwrap_or_else(|error| unreachable!("blob: {error}"));
        let blobs = root.join("blobs");
        fs::write(blobs.join(".pith-0.0.tmp"), b"in flight")
            .unwrap_or_else(|error| unreachable!("temporary: {error}"));

        let inventory = store
            .inventory()
            .unwrap_or_else(|error| unreachable!("inventory: {error}"));

        assert_eq!(
            inventory.len(),
            1,
            "a write in flight was counted: {inventory:?}"
        );
    }

    #[test]
    fn a_name_that_is_not_a_digest_refuses_the_walk() {
        let root = scratch("foreign");
        let store = FilesystemContentStore::open(&root)
            .unwrap_or_else(|error| unreachable!("open: {error}"));
        fs::write(root.join("blobs").join("not-a-digest"), b"foreign")
            .unwrap_or_else(|error| unreachable!("foreign: {error}"));

        let refused = store.inventory();

        assert!(
            refused
                .as_ref()
                .is_err_and(|error| error.to_string().contains("not a content identity")),
            "{refused:?}"
        );
    }

    #[test]
    fn an_unrelated_hidden_file_refuses_the_walk() {
        let root = scratch("hidden");
        let store = FilesystemContentStore::open(&root)
            .unwrap_or_else(|error| unreachable!("open: {error}"));
        fs::write(root.join("blobs").join(".foreign"), b"foreign")
            .unwrap_or_else(|error| unreachable!("foreign: {error}"));

        assert!(store.inventory().is_err());
    }

    #[test]
    fn a_digest_named_directory_is_not_an_object() {
        let root = scratch("directory");
        let store = FilesystemContentStore::open(&root)
            .unwrap_or_else(|error| unreachable!("open: {error}"));
        let digest = ContentId::of_blob(b"directory").digest().to_string();
        fs::create_dir(root.join("blobs").join(digest))
            .unwrap_or_else(|error| unreachable!("directory: {error}"));

        let refused = store.inventory();
        assert!(
            refused
                .as_ref()
                .is_err_and(|error| error.to_string().contains("not a regular file")),
            "{refused:?}"
        );
    }
}
