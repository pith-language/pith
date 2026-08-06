use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use pith_ids::ContentId;

use crate::{Blob, ContentStore, StoreError, Tree};

pub struct FilesystemContentStore {
    blob_directory: PathBuf,
    tree_directory: PathBuf,
    next_temporary_file: u64,
}

impl FilesystemContentStore {
    /// # Errors
    /// Returns an adapter error when the content directories cannot be created.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let blob_directory = root.as_ref().join("blobs");
        let tree_directory = root.as_ref().join("trees");
        fs::create_dir_all(&blob_directory)
            .map_err(|error| io_error("create blob store directory", error))?;
        fs::create_dir_all(&tree_directory)
            .map_err(|error| io_error("create tree store directory", error))?;
        Ok(Self {
            blob_directory,
            tree_directory,
            next_temporary_file: 0,
        })
    }

    fn blob_path(&self, identity: ContentId) -> PathBuf {
        self.blob_directory.join(identity.digest().to_string())
    }

    fn tree_path(&self, identity: ContentId) -> PathBuf {
        self.tree_directory.join(identity.digest().to_string())
    }

    fn write_object(
        &mut self,
        directory: &Path,
        destination: &Path,
        bytes: &[u8],
    ) -> Result<(), StoreError> {
        if let Some(existing) = read_optional(destination)? {
            return verify_existing(destination, &existing, bytes);
        }

        let (temporary_path, mut temporary_file) = self.create_temporary_file(directory)?;
        if let Err(error) = temporary_file.write_all(bytes) {
            drop(temporary_file);
            remove_temporary_file(&temporary_path);
            return Err(io_error("write temporary content object", error));
        }
        if let Err(error) = temporary_file.sync_all() {
            drop(temporary_file);
            remove_temporary_file(&temporary_path);
            return Err(io_error("flush temporary content object", error));
        }
        drop(temporary_file);

        match fs::rename(&temporary_path, destination) {
            Ok(()) => {
                File::open(directory)
                    .and_then(|file| file.sync_all())
                    .map_err(|error| io_error("flush content store directory", error))?;
                Ok(())
            }
            Err(rename_error) => {
                remove_temporary_file(&temporary_path);
                match read_optional(destination)? {
                    Some(existing) => verify_existing(destination, &existing, bytes),
                    None => Err(io_error("publish content object", rename_error)),
                }
            }
        }
    }

    fn create_temporary_file(&mut self, directory: &Path) -> Result<(PathBuf, File), StoreError> {
        loop {
            let sequence = self.next_temporary_file;
            self.next_temporary_file = self
                .next_temporary_file
                .checked_add(1)
                .ok_or_else(|| StoreError::new("content store temporary sequence exhausted"))?;
            let path = directory.join(format!(".pith-{}.{}.tmp", std::process::id(), sequence));
            match File::create_new(&path) {
                Ok(file) => return Ok((path, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(io_error("create temporary content object", error)),
            }
        }
    }
}

impl ContentStore for FilesystemContentStore {
    fn put_blob(&mut self, bytes: &[u8]) -> Result<ContentId, StoreError> {
        let blob = Blob::new(bytes);
        let identity = blob.id();
        let destination = self.blob_path(identity);
        let directory = self.blob_directory.clone();
        self.write_object(&directory, &destination, bytes)?;
        Ok(identity)
    }

    fn get_blob(&self, identity: ContentId) -> Result<Option<Blob>, StoreError> {
        let path = self.blob_path(identity);
        let Some(bytes) = read_optional(&path)? else {
            return Ok(None);
        };
        let blob = Blob::new(bytes);
        if blob.id() != identity {
            return Err(corrupt_object(&path));
        }
        Ok(Some(blob))
    }

    fn put_tree(&mut self, tree: Tree) -> Result<ContentId, StoreError> {
        let identity = tree.id();
        let manifest = tree.manifest()?;
        let destination = self.tree_path(identity);
        let directory = self.tree_directory.clone();
        self.write_object(&directory, &destination, &manifest)?;
        Ok(identity)
    }

    fn get_tree(&self, identity: ContentId) -> Result<Option<Tree>, StoreError> {
        let path = self.tree_path(identity);
        let Some(manifest) = read_optional(&path)? else {
            return Ok(None);
        };
        let tree = Tree::from_manifest(&manifest)?;
        if tree.id() != identity {
            return Err(corrupt_object(&path));
        }
        Ok(Some(tree))
    }
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, StoreError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error("read content object", error)),
    }
}

fn verify_existing(path: &Path, existing: &[u8], expected: &[u8]) -> Result<(), StoreError> {
    if existing == expected {
        Ok(())
    } else {
        Err(corrupt_object(path))
    }
}

fn remove_temporary_file(path: &Path) {
    let _ = fs::remove_file(path);
}

fn corrupt_object(path: &Path) -> StoreError {
    StoreError::new(format!(
        "content object '{}' does not match its identity",
        path.display()
    ))
}

fn io_error(operation: &str, error: io::Error) -> StoreError {
    StoreError::new(format!("{operation}: {error}"))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::{FileContent, TreeEntry, TreeEntryContent};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TemporaryDirectory {
        path: PathBuf,
    }

    impl TemporaryDirectory {
        fn new() -> io::Result<Self> {
            loop {
                let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!(
                    "pith-store-test-{}-{}",
                    std::process::id(),
                    sequence
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Ok(Self { path }),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error),
                }
            }
        }
    }

    impl Drop for TemporaryDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn blob_survives_store_reopen() {
        let directory = TemporaryDirectory::new().unwrap();
        let identity = {
            let mut store = FilesystemContentStore::open(&directory.path).unwrap();
            store.put_blob(b"persistent blob").unwrap()
        };

        let mut store = FilesystemContentStore::open(&directory.path).unwrap();
        assert_eq!(store.put_blob(b"persistent blob").unwrap(), identity);
        let blob = store.get_blob(identity).unwrap().unwrap();

        assert_eq!(blob.as_bytes(), b"persistent blob");
    }

    #[test]
    fn tree_manifest_survives_store_reopen() {
        let directory = TemporaryDirectory::new().unwrap();
        let regular = ContentId::of_blob(b"regular");
        let executable = ContentId::of_blob(b"executable");
        let child = Tree::new([TreeEntry::new(
            "child-file",
            TreeEntryContent::File(FileContent {
                content: regular,
                executable: false,
            }),
        )
        .unwrap()])
        .unwrap();
        let tree = Tree::new([
            TreeEntry::new(
                "regular",
                TreeEntryContent::File(FileContent {
                    content: regular,
                    executable: false,
                }),
            )
            .unwrap(),
            TreeEntry::new(
                "executable",
                TreeEntryContent::File(FileContent {
                    content: executable,
                    executable: true,
                }),
            )
            .unwrap(),
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
        let identity = tree.id();
        {
            let mut store = FilesystemContentStore::open(&directory.path).unwrap();
            assert_eq!(store.put_tree(tree.clone()).unwrap(), identity);
        }

        let store = FilesystemContentStore::open(&directory.path).unwrap();

        assert_eq!(store.get_tree(identity).unwrap(), Some(tree));
    }

    #[test]
    fn corrupt_blob_is_rejected() {
        let directory = TemporaryDirectory::new().unwrap();
        let mut store = FilesystemContentStore::open(&directory.path).unwrap();
        let identity = store.put_blob(b"expected").unwrap();
        fs::write(store.blob_path(identity), b"corrupt").unwrap();

        let error = store.get_blob(identity).unwrap_err();

        assert!(error.to_string().contains("does not match its identity"));
    }

    #[test]
    fn corrupt_tree_manifest_is_rejected() {
        let directory = TemporaryDirectory::new().unwrap();
        let tree = Tree::new([TreeEntry::new(
            "file",
            TreeEntryContent::File(FileContent {
                content: ContentId::of_blob(b"content"),
                executable: false,
            }),
        )
        .unwrap()])
        .unwrap();
        let mut store = FilesystemContentStore::open(&directory.path).unwrap();
        let identity = store.put_tree(tree).unwrap();
        fs::write(store.tree_path(identity), b"corrupt").unwrap();

        assert!(store.get_tree(identity).is_err());
    }
}
