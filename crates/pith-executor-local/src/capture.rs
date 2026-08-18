//! Capturing declared outputs after the child exits.
//!
//! The inverse of [`crate::stage`]: walks the declared output paths, reads each
//! back from the scratch root, and builds the [`CapturedOutput`] values the
//! engine content-addresses on import. Pure filesystem work; no `unsafe`.

use std::path::Path;

use pith_core::OutputKind;
use pith_engine::{
    CapturedFileContent, CapturedOutput, CapturedOutputContent, CapturedTree,
    CapturedTreeEntryContent,
};
use pith_store::TreeEntry;
use tokio::fs;

use crate::{executor_diag, stage::DeclaredOutput};

type CaptureResult<T> = pith_diag::PithResult<T>;

/// Capture every declared output from the working directory. The executor
/// returns these as raw bytes; the engine assigns content identities on import.
pub(super) async fn capture(
    working_dir: &Path,
    outputs: &[DeclaredOutput],
) -> CaptureResult<Vec<CapturedOutput>> {
    let mut captured = Vec::with_capacity(outputs.len());
    for output in outputs {
        let path_in_scratch = working_dir.join(output.path.as_ref());
        let content = capture_output(&path_in_scratch, output.kind).await?;
        captured.push(CapturedOutput {
            path: output.path.clone(),
            content,
        });
    }
    Ok(captured)
}

async fn capture_output(path: &Path, kind: OutputKind) -> CaptureResult<CapturedOutputContent> {
    match kind {
        OutputKind::Blob => {
            let bytes = fs::read(path)
                .await
                .map_err(|error| io_diag("blob output", error))?;
            Ok(CapturedOutputContent::Blob(bytes.into_boxed_slice()))
        }
        OutputKind::Tree => {
            let tree = capture_tree(path).await?;
            Ok(CapturedOutputContent::Tree(tree))
        }
    }
}

async fn capture_tree(path: &Path) -> CaptureResult<CapturedTree> {
    let directory_entries = sorted_directory_entries(path).await?;
    let mut entries = Vec::with_capacity(directory_entries.len());
    for (name, entry) in directory_entries {
        let file_type = entry
            .file_type()
            .await
            .map_err(|error| io_diag("tree output entry type", error))?;
        let entry_path = entry.path();
        let content = if file_type.is_symlink() {
            // A symlink is an entry in its own right: its target is part of
            // the tree's identity the way a file's bytes are. Reading the
            // link rather than following it also lets a target outside the
            // captured tree, or one that does not exist yet, survive as
            // declared content instead of failing or silently becoming a
            // copy of what it points at.
            let target = fs::read_link(&entry_path)
                .await
                .map_err(|error| io_diag("tree output symlink target", error))?;
            CapturedTreeEntryContent::Symlink {
                target: target_bytes(target.as_os_str()),
            }
        } else if file_type.is_dir() {
            // Box the recursive call: an async fn that recurses directly would
            // have an infinitely sized future.
            CapturedTreeEntryContent::Tree(Box::pin(capture_tree(&entry_path)).await?)
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let metadata = fs::metadata(&entry_path)
                    .await
                    .map_err(|error| io_diag("tree output file metadata", error))?;
                let executable = metadata.permissions().mode() & 0o111 != 0;
                let bytes = fs::read(&entry_path)
                    .await
                    .map_err(|error| io_diag("tree output file bytes", error))?;
                CapturedTreeEntryContent::File(CapturedFileContent {
                    bytes: bytes.into_boxed_slice(),
                    executable,
                })
            }
            #[cfg(not(unix))]
            {
                let bytes = fs::read(&entry_path)
                    .await
                    .map_err(|error| io_diag("tree output file bytes", error))?;
                CapturedTreeEntryContent::File(CapturedFileContent {
                    bytes: bytes.into_boxed_slice(),
                    executable: false,
                })
            }
        };
        // `TreeEntry::new` validates the name and rejects `/`, NUL, `.`/`..`,
        // matching the store's own validation so the captured form imports.
        // `CapturedTreeEntry` is `TreeEntry<CapturedFileContent, CapturedTree>`,
        // which is exactly what `TreeEntry::new` returns here.
        let entry = TreeEntry::new(name, content)
            .map_err(|error| executor_diag(format!("captured tree entry is invalid: {error}")))?;
        entries.push(entry);
    }
    Ok(CapturedTree {
        entries: entries.into_boxed_slice(),
    })
}

/// Reads one directory into the UTF-8 name order used by captured trees.
/// Filesystems do not promise an iteration order, so normalization happens at
/// the adapter boundary before the entries become an executor result.
async fn sorted_directory_entries(path: &Path) -> CaptureResult<Vec<(Box<str>, fs::DirEntry)>> {
    let mut entries = Vec::new();
    let mut reader = fs::read_dir(path)
        .await
        .map_err(|error| io_diag("tree output directory", error))?;
    while let Some(entry) = reader
        .next_entry()
        .await
        .map_err(|error| io_diag("tree output entry", error))?
    {
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| executor_diag("a tree output entry name is not valid utf-8"))?
            .to_string()
            .into_boxed_str();
        entries.push((name, entry));
    }
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    Ok(entries)
}

fn io_diag(what: &str, error: std::io::Error) -> pith_diag::DiagnosticSink {
    executor_diag(format!("capturing {what} failed: {error}"))
}

/// The target bytes of a captured symlink, as the tree manifest stores them.
fn target_bytes(target: &std::ffi::OsStr) -> Box<[u8]> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        target.as_bytes().into()
    }
    #[cfg(not(unix))]
    {
        target
            .to_string_lossy()
            .into_owned()
            .into_boxed_str()
            .into_boxed_bytes()
    }
}
