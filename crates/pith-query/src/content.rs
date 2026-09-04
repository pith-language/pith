use std::fs;
use std::path::Path;

use pith_ids::ContentId;
use pith_output::dto::{StoredContent, StoredContentKind};
use pith_store::{ContentStore, FileContent, StoreError, Tree, TreeEntry, TreeEntryContent};

use crate::error::QueryError;

pub(crate) fn add(store: &mut impl ContentStore, path: &Path) -> Result<StoredContent, QueryError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| read_failure(path, &error))?;
    let (id, kind) = if metadata.is_dir() {
        (add_tree(store, path)?, StoredContentKind::Tree)
    } else if metadata.is_symlink() {
        return Err(QueryError::user(format!(
            "`{}` is a symlink; add the directory that holds it, so the link is stored as an \
             entry rather than as a copy of its target",
            path.display()
        )));
    } else if metadata.is_file() {
        (add_blob(store, path)?, StoredContentKind::Blob)
    } else {
        return Err(unsupported_file_type(path));
    };
    Ok(StoredContent {
        id: id.digest().to_string().into(),
        kind,
        path: Some(path.display().to_string().into()),
    })
}

fn add_blob(store: &mut impl ContentStore, path: &Path) -> Result<ContentId, QueryError> {
    let bytes = fs::read(path).map_err(|error| read_failure(path, &error))?;
    store.put_blob(&bytes).map_err(store_failure)
}

fn add_tree(store: &mut impl ContentStore, path: &Path) -> Result<ContentId, QueryError> {
    let tree = build_tree(store, path)?;
    store.put_tree(tree).map_err(store_failure)
}

fn build_tree(store: &mut impl ContentStore, path: &Path) -> Result<Tree, QueryError> {
    let mut entries = Vec::new();
    let directory = fs::read_dir(path).map_err(|error| read_failure(path, &error))?;
    for entry in directory {
        let entry = entry.map_err(|error| read_failure(path, &error))?;
        let entry_path = entry.path();
        let name = entry.file_name().into_string().map_err(|_| {
            QueryError::user(format!("`{}` is not valid utf-8", entry_path.display()))
        })?;
        let file_type = entry
            .file_type()
            .map_err(|error| read_failure(&entry_path, &error))?;
        let content = if file_type.is_symlink() {
            let target =
                fs::read_link(&entry_path).map_err(|error| read_failure(&entry_path, &error))?;
            TreeEntryContent::Symlink {
                target: target_bytes(target.as_os_str()),
            }
        } else if file_type.is_dir() {
            let subtree = build_tree(store, &entry_path)?;
            TreeEntryContent::Tree(store.put_tree(subtree).map_err(store_failure)?)
        } else if file_type.is_file() {
            let bytes = fs::read(&entry_path).map_err(|error| read_failure(&entry_path, &error))?;
            TreeEntryContent::File(FileContent {
                content: store.put_blob(&bytes).map_err(store_failure)?,
                executable: is_executable(&entry_path)?,
            })
        } else {
            return Err(unsupported_file_type(&entry_path));
        };
        entries.push(
            TreeEntry::new(name, content)
                .map_err(|error| QueryError::user(format!("{}: {error}", entry_path.display())))?,
        );
    }
    Tree::new(entries).map_err(store_failure)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> Result<bool, QueryError> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path).map_err(|error| read_failure(path, &error))?;
    Ok(metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> Result<bool, QueryError> {
    Ok(false)
}

#[cfg(unix)]
fn target_bytes(target: &std::ffi::OsStr) -> Box<[u8]> {
    use std::os::unix::ffi::OsStrExt;
    target.as_bytes().into()
}

#[cfg(not(unix))]
fn target_bytes(target: &std::ffi::OsStr) -> Box<[u8]> {
    target.to_string_lossy().into_owned().into_bytes().into()
}

fn unsupported_file_type(path: &Path) -> QueryError {
    QueryError::user(format!(
        "`{}` is not a regular file, directory, or symbolic link",
        path.display()
    ))
}

fn read_failure(path: &Path, error: &std::io::Error) -> QueryError {
    QueryError::user(format!("cannot read `{}`: {error}", path.display()))
}

fn store_failure(error: StoreError) -> QueryError {
    QueryError::internal(error.to_string())
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::net::UnixListener;

    use pith_store::MemoryContentStore;

    use super::add;

    #[test]
    fn special_files_are_refused_at_the_root_and_inside_trees()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let socket = directory.path().join("socket");
        let _listener = UnixListener::bind(&socket)?;
        let mut store = MemoryContentStore::default();

        let root_error = add(&mut store, &socket);
        assert!(
            root_error
                .as_ref()
                .is_err_and(|error| error.message().contains("not a regular file")),
            "{root_error:?}"
        );

        let tree_error = add(&mut store, directory.path());
        assert!(
            tree_error
                .as_ref()
                .is_err_and(|error| error.message().contains("not a regular file")),
            "{tree_error:?}"
        );
        Ok(())
    }
}
