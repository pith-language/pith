//! Atomic filesystem publication for written locks.
//!
//! Publication writes and flushes a temporary file, renames it into place,
//! then flushes the destination directory. These operations are caller-side
//! effects.

use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use pith_diag::PithResult;

use crate::diag;
use crate::document::Lock;
use crate::lockfile;

/// Atomically writes a lock through an exclusive temporary file.
///
/// # Errors
/// Returns a diagnostic when writing, flushing, renaming, or directory
/// synchronization fails.
pub fn write(lock: &Lock, path: &Path) -> PithResult<()> {
    let text = lockfile::render(lock);
    let directory = destination_directory(path);
    let (temporary, mut file) =
        temporary_file(&directory).map_err(|error| io_diag("writing the lock", path, &error))?;
    if let Err(error) = file
        .write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
    {
        drop(file);
        remove_temporary_file(&temporary);
        return Err(io_diag("writing the lock", path, &error));
    }
    drop(file);
    match std::fs::rename(&temporary, path) {
        Ok(()) => std::fs::File::open(&directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| io_diag("flushing the directory of the lock", path, &error)),
        Err(error) => {
            remove_temporary_file(&temporary);
            Err(io_diag("publishing the lock", path, &error))
        }
    }
}

/// The directory a rename into `path` happens in; a bare file name
/// publishes into the working directory.
fn destination_directory(path: &Path) -> std::path::PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => std::path::PathBuf::from("."),
    }
}

/// The process-wide sequence behind the store's per-instance one, so two
/// writers in one process cannot share a temporary file either.
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

fn temporary_file(directory: &Path) -> std::io::Result<(std::path::PathBuf, std::fs::File)> {
    loop {
        let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            ".pith-lock-{}.{}.tmp",
            std::process::id(),
            sequence
        ));
        match std::fs::File::create_new(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
}

/// Best-effort removal on a failure path; a temporary left behind is inert
/// and never parses as a lock under its hidden name.
fn remove_temporary_file(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// Reads and parses the lock at `path`, which names the parsed text in
/// every diagnostic a malformed lock produces.
///
/// # Errors
/// Returns a diagnostic when the file cannot be read or parsed.
pub fn read(path: &Path) -> PithResult<Lock> {
    let text =
        std::fs::read_to_string(path).map_err(|error| io_diag("reading the lock", path, &error))?;
    lockfile::parse(&path.display().to_string(), &text)
}

fn io_diag(what: &str, path: &Path, error: &std::io::Error) -> pith_diag::DiagnosticSink {
    diag(format!("{what} at {} failed: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{DomainIdentity, PackageIdentity, PackageVersion};
    use crate::lock::{LockEntry, Origin};
    use crate::lockfile::LOCK_FILE_VERSION;
    use crate::preference::{Preference, PreferenceList};
    use pith_ids::ContentId;
    use tempfile::TempDir;

    fn lock() -> Lock {
        let universe = ContentId::of_blob(b"universe");
        Lock::new(
            crate::resolve::resolver_revision_hex(),
            Box::from("numeric-segments"),
            universe,
            PreferenceList(Box::new([Preference::Newest])),
            vec![
                LockEntry::new(
                    PackageVersion::new(
                        PackageIdentity::declare(DomainIdentity::new("pithpkgs"), "zlib"),
                        "1.3",
                    ),
                    [] as [&str; 0],
                    ContentId::of_blob(b"zlib-1.3.tar"),
                    Origin::Registry("pkgs.pith-lang.org".into()),
                ),
                LockEntry::new(
                    PackageVersion::new(
                        PackageIdentity::declare(DomainIdentity::new("pithpkgs"), "openssl"),
                        "1.1.1",
                    ),
                    ["shared", "zlib"],
                    ContentId::of_blob(b"openssl-1.1.1.tar"),
                    Origin::Forge("git.pith-lang.org/openssl".into()),
                ),
            ],
        )
        .unwrap()
    }

    #[test]
    fn writing_and_reading_round_trip_through_a_file() {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("pith.lock");
        write(&lock(), &path).unwrap();
        assert!(path.exists());
        let remaining: Vec<std::ffi::OsString> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(
            remaining,
            vec![std::ffi::OsString::from("pith.lock")],
            "a successful write leaves the lock and no temporary behind"
        );
        assert_eq!(read(&path).unwrap(), lock());
        let error = read(&directory.path().join("absent.lock")).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("absent.lock") && d.message.0.contains("reading")),
            "the diagnostic names the path and the operation: {error:?}"
        );
    }

    #[test]
    fn the_written_first_line_names_the_format_version() {
        let scratch = TempDir::new().unwrap();
        let path = scratch.path().join("pith.lock");
        write(&lock(), &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.starts_with(&format!("lock-version {LOCK_FILE_VERSION}\n")),
            "the file opens with the format version: {text}"
        );
    }
}
