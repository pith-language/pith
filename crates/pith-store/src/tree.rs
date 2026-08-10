use crate::StoreError;
use pith_ids::{ContentDigest, ContentId, DIGEST_LEN};

/// Entry-kind tags in the canonical tree manifest. These are a stable
/// content-addressing wire format: changing a value changes every tree's
/// identity. The writer (`canonical_manifest`) and reader (`from_manifest`)
/// both reference these constants so the two halves cannot drift.
const TAG_FILE: u8 = 0;
const TAG_EXECUTABLE_FILE: u8 = 1;
const TAG_TREE: u8 = 2;
const TAG_SYMLINK: u8 = 3;

/// A tree entry's content, generic over the file payload `F` and the recursive
/// tree payload `T`. The three variants — `File`, `Tree`, `Symlink` — are the
/// single source of truth for the entry shape; the canonical store form, the
/// engine's materialized form, and the engine's captured form are all
/// instantiations of this enum, differing only in what `File` carries and what
/// `Tree` recurses on. `Symlink` is identical across all phases.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TreeEntryContent<F, T> {
    File(F),
    Tree(T),
    Symlink { target: Box<[u8]> },
}

/// Canonical file payload stored by the content store: a content identity plus
/// the executability bit that tree identity must preserve.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileContent {
    pub content: ContentId,
    pub executable: bool,
}

/// The canonical store instantiation: files carry [`FileContent`], subtrees
/// carry their content identity. Used by tests that need to name the full type
/// at a construction site where inference alone cannot.
#[cfg(test)]
pub(crate) type StoreTreeEntryContent = TreeEntryContent<FileContent, ContentId>;

/// One named entry in a tree, generic over the same parameters as its content.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TreeEntry<F, T> {
    name: Box<str>,
    content: TreeEntryContent<F, T>,
}

impl<F, T> TreeEntry<F, T> {
    /// # Errors
    /// Returns an error when `name` is not one tree path component or when a
    /// symlink target is empty or contains a NUL byte.
    pub fn new(
        name: impl Into<Box<str>>,
        content: TreeEntryContent<F, T>,
    ) -> Result<Self, StoreError> {
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

    pub fn content(&self) -> &TreeEntryContent<F, T> {
        &self.content
    }
}

/// A content-addressed directory tree. Concrete over the canonical store form
/// (`FileContent` files, `ContentId` subtrees); the engine's materialized and
/// captured tree representations reuse the generic [`TreeEntryContent`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tree {
    id: ContentId,
    entries: Box<[TreeEntry<FileContent, ContentId>]>,
}

impl Tree {
    /// # Errors
    /// Returns an error for duplicate names or an unrepresentable manifest.
    pub fn new(
        entries: impl IntoIterator<Item = TreeEntry<FileContent, ContentId>>,
    ) -> Result<Self, StoreError> {
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

    pub fn entries(&self) -> &[TreeEntry<FileContent, ContentId>] {
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
                TAG_FILE => TreeEntryContent::File(FileContent {
                    content: reader.read_content_id()?,
                    executable: false,
                }),
                TAG_EXECUTABLE_FILE => TreeEntryContent::File(FileContent {
                    content: reader.read_content_id()?,
                    executable: true,
                }),
                TAG_TREE => TreeEntryContent::Tree(reader.read_content_id()?),
                TAG_SYMLINK => TreeEntryContent::Symlink {
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

fn canonical_manifest(
    entries: &[TreeEntry<FileContent, ContentId>],
) -> Result<Vec<u8>, StoreError> {
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
                manifest.push(TAG_TREE);
                manifest.extend_from_slice(id.digest().as_bytes());
            }
            TreeEntryContent::File(FileContent {
                content,
                executable,
            }) => {
                manifest.push(if *executable {
                    TAG_EXECUTABLE_FILE
                } else {
                    TAG_FILE
                });
                manifest.extend_from_slice(content.digest().as_bytes());
            }
            TreeEntryContent::Symlink { target } => {
                manifest.push(TAG_SYMLINK);
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

    #[test]
    fn entry_kind_tags_are_a_stable_wire_format() {
        // These bytes are part of content identity: changing a value changes
        // every tree's ContentId. Pin them so a renumber is a deliberate,
        // visible test failure rather than a silent identity shift.
        assert_eq!(TAG_FILE, 0);
        assert_eq!(TAG_EXECUTABLE_FILE, 1);
        assert_eq!(TAG_TREE, 2);
        assert_eq!(TAG_SYMLINK, 3);
    }

    fn blob_entry(name: &str, bytes: &[u8]) -> TreeEntry<FileContent, ContentId> {
        TreeEntry::new(
            name,
            TreeEntryContent::File(FileContent {
                content: ContentId::of_blob(bytes),
                executable: false,
            }),
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
                    StoreTreeEntryContent::File(FileContent {
                        content: ContentId::of_blob(b"x"),
                        executable: false,
                    }),
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
            TreeEntryContent::File(FileContent {
                content,
                executable: false,
            }),
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
            TreeEntryContent::File(FileContent {
                content,
                executable: false,
            }),
        )
        .unwrap()])
        .unwrap();
        let executable = Tree::new([TreeEntry::new(
            "tool",
            TreeEntryContent::File(FileContent {
                content,
                executable: true,
            }),
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
                TreeEntry::<FileContent, ContentId>::new(
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

#[cfg(test)]
#[path = "tree_manifest_tests.rs"]
mod tree_manifest_tests;

#[cfg(test)]
#[path = "tree_fuzz.rs"]
mod tree_fuzz;
