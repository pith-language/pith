use pith_diag::PithResult;
use pith_engine::Engine;

use crate::diag;

use super::model::{SourceFile, SourceTree};

/// Parses an archive and imports its files into the engine's content store.
///
/// # Errors
/// Returns a diagnostic when the archive is invalid or a file cannot be
/// imported.
pub fn unpack(engine: &mut Engine, archive: &[u8]) -> PithResult<SourceTree> {
    let files = crate::archive::parse(archive)?;
    let mut imported: Vec<SourceFile> = Vec::with_capacity(files.len());
    for file in files.iter() {
        let content = engine
            .put_blob(&file.bytes)
            .map_err(|error| diag(format!("importing `{}` failed: {error}", file.path)))?;
        imported.push(SourceFile {
            path: file.path.clone(),
            content,
        });
    }
    imported.sort_by(|left, right| left.path.as_ref().cmp(right.path.as_ref()));
    Ok(SourceTree {
        files: imported.into(),
    })
}
