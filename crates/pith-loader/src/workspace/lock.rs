//! The lock written beside a project file: the text projection of what
//! resolution selected. A path input is live and unwitnessed, so its entry
//! records what resolved and pins nothing; the next resolution rewrites
//! the file when an input's content moved.

use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{ResolvedModule, Workspace};
use crate::tree::measure;

/// The lock file's name, beside the project file.
pub const LOCK_NAME: &str = "pith.lock";

/// The first line of every written lock, naming the format version.
const FORMAT_LINE: &str = "pith lock 1";

/// Why the lock could not be written.
#[derive(Debug)]
pub enum LockWriteError {
    Io(io::Error),
    Unmeasurable(Box<str>),
}

impl std::fmt::Display for LockWriteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "cannot write the lock: {error}"),
            Self::Unmeasurable(subject) => {
                write!(formatter, "cannot measure the file set of `{subject}`")
            }
        }
    }
}

impl std::error::Error for LockWriteError {}

/// Render `workspace`'s lock and write it beside the project file at
/// `root_project`, leaving the bytes alone when they already spell the
/// same lock.
///
/// # Errors
/// [`LockWriteError`] when a module's file set cannot be measured or the
/// file cannot be written.
pub fn write_lock(workspace: &Workspace, root_project: &Path) -> Result<(), LockWriteError> {
    let rendered = render(workspace)?;
    let parent = root_project
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let path = parent.join(LOCK_NAME);
    if std::fs::read_to_string(&path).ok().as_deref() == Some(rendered.as_str()) {
        return Ok(());
    }
    replace(&path, rendered.as_bytes()).map_err(LockWriteError::Io)
}

/// The lock's text: one line per selected input, sorted by subject, after
/// the line naming the format version. Deterministic over a resolution, so
/// the same inputs render the same bytes in any process. An entry records
/// what resolution adds; the route that reached the content stays in the
/// declarations, which already hold it.
fn render(workspace: &Workspace) -> Result<String, LockWriteError> {
    let mut entries = workspace
        .dependencies()
        .iter()
        .map(|module| {
            entry(module).ok_or_else(|| LockWriteError::Unmeasurable(module.subject().spelling()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    let mut out = String::from(FORMAT_LINE);
    out.push('\n');
    for line in entries {
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out)
}

/// One input's line: subject, canonical version, and the measured identity
/// of the project file and its includes.
fn entry(module: &ResolvedModule) -> Option<String> {
    let includes = module
        .files()
        .includes()
        .files()
        .iter()
        .map(|file| (file.path().into(), file.text().into()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let measured = measure(module.files().project().text(), &includes);
    Some(format!(
        "{} {} {}",
        module.subject(),
        module.project().version().canonical_spelling(),
        measured.digest(),
    ))
}

/// A caller-side write under 0041's discipline: a temporary file named for
/// this writer, created exclusively, flushed and renamed into place, the
/// directory flushed after the rename, and the temporary removed on every
/// failure path.
fn replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let (temporary, mut file) = loop {
        let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".pith-lock-{}-{sequence}.tmp", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => break (candidate, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    };
    let publish = (|| {
        use std::io::Write;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)?;
        std::fs::File::open(parent)?.sync_all()
    })();
    if publish.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    publish
}
