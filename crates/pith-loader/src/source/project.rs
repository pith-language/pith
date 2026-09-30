//! One project file's text, its parse, and its canonical spelling.

use std::sync::Arc;

use pith_diag::{Diag, SourceFile, SourceId};
use pith_hir::{DocumentHeader, Project, ProjectHeader};
use pith_ids::ContentId;

/// Why a parsed project file holds no validated project.
#[derive(Debug, PartialEq, Eq)]
pub enum InvalidProject {
    Diagnostics,
    MissingSubject,
}

/// One project file's text on its way through the project grammar.
pub struct ProjectSource {
    pub source_id: SourceId,
    pub label: Box<str>,
    pub text: Box<str>,
}

impl ProjectSource {
    #[must_use]
    pub fn new(source_id: SourceId, label: impl Into<Box<str>>, text: impl Into<Box<str>>) -> Self {
        Self {
            source_id,
            label: label.into(),
            text: text.into(),
        }
    }
}

pub struct ParsedProjectFile {
    pub(crate) artifact_id: ContentId,
    pub(crate) source: Arc<SourceFile>,
    pub(crate) surface: pith_hir::ParsedSurface,
    pub(crate) diagnostics: Vec<Diag>,
}

impl ParsedProjectFile {
    #[must_use]
    pub const fn artifact_id(&self) -> ContentId {
        self.artifact_id
    }

    #[must_use]
    pub fn source(&self) -> &Arc<SourceFile> {
        &self.source
    }

    #[must_use]
    pub fn header(&self) -> &ProjectHeader {
        match &self.surface.header {
            DocumentHeader::Project(header) => header,
            DocumentHeader::Include => {
                unreachable!("parse_project_file parses a project document")
            }
        }
    }

    #[must_use]
    pub fn surface(&self) -> &pith_hir::ParsedSurface {
        &self.surface
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[Diag] {
        &self.diagnostics
    }

    /// The validated project: present only when the parse reported nothing
    /// and declared its module clause; the diagnostics this file carries
    /// already say why neither held.
    ///
    /// # Errors
    /// Returns [`InvalidProject`] when the parse reported diagnostics or no
    /// module clause survived.
    pub fn validated(&self) -> Result<Project, InvalidProject> {
        if !self.diagnostics.is_empty() {
            return Err(InvalidProject::Diagnostics);
        }
        Project::from_header(self.header()).map_err(|_| InvalidProject::MissingSubject)
    }
}

#[must_use]
pub fn parse_project_file(source: &ProjectSource) -> ParsedProjectFile {
    let source_file = Arc::new(SourceFile::new(
        source.source_id,
        source.label.clone(),
        source.text.clone(),
    ));
    let artifact_id = ContentId::of_blob(source_file.source_text().as_bytes());
    let (surface, diagnostics) = pith_syntax::parse_project(&source_file);
    ParsedProjectFile {
        artifact_id,
        source: source_file,
        surface,
        diagnostics,
    }
}

/// The canonical spelling of `source`'s project file, which needs no
/// dependency resolution: printing is a function of the file alone.
///
/// # Errors
///
/// Returns the parse diagnostics when the file does not parse, as
/// `format_module` does.
pub fn format_project_file(source: &ProjectSource) -> Result<String, Box<[Diag]>> {
    let parsed = parse_project_file(source);
    if parsed.diagnostics.is_empty() {
        Ok(pith_syntax::print(&parsed.surface, &parsed.source))
    } else {
        Err(parsed.diagnostics.into())
    }
}
