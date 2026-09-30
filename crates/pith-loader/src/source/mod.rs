//! A module's text, its parse, and the canonical spelling `pith fmt`
//! writes, for one file, a whole source set, or a project file with its
//! includes. Positions are collected here: the sidecar is a property of
//! the parse, not of elaboration.

mod module;
mod multi;
mod project;

pub use module::{ModuleSource, ParsedModule, format_module, parse_module};
pub use multi::{parse_module_sources, parse_project_files};
pub use project::{
    InvalidProject, ParsedProjectFile, ProjectSource, format_project_file, parse_project_file,
};

use pith_core::Coordinate;
use pith_diag::Span;
use pith_hir::{
    DefinitionKind, DefinitionLocation, ModuleFiles, ParsedSurface, SurfaceBody, SurfaceRuleBody,
};

pub(crate) fn definition_locations(
    module: &str,
    surface: &ParsedSurface,
    files: &ModuleFiles,
) -> Vec<DefinitionLocation> {
    let at = |span: Span| files.file_of(span);
    let declaration_definitions = surface.declarations.iter().map(|declaration| {
        let kind = match declaration.body {
            SurfaceBody::Nominal(_) => DefinitionKind::Nominal,
            SurfaceBody::Sum(_) => DefinitionKind::Sum,
            SurfaceBody::Alias(_) => DefinitionKind::Alias,
        };
        let (source, name_span) = at(declaration.name_span);
        DefinitionLocation::new(
            Coordinate::new(module, declaration.name.clone()),
            kind,
            source.clone(),
            name_span,
            declaration.documentation.clone(),
        )
    });
    let rule_definitions = surface.rules.iter().map(|rule| {
        let kind = match rule.body {
            SurfaceRuleBody::Host => DefinitionKind::HostRule(rule.category),
            SurfaceRuleBody::Written(_) => DefinitionKind::RepresentedRule(rule.category),
        };
        let (source, label_span) = at(rule.label_span);
        DefinitionLocation::new(
            Coordinate::new(module, rule.label.clone()),
            kind,
            source.clone(),
            label_span,
            rule.documentation.clone(),
        )
    });
    let local_definitions = surface.locals.iter().map(|local| {
        let (source, name_span) = at(local.name_span);
        DefinitionLocation::new(
            Coordinate::new(module, local.name.clone()),
            DefinitionKind::Local,
            source.clone(),
            name_span,
            local.documentation.clone(),
        )
    });
    let entry_definitions = surface.entries.iter().map(|entry| {
        let (source, name_span) = at(entry.name_span);
        DefinitionLocation::new(
            Coordinate::new(module, entry.name.clone()),
            DefinitionKind::Entry,
            source.clone(),
            name_span,
            entry.documentation.clone(),
        )
    });
    declaration_definitions
        .chain(rule_definitions)
        .chain(local_definitions)
        .chain(entry_definitions)
        .collect()
}
