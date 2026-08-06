use crate::{Blob, ContentStore, StoreError, Tree};
use indexmap::IndexMap;
use pith_ids::ContentId;

#[derive(Default)]
pub struct MemoryContentStore {
    blobs: IndexMap<ContentId, Blob>,
    trees: IndexMap<ContentId, Tree>,
}

impl MemoryContentStore {
    pub fn blob_count(&self) -> usize {
        self.blobs.len()
    }

    pub fn tree_count(&self) -> usize {
        self.trees.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blobs.is_empty() && self.trees.is_empty()
    }
}

impl ContentStore for MemoryContentStore {
    fn put_blob(&mut self, bytes: &[u8]) -> Result<ContentId, StoreError> {
        let blob = Blob::new(bytes);
        let id = blob.id();
        self.blobs.entry(id).or_insert(blob);
        Ok(id)
    }

    fn get_blob(&self, id: ContentId) -> Result<Option<Blob>, StoreError> {
        Ok(self.blobs.get(&id).cloned())
    }

    fn put_tree(&mut self, tree: Tree) -> Result<ContentId, StoreError> {
        let id = tree.id();
        self.trees.entry(id).or_insert(tree);
        Ok(id)
    }

    fn get_tree(&self, id: ContentId) -> Result<Option<Tree>, StoreError> {
        Ok(self.trees.get(&id).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileContent, TreeEntry, TreeEntryContent};

    #[test]
    fn equal_blobs_are_stored_once() {
        let mut store = MemoryContentStore::default();

        let first = store.put_blob(b"same").unwrap();
        let second = store.put_blob(b"same").unwrap();

        assert_eq!(first, second);
        assert_eq!(store.blob_count(), 1);
    }

    #[test]
    fn content_can_be_named_before_it_is_available_locally() {
        let store = MemoryContentStore::default();
        let remote = ContentId::of_blob(b"remote");
        let id = ContentId::from_digest(remote.digest());

        assert!(store.get_blob(id).unwrap().is_none());
    }

    #[test]
    fn stored_blob_round_trips() {
        let mut store = MemoryContentStore::default();
        let id = store.put_blob(b"payload").unwrap();

        let blob = store.get_blob(id).unwrap().unwrap();

        assert_eq!(blob.id(), id);
        assert_eq!(blob.as_bytes(), b"payload");
    }

    #[test]
    fn equal_trees_are_stored_once() {
        let mut store = MemoryContentStore::default();
        let blob = store.put_blob(b"payload").unwrap();
        let entry = TreeEntry::new(
            "file",
            TreeEntryContent::File(FileContent {
                content: blob,
                executable: false,
            }),
        )
        .unwrap();
        let tree = Tree::new([entry]).unwrap();

        let first = store.put_tree(tree.clone()).unwrap();
        let second = store.put_tree(tree).unwrap();

        assert_eq!(first, second);
        assert_eq!(store.tree_count(), 1);
        assert_eq!(store.get_tree(first).unwrap().unwrap().id(), first);
    }
}
