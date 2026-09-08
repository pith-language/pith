//! One manifest file's text, its parse, and its canonical spelling.

use std::sync::Arc;

use pith_diag::{Diag, SourceFile, SourceId};
use pith_hir::ParsedManifest;
use pith_ids::ContentId;

/// Why a parsed manifest file holds no validated manifest.
#[derive(Debug, PartialEq, Eq)]
pub enum InvalidManifest {
    Diagnostics,
    MissingModule,
}

/// One `module.pi` file's text on its way through the manifest grammar.
pub struct ManifestSource {
    pub source_id: SourceId,
    pub label: Box<str>,
    pub text: Box<str>,
}

impl ManifestSource {
    #[must_use]
    pub fn new(source_id: SourceId, label: impl Into<Box<str>>, text: impl Into<Box<str>>) -> Self {
        Self {
            source_id,
            label: label.into(),
            text: text.into(),
        }
    }
}

pub struct ParsedManifestFile {
    pub(crate) artifact_id: ContentId,
    pub(crate) source: Arc<SourceFile>,
    pub(crate) manifest: ParsedManifest,
    pub(crate) diagnostics: Vec<Diag>,
}

impl ParsedManifestFile {
    #[must_use]
    pub const fn artifact_id(&self) -> ContentId {
        self.artifact_id
    }

    #[must_use]
    pub fn source(&self) -> &Arc<SourceFile> {
        &self.source
    }

    #[must_use]
    pub fn manifest(&self) -> &ParsedManifest {
        &self.manifest
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[Diag] {
        &self.diagnostics
    }

    /// The validated manifest: present only when the parse reported nothing
    /// and declared its module clause; the diagnostics this file carries
    /// already say why neither held.
    ///
    /// # Errors
    /// Returns [`InvalidManifest`] when the parse reported diagnostics or no
    /// module clause survived.
    pub fn validated(&self) -> Result<pith_hir::Manifest, InvalidManifest> {
        if !self.diagnostics.is_empty() {
            return Err(InvalidManifest::Diagnostics);
        }
        self.manifest
            .manifest()
            .map_err(|_| InvalidManifest::MissingModule)
    }
}

#[must_use]
pub fn parse_manifest(source: &ManifestSource) -> ParsedManifestFile {
    let source_file = Arc::new(SourceFile::new(
        source.source_id,
        source.label.clone(),
        source.text.clone(),
    ));
    let artifact_id = ContentId::of_blob(source_file.source_text().as_bytes());
    let (manifest, diagnostics) = pith_syntax::parse_manifest(&source_file);
    ParsedManifestFile {
        artifact_id,
        source: source_file,
        manifest,
        diagnostics,
    }
}

/// The canonical spelling of `source`'s manifest, which needs no dependency
/// resolution: printing is a function of the manifest alone.
///
/// # Errors
///
/// Returns the parse diagnostics when the manifest does not parse, as
/// `format_module` does.
pub fn format_manifest(source: &ManifestSource) -> Result<String, Box<[Diag]>> {
    let ParsedManifestFile {
        source,
        manifest,
        diagnostics,
        ..
    } = parse_manifest(source);
    if diagnostics.is_empty() {
        Ok(pith_syntax::print_manifest(&manifest, &source))
    } else {
        Err(diagnostics.into())
    }
}
