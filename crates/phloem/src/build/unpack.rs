use pith_diag::PithResult;
use pith_engine::Engine;

use crate::diag;

use super::model::{SourceFile, SourceTree};

/// Unpack a fetched archive into the engine's store: parse the tar, import
/// each file, and return the measured tree.
///
/// A caller-side effect in the position 0044 put the fetch: the parse is
/// the pure half ([`crate::archive`]), the import is this half, and the
/// tree value it returns is the declared input a build request carries.
/// The engine has no path by which a pure rule publishes new store content
/// — imports are what action capture does — so the import belongs to the
/// caller, and the file identities it returns are measured from the bytes
/// rather than taken from any claim.
///
/// # Errors
/// The parse's diagnostics when the archive is not ustar-shaped, and the
/// store's when a file cannot be imported.
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
