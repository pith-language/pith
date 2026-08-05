use crate::StoreError;
use pith_ids::ContentId;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum TreeEntryContent {
    Blob(ContentId),
    Tree(ContentId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeEntry {
    name: Box<str>,
    content: TreeEntryContent,
}

impl TreeEntry {
    /// # Errors
    /// Returns an error when `name` is not one tree path component.
    pub fn new(name: impl Into<Box<str>>, content: TreeEntryContent) -> Result<Self, StoreError> {
        let name = name.into();
        if name.is_empty()
            || name.as_ref() == "."
            || name.as_ref() == ".."
            || name.contains('/')
            || name.contains('\0')
        {
            return Err(StoreError::new(format!("invalid tree entry name `{name}`")));
        }
        Ok(Self { name, content })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn content(&self) -> TreeEntryContent {
        self.content
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tree {
    id: ContentId,
    entries: Box<[TreeEntry]>,
}

impl Tree {
    /// # Errors
    /// Returns an error for duplicate names or an unrepresentable manifest.
    pub fn new(entries: impl IntoIterator<Item = TreeEntry>) -> Result<Self, StoreError> {
        let mut entries: Vec<_> = entries.into_iter().collect();
        entries.sort_by(|left, right| left.name.cmp(&right.name));

        let mut previous: Option<&str> = None;
        for entry in &entries {
            if previous == Some(entry.name()) {
                return Err(StoreError::new(format!(
                    "duplicate tree entry `{}`",
                    entry.name()
                )));
            }
            previous = Some(entry.name());
        }

        let manifest = canonical_manifest(&entries)?;
        Ok(Self {
            id: ContentId::of_tree(&manifest),
            entries: entries.into_boxed_slice(),
        })
    }

    pub fn id(&self) -> ContentId {
        self.id
    }

    pub fn entries(&self) -> &[TreeEntry] {
        &self.entries
    }
}

fn canonical_manifest(entries: &[TreeEntry]) -> Result<Vec<u8>, StoreError> {
    let count =
        u64::try_from(entries.len()).map_err(|_| StoreError::new("tree has too many entries"))?;
    let mut manifest = Vec::new();
    manifest.extend_from_slice(&count.to_le_bytes());

    for entry in entries {
        let name = entry.name().as_bytes();
        let name_len = u64::try_from(name.len())
            .map_err(|_| StoreError::new("tree entry name is too long"))?;
        manifest.extend_from_slice(&name_len.to_le_bytes());
        manifest.extend_from_slice(name);
        match entry.content {
            TreeEntryContent::Blob(id) => {
                manifest.push(0);
                manifest.extend_from_slice(id.digest().as_bytes());
            }
            TreeEntryContent::Tree(id) => {
                manifest.push(1);
                manifest.extend_from_slice(id.digest().as_bytes());
            }
        }
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blob_entry(name: &str, bytes: &[u8]) -> TreeEntry {
        TreeEntry::new(name, TreeEntryContent::Blob(ContentId::of_blob(bytes))).unwrap()
    }

    #[test]
    fn identity_does_not_depend_on_entry_order() {
        let first = Tree::new([blob_entry("b", b"second"), blob_entry("a", b"first")]).unwrap();
        let second = Tree::new([blob_entry("a", b"first"), blob_entry("b", b"second")]).unwrap();

        assert_eq!(first.id(), second.id());
        assert_eq!(first.entries().first().unwrap().name(), "a");
    }

    #[test]
    fn duplicate_names_are_rejected() {
        let err = Tree::new([blob_entry("same", b"a"), blob_entry("same", b"b")]).unwrap_err();

        assert!(err.to_string().contains("duplicate tree entry"));
    }

    #[test]
    fn names_are_single_components() {
        for name in ["", ".", "..", "a/b", "nul\0name"] {
            assert!(
                TreeEntry::new(name, TreeEntryContent::Blob(ContentId::of_blob(b"x"))).is_err()
            );
        }
    }

    #[test]
    fn blob_and_tree_entries_have_distinct_identities() {
        let content = ContentId::of_blob(b"same target");
        let blob =
            Tree::new([TreeEntry::new("item", TreeEntryContent::Blob(content)).unwrap()]).unwrap();
        let tree =
            Tree::new([TreeEntry::new("item", TreeEntryContent::Tree(content)).unwrap()]).unwrap();

        assert_ne!(blob.id(), tree.id());
    }
}
