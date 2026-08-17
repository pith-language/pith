use pith_diag::{Diag, DiagnosticSink, Severity, Span, StableCode};

pub fn fixture_error(message: &str) -> DiagnosticSink {
    let mut diagnostics = DiagnosticSink::new();
    diagnostics.push(Diag::new(
        Severity::Error,
        StableCode(1211),
        Span::none(),
        message,
    ));
    diagnostics
}
