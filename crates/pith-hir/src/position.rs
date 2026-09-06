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
    source: Arc<SourceFile>,
    span: Span,
    definition: DefinitionLocation,
}

impl ReferenceSite {
    #[must_use]
    pub fn written_coordinate(&self) -> &Coordinate {
        &self.written_coordinate
    }

    /// The file the reference is written in. The span is local to it: a
    /// site carries its own attribution, so a module of several files never
    /// presents one file's offsets under another file's name.
    #[must_use]
    pub fn source(&self) -> &Arc<SourceFile> {
        &self.source
    }

    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    #[must_use]
    pub fn definition(&self) -> &DefinitionLocation {
        &self.definition
    }

    pub fn new(
        written_coordinate: Coordinate,
        source: Arc<SourceFile>,
        span: Span,
        definition: DefinitionLocation,
    ) -> Self {
        Self {
            written_coordinate,
            source,
            span,
            definition,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PositionSidecar {
    definitions: Box<[DefinitionLocation]>,
    references: Box<[ReferenceSite]>,

    reference_reach: Box<[(u32, ByteOffset, usize)]>,
    definition_reach: Box<[(u32, ByteOffset, usize)]>,
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

    /// The definition visible at `offset` within `source`, if any. Offsets
    /// are file-local: a lookup names its file, so a module of several files
    /// answers from the right one.
    #[must_use]
    pub fn definition_at(
        &self,
        source: &SourceFile,
        offset: ByteOffset,
    ) -> Option<&DefinitionLocation> {
        if let Some(index) = self.reference_index_at(source, offset)
            && let Some(reference) = self.references.get(index)
        {
            return Some(reference.definition());
        }
        self.definition_index_at(source, offset)
            .and_then(|index| self.definitions.get(index))
    }

    fn reference_index_at(&self, source: &SourceFile, offset: ByteOffset) -> Option<usize> {
        let cut = self.references.partition_point(|reference| {
            (reference.source().id.to_raw(), reference.span().start) <= (source.id.to_raw(), offset)
        });
        let &(file, end, position) = self.reference_reach.get(cut.checked_sub(1)?)?;
        (file == source.id.to_raw() && end > offset).then_some(position)
    }

    fn definition_index_at(&self, source: &SourceFile, offset: ByteOffset) -> Option<usize> {
        let cut = self.definitions.partition_point(|definition| {
            (definition.source().id.to_raw(), definition.span().start)
                <= (source.id.to_raw(), offset)
        });
        let &(file, end, position) = self.definition_reach.get(cut.checked_sub(1)?)?;
        (file == source.id.to_raw() && end > offset).then_some(position)
    }

    pub fn new(definitions: Vec<DefinitionLocation>, references: Vec<ReferenceSite>) -> Self {
        let mut definitions = definitions;
        let mut references = references;
        definitions
            .sort_by_key(|definition| (definition.source().id.to_raw(), definition.span.start));
        references.sort_by_key(|reference| (reference.source().id.to_raw(), reference.span.start));
        let reference_reach = prefix_reach(
            references
                .iter()
                .map(|reference| (reference.source().id.to_raw(), reference.span.end)),
        );
        let definition_reach = prefix_reach(
            definitions
                .iter()
                .map(|definition| (definition.source().id.to_raw(), definition.span.end)),
        );
        Self {
            definitions: definitions.into(),
            references: references.into(),
            reference_reach,
            definition_reach,
        }
    }
}

/// The running furthest reach within one file: at each position, the end of
/// the entry that reaches furthest so far in that entry's own file. The
/// reach resets at file boundaries, so a lookup can only be answered by an
/// entry of the file it named.
fn prefix_reach(ends: impl Iterator<Item = (u32, ByteOffset)>) -> Box<[(u32, ByteOffset, usize)]> {
    let mut reach = Vec::new();
    let mut best: Option<(ByteOffset, usize)> = None;
    let mut file = None;
    for (position, (next_file, end)) in ends.enumerate() {
        if file != Some(next_file) {
            file = Some(next_file);
            best = None;
        }
        best = match best {
            Some((current, at)) if current >= end => Some((current, at)),
            _ => Some((end, position)),
        };
        if let Some((end, at)) = best {
            reach.push((next_file, end, at));
        }
    }
    reach.into()
}

fn source_slice(source: &SourceFile, span: Span) -> Option<&str> {
    let start = usize::try_from(span.start.0).ok()?;
    let end = usize::try_from(span.end.0).ok()?;
    source.source_text().get(start..end)
}
