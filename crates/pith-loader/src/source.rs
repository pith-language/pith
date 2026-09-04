//! The module source, its parse, and the canonical spelling `pith fmt`
//! writes. Positions are collected here: the sidecar is a property of the
//! parse, not of elaboration.

use std::sync::Arc;

use pith_core::Coordinate;
use pith_diag::{Diag, SourceFile, SourceId};
use pith_hir::{
    DefinitionKind, DefinitionLocation, ParsedSurface, PositionSidecar, SurfaceBody,
    SurfaceRuleBody,
};
use pith_ids::ContentId;

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

pub struct ParsedModule {
    pub(crate) module: Box<str>,
    pub(crate) artifact_id: ContentId,
    pub(crate) source: Arc<SourceFile>,
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
    pub fn source(&self) -> &Arc<SourceFile> {
        &self.source
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
    let definitions = definition_locations(&source.module, &surface, &source_file);
    ParsedModule {
        module: source.module.clone(),
        artifact_id,
        source: source_file,
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
        source,
        diagnostics,
        ..
    } = parse_module(source);
    if diagnostics.is_empty() {
        Ok(pith_syntax::print(&surface, &source))
    } else {
        Err(diagnostics.into())
    }
}

fn definition_locations(
    module: &str,
    surface: &ParsedSurface,
    source: &Arc<SourceFile>,
) -> Vec<DefinitionLocation> {
    let declaration_definitions = surface.declarations.iter().map(|declaration| {
        let kind = match declaration.body {
            SurfaceBody::Nominal(_) => DefinitionKind::Nominal,
            SurfaceBody::Sum(_) => DefinitionKind::Sum,
            SurfaceBody::Alias(_) => DefinitionKind::Alias,
        };
        DefinitionLocation::new(
            Coordinate::new(module, declaration.name.clone()),
            kind,
            source.clone(),
            declaration.name_span,
            declaration.documentation.clone(),
        )
    });
    let rule_definitions = surface.rules.iter().map(|rule| {
        let kind = match rule.body {
            SurfaceRuleBody::Host => DefinitionKind::HostRule(rule.category),
            SurfaceRuleBody::Written(_) => DefinitionKind::RepresentedRule(rule.category),
        };
        DefinitionLocation::new(
            Coordinate::new(module, rule.label.clone()),
            kind,
            source.clone(),
            rule.label_span,
            rule.documentation.clone(),
        )
    });
    let local_definitions = surface.locals.iter().map(|local| {
        DefinitionLocation::new(
            Coordinate::new(module, local.name.clone()),
            DefinitionKind::Local,
            source.clone(),
            local.name_span,
            local.documentation.clone(),
        )
    });
    let entry_definitions = surface.entries.iter().map(|entry| {
        DefinitionLocation::new(
            Coordinate::new(module, entry.name.clone()),
            DefinitionKind::Entry,
            source.clone(),
            entry.name_span,
            entry.documentation.clone(),
        )
    });
    declaration_definitions
        .chain(rule_definitions)
        .chain(local_definitions)
        .chain(entry_definitions)
        .collect()
}
