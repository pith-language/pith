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
    RepresentedRule(RuleCategory),
    Local,
    Entry,
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

    reference_reach: Box<[(ByteOffset, usize)]>,
    definition_reach: Box<[(ByteOffset, usize)]>,
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
        if let Some(index) = self.reference_index_at(offset)
            && let Some(reference) = self.references.get(index)
        {
            return Some(reference.definition());
        }
        self.definition_index_at(offset)
            .and_then(|index| self.definitions.get(index))
    }

    fn reference_index_at(&self, offset: ByteOffset) -> Option<usize> {
        let cut = self
            .references
            .partition_point(|reference| reference.span.start <= offset);
        let &(end, position) = self.reference_reach.get(cut.checked_sub(1)?)?;
        (end > offset).then_some(position)
    }

    fn definition_index_at(&self, offset: ByteOffset) -> Option<usize> {
        let cut = self
            .definitions
            .partition_point(|definition| definition.span.start <= offset);
        let &(end, position) = self.definition_reach.get(cut.checked_sub(1)?)?;
        (end > offset).then_some(position)
    }

    pub fn new(definitions: Vec<DefinitionLocation>, references: Vec<ReferenceSite>) -> Self {
        let mut definitions = definitions;
        let mut references = references;
        definitions.sort_by_key(|definition| definition.span.start);
        references.sort_by_key(|reference| reference.span.start);
        let reference_reach = prefix_reach(references.iter().map(|reference| reference.span.end));
        let definition_reach =
            prefix_reach(definitions.iter().map(|definition| definition.span.end));
        Self {
            definitions: definitions.into(),
            references: references.into(),
            reference_reach,
            definition_reach,
        }
    }
}

fn prefix_reach(ends: impl Iterator<Item = ByteOffset>) -> Box<[(ByteOffset, usize)]> {
    let mut reach = Vec::new();
    let mut best: Option<(ByteOffset, usize)> = None;
    for (position, end) in ends.enumerate() {
        best = match best {
            Some((current, at)) if current >= end => Some((current, at)),
            _ => Some((end, position)),
        };
        if let Some(best) = best {
            reach.push(best);
        }
    }
    reach.into()
}

fn source_slice(source: &SourceFile, span: Span) -> Option<&str> {
    let start = usize::try_from(span.start.0).ok()?;
    let end = usize::try_from(span.end.0).ok()?;
    source.source_text().get(start..end)
}
