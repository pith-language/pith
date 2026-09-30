//! A module's whole file set parsed into one module.

use std::sync::Arc;

use pith_diag::{SourceFile, SourceId};
use pith_hir::{PositionSidecar, merge_module_files};
use pith_ids::ContentId;

use super::definition_locations;
use super::module::ParsedModule;
use crate::workspace::{ModuleFile, ProjectFiles};

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
    parse_merged(module, parsed, artifact_of(sources.files()), diagnostics)
}

/// Parse a project's whole file set into one module: the project file
/// heads the merge, its includes follow in canonical order, and the
/// artifact identity covers the project file and every include together.
#[must_use]
pub fn parse_project_files(module: &str, files: &ProjectFiles) -> ParsedModule {
    let mut parsed = Vec::new();
    let mut diagnostics = Vec::new();
    let project = Arc::new(SourceFile::new(
        SourceId::from_raw(0),
        files.project().path(),
        files.project().text(),
    ));
    let (surface, mut project_diagnostics) = pith_syntax::parse_project(&project);
    diagnostics.append(&mut project_diagnostics);
    parsed.push((project, surface));
    for (position, file) in files.includes().files().iter().enumerate() {
        let source_file = Arc::new(SourceFile::new(
            SourceId::from_raw(u32::try_from(position.saturating_add(1)).unwrap_or(u32::MAX)),
            file.path(),
            file.text(),
        ));
        let (surface, mut file_diagnostics) = pith_syntax::parse(&source_file);
        diagnostics.append(&mut file_diagnostics);
        parsed.push((source_file, surface));
    }
    let mut ordered = vec![(files.project().path().into(), files.project().content_id())];
    ordered.extend(
        files
            .includes()
            .files()
            .iter()
            .map(|file| (file.path().into(), file.content_id())),
    );
    let artifact_id = ContentId::of_blob(&encode_files(&ordered));
    parse_merged(module, parsed, artifact_id, diagnostics)
}

/// The text-keyed half of an artifact identity: path and content identity
/// per file, in merge order.
type FileIdentity = (Box<str>, ContentId);

fn parse_merged(
    module: &str,
    parsed: Vec<(Arc<SourceFile>, pith_hir::ParsedSurface)>,
    artifact_id: ContentId,
    diagnostics: Vec<pith_diag::Diag>,
) -> ParsedModule {
    let merged = merge_module_files(&parsed);
    let definitions = definition_locations(module, &merged.surface, &merged.files);
    ParsedModule {
        module: module.into(),
        artifact_id,
        files: merged.files,
        surface: merged.surface,
        diagnostics,
        positions: PositionSidecar::new(definitions, Vec::new()),
    }
}

fn artifact_of(files: &[ModuleFile]) -> ContentId {
    let ordered = files
        .iter()
        .map(|file| (file.path().into(), file.content_id()))
        .collect::<Vec<_>>();
    ContentId::of_blob(&encode_files(&ordered))
}

fn encode_files(files: &[FileIdentity]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (path, content_id) in files {
        bytes.extend_from_slice(path.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(content_id.digest().as_bytes());
        bytes.push(0);
    }
    bytes
}
