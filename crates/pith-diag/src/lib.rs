//! Spans, diagnostics, and the result type for the pith kernel.
//!
//! Leaf crate: every crate that produces structured output depends on this.
//! `text-size` and `line-index` are wrapped behind [`Span`] / [`SourceFile`]
//! so they never appear in public types outside this module.

use std::sync::{Arc, OnceLock};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ByteOffset(pub u32);

/// A half-open byte range `[start, end)` into a [`SourceFile`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: ByteOffset,
    pub end: ByteOffset,
}

impl Span {
    pub const fn new(start: ByteOffset, end: ByteOffset) -> Self {
        Self { start, end }
    }

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

    line_index: OnceLock<line_index::LineIndex>,
}

/// One line of a [`SourceFile`]: its 1-based number, the span of its content
/// without the terminator, and the content itself.
#[derive(Clone, Copy, Debug)]
pub struct FileLine<'a> {
    pub number: usize,
    pub span: Span,
    pub text: &'a str,
}

impl SourceFile {
    pub fn new(id: SourceId, label: impl Into<Box<str>>, text: impl Into<Box<str>>) -> Self {
        Self {
            id,
            label: label.into(),
            text: text.into(),
            line_index: OnceLock::new(),
        }
    }

    pub fn source_text(&self) -> &str {
        &self.text
    }

    /// The span of a slice of this file's text. The slice must come from
    /// [`SourceFile::source_text`] — a slice of some other string, including
    /// the one this file was built from, yields a meaningless span, silently.
    pub fn span_of(&self, slice: &str) -> Span {
        let text = self.text.as_ptr() as usize;
        let start = (slice.as_ptr() as usize).saturating_sub(text);
        let end = start.saturating_add(slice.len());
        debug_assert_eq!(
            self.text.get(start..end),
            Some(slice),
            "span_of was handed a slice of some other string"
        );
        Span {
            start: offset_from(start),
            end: offset_from(end),
        }
    }

    /// The file's lines in order, numbered from 1, with `\n` and a preceding
    /// `\r` stripped from the text and excluded from the span — the same
    /// line split `str::lines` performs, with the positions kept.
    pub fn lines(&self) -> Lines<'_> {
        Lines {
            inner: self.text.split_inclusive('\n'),
            offset: 0,
            number: 0,
        }
    }

    pub fn line_col(&self, offset: ByteOffset) -> (usize, usize) {
        let index = self
            .line_index
            .get_or_init(|| line_index::LineIndex::new(self.source_text()));
        let pos = line_index::TextSize::new(offset.0);
        let line_col = index.line_col(pos);
        (
            line_col.line.saturating_add(1) as usize,
            line_col.col.saturating_add(1) as usize,
        )
    }
}

fn offset_from(at: usize) -> ByteOffset {
    ByteOffset(u32::try_from(at).unwrap_or(u32::MAX))
}

/// The iterator behind [`SourceFile::lines`].
#[derive(Clone, Debug)]
pub struct Lines<'a> {
    inner: std::str::SplitInclusive<'a, char>,
    offset: usize,
    number: usize,
}

impl<'a> Iterator for Lines<'a> {
    type Item = FileLine<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let raw = self.inner.next()?;
        let text = raw.strip_suffix('\n').unwrap_or(raw);
        let text = text.strip_suffix('\r').unwrap_or(text);
        self.number = self.number.saturating_add(1);
        let start = self.offset;
        self.offset = self.offset.saturating_add(raw.len());
        let end = start.saturating_add(text.len());
        Some(FileLine {
            number: self.number,
            span: Span {
                start: offset_from(start),
                end: offset_from(end),
            },
            text,
        })
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
    #[doc(hidden)]
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "engine diagnostic codes occupy the stable 1000-based namespace"
    )]
    pub const fn from_engine_code(code: EngineCode) -> Self {
        Self(1000 + code as u32)
    }

    /// Reserve a code in the stable 2000-based composition namespace. Unused
    /// today; kept so composition diagnostics can claim codes without touching
    /// the engine namespace later.
    #[doc(hidden)]
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "composition diagnostic codes occupy the stable 2000-based namespace"
    )]
    pub const fn compose(code: u32) -> Self {
        Self(2000 + code)
    }

    /// Reserve a code in the stable 3000-based frontend namespace, which the
    /// language surface's lexer, parser, and elaborator occupy. Codes are
    /// allocated by the frontend crates that own the diagnostics, never
    /// renumbered, only appended, on the same terms as the engine namespace.
    #[doc(hidden)]
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "frontend diagnostic codes occupy the stable 3000-based namespace"
    )]
    pub const fn frontend(code: u32) -> Self {
        Self(3000 + code)
    }
}

/// Named engine diagnostic codes. The discriminant is the stable `n` in
/// `E-{1000 + n}`; never renumber, only append (requirement K-11).
///
/// This is the single source of truth for engine codes: every diagnostic the
/// kernel emits names its variant here, so the code, its number, and a label
/// for the variant live together.
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum EngineCode {
    /// `E-1101` — no rule provides the requested interface.
    NoRuleForInterface = 101,
    /// `E-1102` — more than one rule provides the interface; ambiguity is never ranked.
    AmbiguousRule = 102,
    /// `E-1103` — a request's inputs do not match its declared interface.
    RequestInputsMismatch = 103,
    /// `E-1104` — a rule or action returned a value of the wrong type.
    ResultTypeMismatch = 104,
    /// `E-1105` — a declared action contract is invalid.
    InvalidActionSpec = 105,
    /// `E-1203` — the dependency graph contains a cycle.
    DependencyCycle = 203,
    /// `E-1204` — an engine-internal invariant was violated.
    InternalInvariant = 204,
    /// `E-1205` — requested content is not available in the local store.
    ContentUnavailable = 205,
    /// `E-1206` — an effectful step appeared in a pure-only evaluation.
    EffectfulStepInPure = 206,
    /// `E-1207` — the content store returned an error.
    StoreError = 207,
    /// `E-1208` — the executor reported use of an undeclared capability.
    UndeclaredCapabilityUse = 208,
    /// `E-1209` — the executor reported an output outside the declared contract
    /// or with the wrong kind.
    UndeclaredOutput = 209,
    /// `E-1210` — the executor did not produce a declared output.
    MissingDeclaredOutput = 210,
    /// `E-1212` — the executor did not report a concrete platform, or reported
    /// one outside the declared requirement.
    PlatformMismatch = 212,
    /// `E-1213` — the action policy denied the planned action.
    PolicyDenied = 213,
    /// `E-1214` — an attempt was left `Pending` when its owner stopped, and was
    /// marked cancelled on reopen rather than resumed. Not a fault: it records
    /// why the attempt has no result, not that computing one would fail.
    InterruptedAttempt = 214,
    /// `E-1215` — the caller cancelled the run. Not a fault: the work that was
    /// in flight is recorded as cancelled, and re-running it is reasonable.
    RunCancelled = 215,
    /// `E-1216` — the run exceeded a bound its caller declared: a wall-clock
    /// deadline or a step budget. Not a fault of any computation the run was
    /// merely holding, and re-running under a larger bound is reasonable; the
    /// action that exceeded a wall clock did run and produced nothing within
    /// the authority it was given, so its own attempt is a failure carrying
    /// this code.
    RunBoundExceeded = 216,
    /// `E-1217` — a rule body requested an observation and the engine has no
    /// observer configured. The observation effect cannot be served by the
    /// engine itself; a host supplies the adapter (decision 0060).
    ObserverMissing = 217,
    /// `E-1218` — a represented rule body produced a declared value failure.
    RepresentedBodyFailed = 218,
    /// `E-1219` — action planning was requested for an entry whose pure
    /// evaluation completed without reaching an action.
    EntryHasNoAction = 219,
}

impl From<EngineCode> for StableCode {
    fn from(code: EngineCode) -> Self {
        Self::from_engine_code(code)
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
///
/// A diagnostic that knows the text it talks about carries it in `source`,
/// and its span and its notes' spans index that text. One produced away from
/// any text — engine evaluation, durable reads — carries none, and renders
/// without a snippet. The source is context for whoever renders it; it is
/// not part of the diagnostic's identity and does not persist.
#[derive(Clone, Debug)]
pub struct Diag {
    pub severity: Severity,
    pub code: StableCode,
    pub span: Span,
    pub source: Option<Arc<SourceFile>>,
    pub message: Text,
    pub notes: Vec<Note>,
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
            source: None,
            message: Text::new(message),
            notes: Vec::new(),
        }
    }

    /// Build an engine error diagnostic from its named code. Every kernel
    /// diagnostic today is an error, so this is the usual construction path.
    pub fn engine(code: EngineCode, span: Span, message: impl Into<Box<str>>) -> Self {
        Self::new(Severity::Error, code.into(), span, message)
    }

    /// Attaches the text the span and its notes index.
    pub fn with_source(mut self, source: Arc<SourceFile>) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_note(mut self, span: Span, message: impl Into<Box<str>>) -> Self {
        self.notes.push(Note {
            span,
            message: Text::new(message),
        });
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

impl miette::SourceCode for SourceFile {
    fn read_span<'a>(
        &'a self,
        span: &miette::SourceSpan,
        context_lines_before: usize,
        context_lines_after: usize,
    ) -> Result<Box<dyn miette::SpanContents<'a> + 'a>, miette::MietteError> {
        let contents = miette::SourceCode::read_span(
            self.text.as_ref(),
            span,
            context_lines_before,
            context_lines_after,
        )?;
        Ok(Box::new(miette::MietteSpanContents::new_named(
            self.label.to_string(),
            contents.data(),
            *contents.span(),
            contents.line(),
            contents.column(),
            contents.line_count(),
        )))
    }
}

fn miette_span(span: Span) -> miette::SourceSpan {
    let start = usize::try_from(span.start.0).unwrap_or(usize::MAX);
    let length = usize::try_from(span.end.0.saturating_sub(span.start.0)).unwrap_or(usize::MAX);
    miette::SourceSpan::new(miette::SourceOffset::from(start), length)
}

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
        Some(Box::new(format!("E-{}", self.code.0)))
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        self.source
            .as_deref()
            .map(|file| file as &dyn miette::SourceCode)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        self.source.as_ref()?;
        let primary = miette::LabeledSpan::underline(miette_span(self.span));
        let notes = self.notes.iter().map(|note| {
            miette::LabeledSpan::new_with_span(
                Some(note.message.0.to_string()),
                miette_span(note.span),
            )
        });
        Some(Box::new(std::iter::once(primary).chain(notes)))
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
            StableCode::compose(1),
            Span::none(),
            "minor",
        ));
        assert!(!sink.has_errors());
        sink.push(Diag::engine(
            EngineCode::ResultTypeMismatch,
            Span::none(),
            "bad",
        ));
        assert!(sink.has_errors());
        assert_eq!(sink.warnings().count(), 1);
    }

    #[test]
    fn engine_code_discriminants_are_stable() {
        // K-11 stability: these numbers are the public contract. The match
        // names every variant with no wildcard, so a new variant fails to
        // compile until its number is pinned here.
        for code in [
            EngineCode::NoRuleForInterface,
            EngineCode::AmbiguousRule,
            EngineCode::RequestInputsMismatch,
            EngineCode::ResultTypeMismatch,
            EngineCode::InvalidActionSpec,
            EngineCode::DependencyCycle,
            EngineCode::InternalInvariant,
            EngineCode::ContentUnavailable,
            EngineCode::EffectfulStepInPure,
            EngineCode::StoreError,
            EngineCode::UndeclaredCapabilityUse,
            EngineCode::UndeclaredOutput,
            EngineCode::MissingDeclaredOutput,
            EngineCode::PlatformMismatch,
            EngineCode::PolicyDenied,
            EngineCode::InterruptedAttempt,
            EngineCode::RunCancelled,
            EngineCode::RunBoundExceeded,
            EngineCode::ObserverMissing,
            EngineCode::RepresentedBodyFailed,
            EngineCode::EntryHasNoAction,
        ] {
            let pinned = match code {
                EngineCode::NoRuleForInterface => 1101,
                EngineCode::AmbiguousRule => 1102,
                EngineCode::RequestInputsMismatch => 1103,
                EngineCode::ResultTypeMismatch => 1104,
                EngineCode::InvalidActionSpec => 1105,
                EngineCode::DependencyCycle => 1203,
                EngineCode::InternalInvariant => 1204,
                EngineCode::ContentUnavailable => 1205,
                EngineCode::EffectfulStepInPure => 1206,
                EngineCode::StoreError => 1207,
                EngineCode::UndeclaredCapabilityUse => 1208,
                EngineCode::UndeclaredOutput => 1209,
                EngineCode::MissingDeclaredOutput => 1210,
                EngineCode::PlatformMismatch => 1212,
                EngineCode::PolicyDenied => 1213,
                EngineCode::InterruptedAttempt => 1214,
                EngineCode::RunCancelled => 1215,
                EngineCode::RunBoundExceeded => 1216,
                EngineCode::ObserverMissing => 1217,
                EngineCode::RepresentedBodyFailed => 1218,
                EngineCode::EntryHasNoAction => 1219,
            };
            assert_eq!(StableCode::from(code).0, pinned);
        }
    }

    #[test]
    fn engine_diag_is_an_error_with_named_code() {
        let diag = Diag::engine(EngineCode::DependencyCycle, Span::none(), "cyclical");
        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.code, EngineCode::DependencyCycle.into());
    }

    #[test]
    fn span_of_names_a_slice_of_the_file() {
        let text = "lock-version 1\nresolver blake3:00\n";
        let file = SourceFile::new(SourceId::from_raw(0), "pith.lock", text);
        let word = file.source_text().get(16..23).unwrap();
        let span = file.span_of(word);
        assert_eq!(span.start, ByteOffset(16));
        assert_eq!(span.end, ByteOffset(23));
        assert_eq!(file.source_text().get(16..23), Some(word));
    }

    #[test]
    fn lines_match_str_lines_and_carry_spans_without_terminators() {
        let text = "one\ntwo\r\n\nthree";
        let file = SourceFile::new(SourceId::from_raw(0), "f", text);
        let lines: Vec<FileLine> = file.lines().collect();
        let plain: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines.iter().map(|line| line.text).collect::<Vec<_>>(),
            plain
        );
        for (index, line) in lines.iter().enumerate() {
            assert_eq!(line.number, index + 1);
            assert_eq!(
                text.get(line.span.start.0 as usize..line.span.end.0 as usize),
                Some(line.text),
                "each span selects its line without the terminator"
            );
        }
        assert_eq!(
            lines.first().map(|line| line.span),
            Some(Span::new(ByteOffset(0), ByteOffset(3)))
        );
        assert_eq!(
            lines.get(1).map(|line| line.span),
            Some(Span::new(ByteOffset(4), ByteOffset(4 + 3)))
        );
        assert_eq!(
            lines.get(2).map(|line| line.span),
            Some(Span::new(ByteOffset(9), ByteOffset(9)))
        );
        assert_eq!(
            lines.get(3).map(|line| line.span.start),
            Some(ByteOffset(10))
        );
    }

    #[test]
    fn an_attached_source_renders_its_label_line_and_column() {
        let text = "lock-version 1\nresolver not-a-digest\n";
        let file = SourceFile::new(SourceId::from_raw(0), "pith.lock", text);
        let bad = file.span_of(file.source_text().get(24..37).unwrap());
        let diag = Diag::new(
            Severity::Error,
            StableCode::compose(1),
            bad,
            "the resolver is not a digest",
        )
        .with_source(std::sync::Arc::new(file));
        let handler = miette::GraphicalReportHandler::new();
        let report = miette::Report::new(diag);
        let mut rendered = String::new();
        handler
            .render_report(&mut rendered, report.as_ref())
            .unwrap();
        assert!(
            rendered.contains("pith.lock:2:10"),
            "the rendered header names the source, line, and column: {rendered}"
        );
        assert!(
            rendered.contains("not-a-digest"),
            "the rendered snippet quotes the spanned text: {rendered}"
        );
    }

    #[test]
    fn a_note_becomes_a_second_label_in_the_rendered_report() {
        let file = SourceFile::new(SourceId::from_raw(0), "f", "first\nsecond\n");
        let (first, second) = (
            file.span_of(file.source_text().get(0..5).unwrap()),
            file.span_of(file.source_text().get(6..12).unwrap()),
        );
        let file = std::sync::Arc::new(file);
        let diag = Diag::new(
            Severity::Error,
            StableCode::compose(2),
            second,
            "the second line",
        )
        .with_source(std::sync::Arc::clone(&file))
        .with_note(first, "the first line");
        let handler = miette::GraphicalReportHandler::new();
        let report = miette::Report::new(diag);
        let mut rendered = String::new();
        handler
            .render_report(&mut rendered, report.as_ref())
            .unwrap();
        assert!(
            rendered.contains("first") && rendered.contains("second"),
            "both the primary span and the note's span render: {rendered}"
        );
    }
}
