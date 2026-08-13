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

    /// Build an engine error diagnostic from its named code. Every kernel
    /// diagnostic today is an error, so this is the usual construction path.
    pub fn engine(code: EngineCode, span: Span, message: impl Into<Box<str>>) -> Self {
        Self::new(Severity::Error, code.into(), span, message)
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
        Some(Box::new(format!("E-{}", self.code.0)))
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
        // K-11 stability: these numbers are the public contract.
        assert_eq!(StableCode::from(EngineCode::NoRuleForInterface).0, 1101);
        assert_eq!(StableCode::from(EngineCode::AmbiguousRule).0, 1102);
        assert_eq!(StableCode::from(EngineCode::RequestInputsMismatch).0, 1103);
        assert_eq!(StableCode::from(EngineCode::ResultTypeMismatch).0, 1104);
        assert_eq!(StableCode::from(EngineCode::InvalidActionSpec).0, 1105);
        assert_eq!(StableCode::from(EngineCode::DependencyCycle).0, 1203);
        assert_eq!(StableCode::from(EngineCode::InternalInvariant).0, 1204);
        assert_eq!(StableCode::from(EngineCode::ContentUnavailable).0, 1205);
        assert_eq!(StableCode::from(EngineCode::EffectfulStepInPure).0, 1206);
        assert_eq!(StableCode::from(EngineCode::StoreError).0, 1207);
        assert_eq!(
            StableCode::from(EngineCode::UndeclaredCapabilityUse).0,
            1208
        );
        assert_eq!(StableCode::from(EngineCode::UndeclaredOutput).0, 1209);
        assert_eq!(StableCode::from(EngineCode::MissingDeclaredOutput).0, 1210);
        assert_eq!(StableCode::from(EngineCode::PlatformMismatch).0, 1212);
        assert_eq!(StableCode::from(EngineCode::PolicyDenied).0, 1213);
        assert_eq!(StableCode::from(EngineCode::InterruptedAttempt).0, 1214);
    }

    #[test]
    fn engine_diag_is_an_error_with_named_code() {
        let diag = Diag::engine(EngineCode::DependencyCycle, Span::none(), "cyclical");
        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.code, EngineCode::DependencyCycle.into());
    }
}
