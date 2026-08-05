use crate::StoreError;
use pith_ids::{ContentDigest, ContentId, DIGEST_LEN};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TreeEntryContent {
    File {
        content: ContentId,
        executable: bool,
    },
    Tree(ContentId),
    Symlink {
        target: Box<[u8]>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeEntry {
    name: Box<str>,
    content: TreeEntryContent,
}

impl TreeEntry {
    /// # Errors
    /// Returns an error when `name` is not one tree path component or when a
    /// symlink target is empty or contains a NUL byte.
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
        if let TreeEntryContent::Symlink { target } = &content
            && (target.is_empty() || target.contains(&0))
        {
            return Err(StoreError::new(format!(
                "invalid symlink target for tree entry `{name}`"
            )));
        }
        Ok(Self { name, content })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn content(&self) -> &TreeEntryContent {
        &self.content
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

    pub(crate) fn manifest(&self) -> Result<Vec<u8>, StoreError> {
        canonical_manifest(&self.entries)
    }

    pub(crate) fn from_manifest(manifest: &[u8]) -> Result<Self, StoreError> {
        let mut reader = ManifestReader::new(manifest);
        let entry_count = reader.read_length()?;
        let mut entries = Vec::new();
        for _ in 0..entry_count {
            let name = reader.read_text()?;
            let tag = reader.read_byte()?;
            let content = match tag {
                0 => TreeEntryContent::File {
                    content: reader.read_content_id()?,
                    executable: false,
                },
                1 => TreeEntryContent::File {
                    content: reader.read_content_id()?,
                    executable: true,
                },
                2 => TreeEntryContent::Tree(reader.read_content_id()?),
                3 => TreeEntryContent::Symlink {
                    target: reader.read_bytes()?.into(),
                },
                _ => return Err(StoreError::new("tree manifest has an unknown entry tag")),
            };
            entries.push(TreeEntry::new(name, content)?);
        }
        if !reader.is_empty() {
            return Err(StoreError::new("tree manifest has trailing bytes"));
        }
        let tree = Self::new(entries)?;
        if tree.manifest()?.as_slice() != manifest {
            return Err(StoreError::new("tree manifest is not canonical"));
        }
        Ok(tree)
    }
}

struct ManifestReader<'manifest> {
    remaining: &'manifest [u8],
}

impl<'manifest> ManifestReader<'manifest> {
    fn new(manifest: &'manifest [u8]) -> Self {
        Self {
            remaining: manifest,
        }
    }

    fn read_length(&mut self) -> Result<usize, StoreError> {
        let bytes = self.take(size_of::<u64>())?;
        let mut encoded = [0; size_of::<u64>()];
        encoded.copy_from_slice(bytes);
        usize::try_from(u64::from_le_bytes(encoded))
            .map_err(|_| StoreError::new("tree manifest length is not representable"))
    }

    fn read_byte(&mut self) -> Result<u8, StoreError> {
        let Some((byte, remaining)) = self.remaining.split_first() else {
            return Err(StoreError::new("tree manifest ended unexpectedly"));
        };
        self.remaining = remaining;
        Ok(*byte)
    }

    fn read_bytes(&mut self) -> Result<&'manifest [u8], StoreError> {
        let length = self.read_length()?;
        self.take(length)
    }

    fn read_text(&mut self) -> Result<&'manifest str, StoreError> {
        std::str::from_utf8(self.read_bytes()?)
            .map_err(|_| StoreError::new("tree manifest entry name is not utf-8"))
    }

    fn read_content_id(&mut self) -> Result<ContentId, StoreError> {
        let bytes = self.take(DIGEST_LEN)?;
        let mut digest = [0u8; DIGEST_LEN];
        digest.copy_from_slice(bytes);
        Ok(ContentId::from_digest(ContentDigest::from_bytes(digest)))
    }

    fn take(&mut self, length: usize) -> Result<&'manifest [u8], StoreError> {
        if self.remaining.len() < length {
            return Err(StoreError::new("tree manifest ended unexpectedly"));
        }
        let (value, remaining) = self.remaining.split_at(length);
        self.remaining = remaining;
        Ok(value)
    }

    fn is_empty(&self) -> bool {
        self.remaining.is_empty()
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
        match &entry.content {
            TreeEntryContent::Tree(id) => {
                manifest.push(2);
                manifest.extend_from_slice(id.digest().as_bytes());
            }
            TreeEntryContent::File {
                content,
                executable,
            } => {
                manifest.push(if *executable { 1 } else { 0 });
                manifest.extend_from_slice(content.digest().as_bytes());
            }
            TreeEntryContent::Symlink { target } => {
                manifest.push(3);
                let target_len = u64::try_from(target.len())
                    .map_err(|_| StoreError::new("symlink target is too long"))?;
                manifest.extend_from_slice(&target_len.to_le_bytes());
                manifest.extend_from_slice(target);
            }
        }
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blob_entry(name: &str, bytes: &[u8]) -> TreeEntry {
        TreeEntry::new(
            name,
            TreeEntryContent::File {
                content: ContentId::of_blob(bytes),
                executable: false,
            },
        )
        .unwrap()
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
                TreeEntry::new(
                    name,
                    TreeEntryContent::File {
                        content: ContentId::of_blob(b"x"),
                        executable: false,
                    },
                )
                .is_err()
            );
        }
    }

    #[test]
    fn file_and_tree_entries_have_distinct_identities() {
        let content = ContentId::of_blob(b"same target");
        let file = Tree::new([TreeEntry::new(
            "item",
            TreeEntryContent::File {
                content,
                executable: false,
            },
        )
        .unwrap()])
        .unwrap();
        let tree =
            Tree::new([TreeEntry::new("item", TreeEntryContent::Tree(content)).unwrap()]).unwrap();

        assert_ne!(file.id(), tree.id());
    }

    #[test]
    fn executable_file_identity_is_distinct_from_regular_file() {
        let content = ContentId::of_blob(b"program");
        let regular = Tree::new([TreeEntry::new(
            "tool",
            TreeEntryContent::File {
                content,
                executable: false,
            },
        )
        .unwrap()])
        .unwrap();
        let executable = Tree::new([TreeEntry::new(
            "tool",
            TreeEntryContent::File {
                content,
                executable: true,
            },
        )
        .unwrap()])
        .unwrap();

        assert_ne!(regular.id(), executable.id());
    }

    #[test]
    fn symlink_identity_uses_the_target_bytes() {
        let first = Tree::new([TreeEntry::new(
            "link",
            TreeEntryContent::Symlink {
                target: b"target".as_slice().into(),
            },
        )
        .unwrap()])
        .unwrap();
        let second = Tree::new([TreeEntry::new(
            "link",
            TreeEntryContent::Symlink {
                target: b"other".as_slice().into(),
            },
        )
        .unwrap()])
        .unwrap();

        assert_ne!(first.id(), second.id());
    }

    #[test]
    fn invalid_symlink_targets_are_rejected() {
        for target in [b"".as_slice(), b"has\0nul".as_slice()] {
            assert!(
                TreeEntry::new(
                    "link",
                    TreeEntryContent::Symlink {
                        target: target.into(),
                    },
                )
                .is_err()
            );
        }
    }
}
