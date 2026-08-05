//! Spans, diagnostics, and the result type for the pith kernel.
//!
//! Leaf crate: every crate that produces structured output depends on this.
//! `text-size` and `line-index` are wrapped behind [`Span`] / [`SourceFile`]
//! so they never appear in public types outside this module.

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ByteOffset(pub u32);

/// A half-open byte range `[start, end)` into a [`SourceFile`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: ByteOffset,
    pub end: ByteOffset,
}

impl Span {
    pub const fn point(at: ByteOffset) -> Self {
        Self { start: at, end: at }
    }

    pub const fn none() -> Self {
        Self {
            start: ByteOffset(0),
            end: ByteOffset(0),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub id: SourceId,
    pub label: Box<str>,
    text: Box<str>,
}

impl SourceFile {
    pub fn new(id: SourceId, label: impl Into<Box<str>>, text: impl Into<Box<str>>) -> Self {
        Self {
            id,
            label: label.into(),
            text: text.into(),
        }
    }

    pub fn source_text(&self) -> &str {
        &self.text
    }

    pub fn line_col(&self, offset: ByteOffset) -> (usize, usize) {
        let index = line_index::LineIndex::new(self.source_text());
        let pos = line_index::TextSize::new(offset.0);
        let line_col = index.line_col(pos);
        (
            line_col.line.saturating_add(1) as usize,
            line_col.col.saturating_add(1) as usize,
        )
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SourceId(u32);

impl SourceId {
    pub const fn from_raw(n: u32) -> Self {
        Self(n)
    }
    pub const fn to_raw(self) -> u32 {
        self.0
    }
}

/// A stable diagnostic code. JSON consumers depend on these being stable across
/// releases; never renumber, only add (requirement K-11).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StableCode(pub u32);

impl StableCode {
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "engine diagnostic codes occupy the stable 1000-based namespace"
    )]
    pub const fn engine(code: u32) -> Self {
        Self(1000 + code)
    }

    #[allow(
        clippy::arithmetic_side_effects,
        reason = "composition diagnostic codes occupy the stable 2000-based namespace"
    )]
    pub const fn compose(code: u32) -> Self {
        Self(2000 + code)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Note,
}

/// A single diagnostic with typed context. Rendered at the CLI boundary via
/// miette; libraries emit `Diag`, the binary decides how to show it.
#[derive(Clone, Debug)]
pub struct Diag {
    pub severity: Severity,
    pub code: StableCode,
    pub span: Span,
    pub message: Text,
    pub notes: Box<[Note]>,
}

#[derive(Clone, Debug)]
pub struct Text(pub Box<str>);

impl Text {
    pub fn new(s: impl Into<Box<str>>) -> Self {
        Self(s.into())
    }
}

#[derive(Clone, Debug)]
pub struct Note {
    pub span: Span,
    pub message: Text,
}

impl Diag {
    pub fn new(
        severity: Severity,
        code: StableCode,
        span: Span,
        message: impl Into<Box<str>>,
    ) -> Self {
        Self {
            severity,
            code,
            span,
            message: Text::new(message),
            notes: Box::new([]),
        }
    }

    pub fn with_note(mut self, span: Span, message: impl Into<Box<str>>) -> Self {
        self.notes = {
            let mut v = Vec::from(self.notes);
            v.push(Note {
                span,
                message: Text::new(message),
            });
            v.into_boxed_slice()
        };
        self
    }
}

#[derive(Default, Debug)]
pub struct DiagnosticSink {
    diags: Vec<Diag>,
}

impl DiagnosticSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, diag: Diag) {
        self.diags.push(diag);
    }

    pub fn extend(&mut self, other: DiagnosticSink) {
        self.diags.extend(other.diags);
    }

    pub fn has_errors(&self) -> bool {
        self.diags.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Diag> {
        self.diags.iter()
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diag> {
        self.diags
            .iter()
            .filter(|d| d.severity == Severity::Warning)
    }

    pub fn into_inner(self) -> Vec<Diag> {
        self.diags
    }
}

pub type PithResult<T> = Result<T, DiagnosticSink>;

impl std::fmt::Display for Diag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message.0)
    }
}

impl std::error::Error for Diag {}

impl miette::Diagnostic for Diag {
    fn severity(&self) -> Option<miette::Severity> {
        Some(match self.severity {
            Severity::Error => miette::Severity::Error,
            Severity::Warning => miette::Severity::Warning,
            Severity::Info => miette::Severity::Advice,
            Severity::Note => miette::Severity::Advice,
        })
    }

    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(format!("P{}", self.code.0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_span_is_zero_width() {
        let s = Span::point(ByteOffset(7));
        assert_eq!(s.start, s.end);
    }

    #[test]
    fn sink_distinguishes_errors_from_warnings() {
        let mut sink = DiagnosticSink::new();
        sink.push(Diag::new(
            Severity::Warning,
            StableCode::engine(1),
            Span::none(),
            "minor",
        ));
        assert!(!sink.has_errors());
        sink.push(Diag::new(
            Severity::Error,
            StableCode::engine(2),
            Span::none(),
            "bad",
        ));
        assert!(sink.has_errors());
        assert_eq!(sink.warnings().count(), 1);
    }
}
