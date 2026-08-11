use super::*;
use pith_ids::DIGEST_LEN;

fn file_entry_full(
    name: &str,
    bytes: &[u8],
    executable: bool,
) -> TreeEntry<FileContent, ContentId> {
    TreeEntry::new(
        name,
        TreeEntryContent::File(FileContent {
            content: ContentId::of_blob(bytes),
            executable,
        }),
    )
    .unwrap()
}

#[test]
fn an_empty_tree_round_trips_through_its_manifest() {
    let tree = Tree::new([]).unwrap();
    let manifest = tree.manifest().unwrap();
    let decoded = Tree::from_manifest(&manifest).unwrap();

    assert_eq!(decoded.id(), tree.id());
    assert!(decoded.entries().is_empty());
}

#[test]
fn a_tree_with_every_entry_kind_round_trips_through_its_manifest() {
    let child = Tree::new([file_entry_full("nested", b"nested", false)]).unwrap();
    let tree = Tree::new([
        file_entry_full("regular", b"regular", false),
        file_entry_full("executable", b"executable", true),
        TreeEntry::new("directory", TreeEntryContent::Tree(child.id())).unwrap(),
        TreeEntry::new(
            "link",
            TreeEntryContent::Symlink {
                target: b"regular".as_slice().into(),
            },
        )
        .unwrap(),
    ])
    .unwrap();

    let manifest = tree.manifest().unwrap();
    let decoded = Tree::from_manifest(&manifest).unwrap();

    assert_eq!(decoded.id(), tree.id());
    assert_eq!(decoded.entries(), tree.entries());
}

#[test]
fn from_manifest_rejects_an_empty_manifest() {
    assert!(Tree::from_manifest(&[]).is_err());
}

#[test]
fn from_manifest_rejects_a_truncated_entry_count() {
    assert!(Tree::from_manifest(&[0x01, 0x02, 0x03]).is_err());
}

#[test]
fn from_manifest_rejects_a_truncated_entry_name_length() {
    let mut manifest = Vec::new();
    manifest.extend_from_slice(&1u64.to_le_bytes());
    manifest.extend_from_slice(&3u64.to_le_bytes());
    assert!(Tree::from_manifest(&manifest).is_err());
}

#[test]
fn from_manifest_rejects_an_unknown_entry_tag() {
    let mut manifest = Vec::new();
    manifest.extend_from_slice(&1u64.to_le_bytes());
    manifest.extend_from_slice(&1u64.to_le_bytes());
    manifest.push(b'a');
    manifest.push(0xff);
    assert!(Tree::from_manifest(&manifest).is_err());
}

#[test]
fn from_manifest_rejects_a_truncated_content_digest() {
    let mut manifest = Vec::new();
    manifest.extend_from_slice(&1u64.to_le_bytes());
    manifest.extend_from_slice(&1u64.to_le_bytes());
    manifest.push(b'a');
    manifest.push(TAG_FILE);
    manifest.extend_from_slice(&[0u8; DIGEST_LEN - 1]);
    assert!(Tree::from_manifest(&manifest).is_err());
}

#[test]
fn from_manifest_rejects_a_truncated_symlink_target() {
    let mut manifest = Vec::new();
    manifest.extend_from_slice(&1u64.to_le_bytes());
    manifest.extend_from_slice(&1u64.to_le_bytes());
    manifest.push(b'a');
    manifest.push(TAG_SYMLINK);
    manifest.extend_from_slice(&5u64.to_le_bytes());
    manifest.extend_from_slice(b"ab");
    assert!(Tree::from_manifest(&manifest).is_err());
}

#[test]
fn from_manifest_rejects_trailing_bytes() {
    let tree = Tree::new([file_entry_full("a", b"x", false)]).unwrap();
    let mut manifest = tree.manifest().unwrap();
    manifest.push(0xff);
    assert!(Tree::from_manifest(&manifest).is_err());
}

#[test]
fn from_manifest_rejects_a_non_utf8_entry_name() {
    let mut manifest = Vec::new();
    manifest.extend_from_slice(&1u64.to_le_bytes());
    manifest.extend_from_slice(&1u64.to_le_bytes());
    manifest.push(0xff);
    manifest.push(TAG_FILE);
    manifest.extend_from_slice(&[0u8; DIGEST_LEN]);
    assert!(Tree::from_manifest(&manifest).is_err());
}

#[test]
fn from_manifest_rejects_an_invalid_entry_name() {
    let mut manifest = Vec::new();
    manifest.extend_from_slice(&1u64.to_le_bytes());
    manifest.extend_from_slice(&3u64.to_le_bytes());
    manifest.extend_from_slice(b"a/b");
    manifest.push(TAG_FILE);
    manifest.extend_from_slice(&[0u8; DIGEST_LEN]);
    assert!(Tree::from_manifest(&manifest).is_err());
}

#[test]
fn from_manifest_rejects_an_invalid_symlink_target() {
    let mut manifest = Vec::new();
    manifest.extend_from_slice(&1u64.to_le_bytes());
    manifest.extend_from_slice(&4u64.to_le_bytes());
    manifest.extend_from_slice(b"link");
    manifest.push(TAG_SYMLINK);
    manifest.extend_from_slice(&4u64.to_le_bytes());
    manifest.extend_from_slice(b"has\0nul");
    assert!(Tree::from_manifest(&manifest).is_err());
}

#[test]
fn from_manifest_rejects_a_non_canonical_entry_ordering() {
    let b = file_entry_full("b", b"second", false);
    let a = file_entry_full("a", b"first", false);
    let mut manifest = Vec::new();
    manifest.extend_from_slice(&2u64.to_le_bytes());
    for entry in [b, a] {
        manifest.extend_from_slice(&(entry.name().len() as u64).to_le_bytes());
        manifest.extend_from_slice(entry.name().as_bytes());
        match &entry.content {
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
            TreeEntryContent::Tree(_) | TreeEntryContent::Symlink { .. } => unreachable!(),
        }
    }
    assert!(Tree::from_manifest(&manifest).is_err());
}

#[test]
fn from_manifest_rejects_duplicate_entry_names() {
    let id = ContentId::of_blob(b"x");
    let mut manifest = Vec::new();
    manifest.extend_from_slice(&2u64.to_le_bytes());
    for _ in 0..2 {
        manifest.extend_from_slice(&4u64.to_le_bytes());
        manifest.extend_from_slice(b"same");
        manifest.push(TAG_FILE);
        manifest.extend_from_slice(id.digest().as_bytes());
    }
    assert!(Tree::from_manifest(&manifest).is_err());
}
