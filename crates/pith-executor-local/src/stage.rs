//! Staging declared inputs and the executable into a private scratch root.
//!
//! Pure filesystem work: no `unsafe`, no syscalls beyond ordinary `mkdir`,
//! `write`, and `symlink`. The executor's authority is the mapping from the
//! engine's typed [`MaterializedContent`] to a concrete directory layout the
//! child sees. Paths come from the validated [`ActionSpec`], which already
//! rejected absolute paths, `..`, trailing slashes, and overlapping input/output
//! pairs (see `pith_core::ActionSpec::validate`).

use std::path::{Path, PathBuf};

use pith_core::OutputKind;
use pith_engine::{
    ActionInvocation, MaterializedContent, MaterializedFileContent, MaterializedTree,
    MaterializedTreeEntryContent,
};
use pith_store::TreeEntryContent;
use tokio::fs;

use crate::executor_diag;

/// The staged scratch root: where inputs were laid out, where the executable
/// lives, and where the child should run. Paths are absolute and local to this
/// machine; they do not escape the executor.
pub(super) struct StagedAction {
    /// The directory the child runs in (`chdir` target).
    pub(super) working_dir: PathBuf,
    /// The absolute path to the staged executable, with its executable bit set.
    pub(super) executable: PathBuf,
}

type StageResult<T> = pith_diag::PithResult<T>;

/// Stage `invocation` under `root`: a working directory, the executable blob
/// with its executable bit set, and each declared input at its declared relative
/// path. Output paths are *not* created here; the child creates them as it
/// writes, and [`crate::capture`] reads them back after exit.
pub(super) async fn stage(invocation: &ActionInvocation, root: &Path) -> StageResult<StagedAction> {
    let working_dir = root.join("work");
    fs::create_dir_all(&working_dir)
        .await
        .map_err(|error| io_diag("working directory", error))?;

    let executable = stage_executable(invocation, root).await?;
    stage_inputs(invocation, &working_dir).await?;

    Ok(StagedAction {
        working_dir,
        executable,
    })
}

/// The staged executable's path, written with its executable bit set so the
/// child can `execve` it. Lives at `root/exe` (a single well-known name) so the
/// landlock rule for it is one path, not one per action.
async fn stage_executable(invocation: &ActionInvocation, root: &Path) -> StageResult<PathBuf> {
    let executable_path = root.join("exe");
    let MaterializedContent::Blob(blob) = &invocation.executable else {
        return Err(executor_diag(
            "the action executable materialized as a tree, not a blob",
        ));
    };
    fs::write(&executable_path, &blob.bytes)
        .await
        .map_err(|error| io_diag("executable", error))?;
    set_executable_bit(&executable_path)?;
    Ok(executable_path)
}

/// Lay out each declared input at its declared relative path under the working
/// directory. Blobs become files; trees become directories recursively; symlinks
/// become symlinks.
async fn stage_inputs(invocation: &ActionInvocation, working_dir: &Path) -> StageResult<()> {
    for input in &invocation.inputs {
        let destination = working_dir.join(input.path.as_ref());
        stage_content(&input.content, &destination).await?;
    }
    Ok(())
}

async fn stage_content(content: &MaterializedContent, destination: &Path) -> StageResult<()> {
    match content {
        MaterializedContent::Blob(blob) => {
            create_parent(destination).await?;
            fs::write(destination, &blob.bytes)
                .await
                .map_err(|error| io_diag("input blob", error))?;
        }
        MaterializedContent::Tree(tree) => {
            stage_tree(tree, destination).await?;
        }
    }
    Ok(())
}

async fn stage_tree(tree: &MaterializedTree, destination: &Path) -> StageResult<()> {
    fs::create_dir_all(destination)
        .await
        .map_err(|error| io_diag("input tree", error))?;
    for entry in tree.entries.iter() {
        let entry_path = destination.join(entry.name());
        match entry.content() {
            MaterializedTreeEntryContent::File(MaterializedFileContent {
                bytes,
                executable,
                ..
            }) => {
                fs::write(&entry_path, bytes)
                    .await
                    .map_err(|error| io_diag("input tree file", error))?;
                if *executable {
                    set_executable_bit(&entry_path)?;
                }
            }
            MaterializedTreeEntryContent::Tree(child) => {
                // Box the recursive call: an async fn that recurses directly
                // would have an infinitely sized future. The tree depth is
                // bounded by the content store's own tree depth.
                Box::pin(stage_tree(child, &entry_path)).await?;
            }
            TreeEntryContent::Symlink { target } => {
                create_parent(&entry_path).await?;
                #[cfg(unix)]
                {
                    use std::os::unix::ffi::OsStringExt;
                    let target_os = std::ffi::OsString::from_vec(target.to_vec());
                    tokio::fs::symlink(target_os, &entry_path)
                        .await
                        .map_err(|error| io_diag("symlink", error))?;
                }
            }
        }
    }
    Ok(())
}

async fn create_parent(path: &Path) -> StageResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|error| io_diag("parent directory", error))?;
    }
    Ok(())
}

/// Set `0o755` on `path` so the child can execute it. Uses the synchronous
/// `std::fs` interface because `chmod` is not in `tokio::fs`.
fn set_executable_bit(path: &Path) -> StageResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|error| io_diag("executable bit", error))?;
    }
    let _ = path;
    Ok(())
}

fn io_diag(what: &str, error: std::io::Error) -> pith_diag::DiagnosticSink {
    executor_diag(format!("staging {what} failed: {error}"))
}

/// Declared outputs the child must produce. Used by [`crate::capture`] to walk
/// the scratch root after exit.
pub(super) struct DeclaredOutput {
    pub(super) path: Box<str>,
    pub(super) kind: OutputKind,
}

/// Extract the declared outputs from the invocation's spec. [`crate::capture`]
/// uses this so it does not re-parse the invocation.
pub(super) fn declared_outputs(invocation: &ActionInvocation) -> Vec<DeclaredOutput> {
    invocation
        .spec
        .outputs
        .iter()
        .map(|output| DeclaredOutput {
            path: output.path.clone(),
            kind: output.kind,
        })
        .collect()
}
