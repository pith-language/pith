//! Public diagnostic-contract tests for stable codes and source locations (K-11).

use miette::Diagnostic as _;
use pith_diag::{
    ByteOffset, Diag, DiagnosticSink, EngineCode, Severity, SourceFile, SourceId, Span, StableCode,
};

fn diagnostic(severity: Severity, code: u32, message: &str) -> Diag {
    Diag::new(severity, StableCode(code), Span::none(), message)
}

#[test]
fn source_ids_round_trip_every_raw_value() {
    for raw in [0, 1, u32::MAX] {
        assert_eq!(SourceId::from_raw(raw).to_raw(), raw);
    }
}

#[test]
fn source_file_preserves_label_and_text() {
    let source = SourceFile::new(SourceId::from_raw(7), "module.pi", "value = 1\n");

    assert_eq!(source.id.to_raw(), 7);
    assert_eq!(source.label.as_ref(), "module.pi");
    assert_eq!(source.source_text(), "value = 1\n");
}

#[test]
fn line_and_column_are_one_based() {
    let source = SourceFile::new(SourceId::from_raw(1), "test", "abc\ndef");

    assert_eq!(source.line_col(ByteOffset(0)), (1, 1));
    assert_eq!(source.line_col(ByteOffset(2)), (1, 3));
    assert_eq!(source.line_col(ByteOffset(4)), (2, 1));
    assert_eq!(source.line_col(ByteOffset(7)), (2, 4));
}

#[test]
fn columns_follow_the_byte_based_span_model_for_utf8() {
    let source = SourceFile::new(SourceId::from_raw(1), "utf8", "éx\n世界");

    assert_eq!(source.line_col(ByteOffset(2)), (1, 3));
    assert_eq!(source.line_col(ByteOffset(4)), (2, 1));
    assert_eq!(source.line_col(ByteOffset(7)), (2, 4));
}

#[test]
fn none_is_the_zero_point_span() {
    assert_eq!(Span::none(), Span::point(ByteOffset(0)));
}

#[test]
fn notes_are_appended_in_call_order() {
    let diag = Diag::engine(EngineCode::AmbiguousRule, Span::none(), "ambiguous")
        .with_note(Span::point(ByteOffset(3)), "first")
        .with_note(Span::point(ByteOffset(9)), "second");

    let messages = diag
        .notes
        .iter()
        .map(|note| note.message.0.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(messages, ["first", "second"]);
    assert_eq!(
        diag.notes.first().map(|note| note.span.start),
        Some(ByteOffset(3))
    );
    assert_eq!(
        diag.notes.get(1).map(|note| note.span.start),
        Some(ByteOffset(9))
    );
}

#[test]
fn display_is_the_human_message_without_losing_structured_fields() {
    let diag = Diag::new(
        Severity::Info,
        StableCode(42),
        Span::point(ByteOffset(5)),
        "human detail",
    );

    assert_eq!(diag.to_string(), "human detail");
    assert_eq!(diag.code, StableCode(42));
    assert_eq!(diag.span, Span::point(ByteOffset(5)));
}

#[test]
fn miette_codes_use_the_stable_p_prefix() {
    let diag = Diag::engine(EngineCode::PolicyDenied, Span::none(), "denied");

    assert_eq!(
        diag.code().map(|code| code.to_string()),
        Some("E-1213".to_string())
    );
}

#[test]
fn every_non_error_severity_maps_to_the_documented_miette_level() {
    for (severity, expected) in [
        (Severity::Error, miette::Severity::Error),
        (Severity::Warning, miette::Severity::Warning),
        (Severity::Info, miette::Severity::Advice),
        (Severity::Note, miette::Severity::Advice),
    ] {
        assert_eq!(
            diagnostic(severity, 1, "message").severity(),
            Some(expected)
        );
    }
}

#[test]
fn an_empty_sink_has_no_errors_warnings_or_items() {
    let sink = DiagnosticSink::new();

    assert!(!sink.has_errors());
    assert_eq!(sink.warnings().count(), 0);
    assert_eq!(sink.iter().count(), 0);
}

#[test]
fn extending_a_sink_preserves_order_and_warning_filtering() {
    let mut first = DiagnosticSink::new();
    first.push(diagnostic(Severity::Info, 1, "info"));
    let mut second = DiagnosticSink::new();
    second.push(diagnostic(Severity::Warning, 2, "warning"));
    second.push(diagnostic(Severity::Error, 3, "error"));

    first.extend(second);

    assert!(first.has_errors());
    assert_eq!(first.warnings().count(), 1);
    let messages = first
        .into_inner()
        .into_iter()
        .map(|diag| diag.message.0)
        .collect::<Vec<_>>();
    assert_eq!(messages, ["info".into(), "warning".into(), "error".into()]);
}

#[test]
fn composition_codes_live_in_their_reserved_namespace() {
    assert_eq!(StableCode::compose(0), StableCode(2000));
    assert_eq!(StableCode::compose(999), StableCode(2999));
}
