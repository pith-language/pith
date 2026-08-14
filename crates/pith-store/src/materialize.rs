use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use pith_ids::ContentId;

use crate::{ContentStore, StoreError, TreeEntryContent};

static NEXT_STAGING_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// Materialize a stored tree as a filesystem directory.
///
/// The destination must not already exist. Objects are first rendered into a
/// sibling staging directory and published with one rename, so a failed read
/// or write does not leave a partial destination behind.
///
/// # Errors
/// Returns an adapter error when the tree or one of its referenced objects is
/// unavailable, the destination already exists, or filesystem work fails.
pub fn materialize_tree(
    store: &dyn ContentStore,
    identity: ContentId,
    destination: impl AsRef<Path>,
) -> Result<(), StoreError> {
    let destination = destination.as_ref();
    if destination.as_os_str().is_empty() {
        return Err(StoreError::new("materialization destination is empty"));
    }
    if path_exists(destination)? {
        return Err(StoreError::new(format!(
            "materialization destination '{}' already exists",
            destination.display()
        )));
    }

    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| io_error("create materialization parent directory", error))?;
    let staging = create_staging_directory(parent)?;

    let result = materialize_tree_into(store, identity, &staging).and_then(|()| {
        if path_exists(destination)? {
            return Err(StoreError::new(format!(
                "materialization destination '{}' already exists",
                destination.display()
            )));
        }
        fs::rename(&staging, destination)
            .map_err(|error| io_error("publish materialized tree", error))
    });
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn create_staging_directory(parent: &Path) -> Result<PathBuf, StoreError> {
    loop {
        let sequence = NEXT_STAGING_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".pith-materialize-{}-{sequence}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(io_error("create materialization staging directory", error)),
        }
    }
}

fn materialize_tree_into(
    store: &dyn ContentStore,
    identity: ContentId,
    destination: &Path,
) -> Result<(), StoreError> {
    let tree = store.get_tree(identity)?.ok_or_else(|| {
        StoreError::new(format!(
            "tree {} is not available in the content store",
            identity.digest()
        ))
    })?;

    for entry in tree.entries() {
        let path = destination.join(entry.name());
        match entry.content() {
            TreeEntryContent::File(file) => {
                let blob = store.get_blob(file.content)?.ok_or_else(|| {
                    StoreError::new(format!(
                        "blob {} is not available in the content store",
                        file.content.digest()
                    ))
                })?;
                fs::write(&path, blob.as_bytes())
                    .map_err(|error| io_error("write materialized file", error))?;
                set_executable(&path, file.executable)?;
            }
            TreeEntryContent::Tree(child) => {
                fs::create_dir(&path)
                    .map_err(|error| io_error("create materialized directory", error))?;
                materialize_tree_into(store, *child, &path)?;
            }
            TreeEntryContent::Symlink { target } => create_symlink(target, &path)?,
        }
    }
    Ok(())
}

fn set_executable(path: &Path, executable: bool) -> Result<(), StoreError> {
    #[cfg(unix)]
    if executable {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .map_err(|error| io_error("set materialized executable permissions", error))?;
    }
    let _ = (path, executable);
    Ok(())
}

#[cfg(unix)]
fn create_symlink(target: &[u8], path: &Path) -> Result<(), StoreError> {
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::symlink;

    symlink(std::ffi::OsString::from_vec(target.to_vec()), path)
        .map_err(|error| io_error("create materialized symlink", error))
}

#[cfg(not(unix))]
fn create_symlink(_target: &[u8], _path: &Path) -> Result<(), StoreError> {
    Err(StoreError::new(
        "materializing tree symlinks is not supported on this platform",
    ))
}

fn path_exists(path: &Path) -> Result<bool, StoreError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error("inspect materialization destination", error)),
    }
}

fn io_error(operation: &str, error: io::Error) -> StoreError {
    StoreError::new(format!("{operation}: {error}"))
}
