use std::sync::Arc;

use pith_core::Coordinate;
use pith_diag::{ByteOffset, SourceFile, Span};

use crate::RuleCategory;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefinitionKind {
    Nominal,
    Sum,
    Alias,
    HostRule(RuleCategory),
}

#[derive(Clone, Debug)]
pub struct DefinitionLocation {
    coordinate: Coordinate,
    kind: DefinitionKind,
    source: Arc<SourceFile>,
    span: Span,
    documentation: Box<[Span]>,
}

impl DefinitionLocation {
    #[must_use]
    pub fn coordinate(&self) -> &Coordinate {
        &self.coordinate
    }

    #[must_use]
    pub const fn kind(&self) -> DefinitionKind {
        self.kind
    }

    #[must_use]
    pub fn source(&self) -> &Arc<SourceFile> {
        &self.source
    }

    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    #[must_use]
    pub fn documentation_spans(&self) -> &[Span] {
        &self.documentation
    }

    #[must_use]
    pub fn documentation(&self) -> String {
        self.documentation
            .iter()
            .filter_map(|span| source_slice(&self.source, *span))
            .map(|line| line.strip_prefix("--").unwrap_or(line).trim())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn new(
        coordinate: Coordinate,
        kind: DefinitionKind,
        source: Arc<SourceFile>,
        span: Span,
        documentation: Box<[Span]>,
    ) -> Self {
        Self {
            coordinate,
            kind,
            source,
            span,
            documentation,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReferenceSite {
    written_coordinate: Coordinate,
    span: Span,
    definition: DefinitionLocation,
}

impl ReferenceSite {
    #[must_use]
    pub fn written_coordinate(&self) -> &Coordinate {
        &self.written_coordinate
    }

    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    #[must_use]
    pub fn definition(&self) -> &DefinitionLocation {
        &self.definition
    }

    pub fn new(written_coordinate: Coordinate, span: Span, definition: DefinitionLocation) -> Self {
        Self {
            written_coordinate,
            span,
            definition,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PositionSidecar {
    definitions: Box<[DefinitionLocation]>,
    references: Box<[ReferenceSite]>,
}

impl PositionSidecar {
    #[must_use]
    pub fn definitions(&self) -> &[DefinitionLocation] {
        &self.definitions
    }

    #[must_use]
    pub fn references(&self) -> &[ReferenceSite] {
        &self.references
    }

    #[must_use]
    pub fn definition_at(&self, offset: ByteOffset) -> Option<&DefinitionLocation> {
        self.references
            .iter()
            .find(|reference| contains(reference.span, offset))
            .map(ReferenceSite::definition)
            .or_else(|| {
                self.definitions
                    .iter()
                    .find(|definition| contains(definition.span, offset))
            })
    }

    pub fn new(definitions: Vec<DefinitionLocation>, references: Vec<ReferenceSite>) -> Self {
        Self {
            definitions: definitions.into(),
            references: references.into(),
        }
    }
}

fn contains(span: Span, offset: ByteOffset) -> bool {
    span.start <= offset && offset < span.end
}

fn source_slice(source: &SourceFile, span: Span) -> Option<&str> {
    let start = usize::try_from(span.start.0).ok()?;
    let end = usize::try_from(span.end.0).ok()?;
    source.source_text().get(start..end)
}
