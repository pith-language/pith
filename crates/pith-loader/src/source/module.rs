//! One source file's module: identity, text, parse, and canonical
//! spelling.

use std::sync::Arc;

use pith_diag::{Diag, SourceFile, SourceId};
use pith_hir::{ModuleFiles, ParsedSurface, PositionSidecar};
use pith_ids::ContentId;

use super::definition_locations;

pub struct ModuleSource {
    pub module: Box<str>,
    pub source_id: SourceId,
    pub label: Box<str>,
    pub text: Box<str>,
}

impl ModuleSource {
    #[must_use]
    pub fn new(
        module: impl Into<Box<str>>,
        source_id: SourceId,
        label: impl Into<Box<str>>,
        text: impl Into<Box<str>>,
    ) -> Self {
        Self {
            module: module.into(),
            source_id,
            label: label.into(),
            text: text.into(),
        }
    }
}

/// A parsed module: one file in standalone mode, or a whole sorted source
/// set merged into one span space in manifest mode. Either way the files map
/// every merged span back to the file that owns it.
pub struct ParsedModule {
    pub(crate) module: Box<str>,
    pub(crate) artifact_id: ContentId,
    pub(crate) files: ModuleFiles,
    pub(crate) surface: ParsedSurface,
    pub(crate) diagnostics: Vec<Diag>,
    pub(crate) positions: PositionSidecar,
}

impl ParsedModule {
    #[must_use]
    pub fn module(&self) -> &str {
        &self.module
    }

    #[must_use]
    pub const fn artifact_id(&self) -> ContentId {
        self.artifact_id
    }

    #[must_use]
    pub const fn files(&self) -> &ModuleFiles {
        &self.files
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[Diag] {
        &self.diagnostics
    }

    #[must_use]
    pub fn positions(&self) -> &PositionSidecar {
        &self.positions
    }

    pub fn imports(&self) -> impl Iterator<Item = &str> {
        self.surface
            .imports
            .iter()
            .map(|import| import.module.as_ref())
    }
}

#[must_use]
pub fn parse_module(source: &ModuleSource) -> ParsedModule {
    let source_file = Arc::new(SourceFile::new(
        source.source_id,
        source.label.clone(),
        source.text.clone(),
    ));
    let artifact_id = ContentId::of_blob(source_file.source_text().as_bytes());
    let (surface, diagnostics) = pith_syntax::parse(&source_file);
    let definitions =
        definition_locations(&source.module, &surface, &ModuleFiles::one(&source_file));
    ParsedModule {
        module: source.module.clone(),
        artifact_id,
        files: ModuleFiles::one(&source_file),
        surface,
        diagnostics,
        positions: PositionSidecar::new(definitions, Vec::new()),
    }
}

/// The canonical spelling of `source`'s module: what `pith fmt` writes, and
/// what the digest-stability property is measured over.
///
/// # Errors
///
/// Returns the parse diagnostics when the source does not parse. A module
/// that does not parse has no canonical spelling, and recovering one would
/// be a second parser with different recovery rules.
pub fn format_module(source: &ModuleSource) -> Result<String, Box<[Diag]>> {
    let ParsedModule {
        surface,
        files,
        diagnostics,
        ..
    } = parse_module(source);
    if let ([source_file], true) = (files.sources(), diagnostics.is_empty()) {
        Ok(pith_syntax::print(&surface, source_file))
    } else if diagnostics.is_empty() {
        unreachable!("a standalone parse holds exactly one file")
    } else {
        Err(diagnostics.into())
    }
}
