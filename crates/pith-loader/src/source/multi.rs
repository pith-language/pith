//! A module's whole source set parsed into one module.

use std::sync::Arc;

use pith_diag::{SourceFile, SourceId};
use pith_hir::{PositionSidecar, merge_module_files};
use pith_ids::ContentId;

use super::definition_locations;
use super::module::ParsedModule;

/// Parse a module's whole source set into one module: files merge in the
/// set's canonical order, positions attribute back to the file that owns
/// them, and the artifact identity covers path and text together, so a
/// rename or a move is not the same module bytes.
#[must_use]
pub fn parse_module_sources(module: &str, sources: &crate::SourceSet) -> ParsedModule {
    let mut parsed = Vec::new();
    let mut diagnostics = Vec::new();
    for (position, file) in sources.files().iter().enumerate() {
        let source_file = Arc::new(SourceFile::new(
            SourceId::from_raw(u32::try_from(position).unwrap_or(u32::MAX)),
            file.path(),
            file.text(),
        ));
        let (surface, mut file_diagnostics) = pith_syntax::parse(&source_file);
        diagnostics.append(&mut file_diagnostics);
        parsed.push((source_file, surface));
    }
    let merged = merge_module_files(&parsed);
    let definitions = definition_locations(module, &merged.surface, &merged.files);
    ParsedModule {
        module: module.into(),
        artifact_id: source_set_artifact(sources),
        files: merged.files,
        surface: merged.surface,
        diagnostics,
        positions: PositionSidecar::new(definitions, Vec::new()),
    }
}

fn source_set_artifact(sources: &crate::SourceSet) -> ContentId {
    let mut bytes = Vec::new();
    for file in sources.files() {
        bytes.extend_from_slice(file.path().as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(file.content_id().digest().as_bytes());
        bytes.push(0);
    }
    ContentId::of_blob(&bytes)
}
