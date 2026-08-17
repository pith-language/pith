//! Additional diagnostic and span tests.

use pith_diag::{
    ByteOffset, Diag, DiagnosticSink, EngineCode, Note, Severity, SourceFile, SourceId, Span,
    StableCode, Text,
};

#[test]
fn span_none_is_at_the_origin_and_zero_width() {
    let s = Span::none();
    assert_eq!(s.start, ByteOffset(0));
    assert_eq!(s.end, ByteOffset(0));
    assert_eq!(s.start, s.end);
}

#[test]
fn span_point_is_zero_width_at_the_given_offset() {
    let s = Span::point(ByteOffset(42));
    assert_eq!(s.start, ByteOffset(42));
    assert_eq!(s.end, ByteOffset(42));
}

#[test]
fn source_id_round_trips_through_its_raw_form() {
    for raw in [0u32, 1, u32::MAX, 7] {
        let id = SourceId::from_raw(raw);
        assert_eq!(id.to_raw(), raw);
    }
}

#[test]
fn source_file_exposes_its_text() {
    let file = SourceFile::new(SourceId::from_raw(3), "src/main.pi", "let x = 1\n");
    assert_eq!(file.id, SourceId::from_raw(3));
    assert_eq!(file.label.as_ref(), "src/main.pi");
    assert_eq!(file.source_text(), "let x = 1\n");
}

#[test]
fn line_col_reports_one_based_line_and_column_for_first_line() {
    let file = SourceFile::new(SourceId::from_raw(0), "f", "hello");
    let (line, col) = file.line_col(ByteOffset(0));
    assert_eq!((line, col), (1, 1));
    let (line, col) = file.line_col(ByteOffset(4));
    assert_eq!((line, col), (1, 5));
}

#[test]
fn line_col_advances_across_newlines() {
    let file = SourceFile::new(SourceId::from_raw(0), "f", "ab\ncd\nef");
    // offset 0 -> line 1 col 1
    assert_eq!(file.line_col(ByteOffset(0)), (1, 1));
    // offset 3 -> 'c' on line 2 col 1
    assert_eq!(file.line_col(ByteOffset(3)), (2, 1));
    // offset 6 -> 'e' on line 3 col 1
    assert_eq!(file.line_col(ByteOffset(6)), (3, 1));
}

#[test]
fn line_col_handles_empty_source_text() {
    let file = SourceFile::new(SourceId::from_raw(0), "f", "");
    let (line, col) = file.line_col(ByteOffset(0));
    assert_eq!((line, col), (1, 1));
}

#[test]
fn line_col_handles_trailing_newline() {
    let file = SourceFile::new(SourceId::from_raw(0), "f", "ab\n");
    assert_eq!(file.line_col(ByteOffset(3)), (2, 1));
}

#[test]
fn diag_with_note_appends_a_note_preserving_prior_ones() {
    let base = Diag::engine(EngineCode::DependencyCycle, Span::none(), "cycle");
    let with_one = base.with_note(Span::point(ByteOffset(5)), "first");
    let with_two = with_one.with_note(Span::point(ByteOffset(9)), "second");

    assert_eq!(with_two.notes.len(), 2);
    let messages: Vec<&str> = with_two
        .notes
        .iter()
        .map(|n| n.message.0.as_ref())
        .collect();
    assert_eq!(messages, ["first", "second"]);
    let spans: Vec<Span> = with_two.notes.iter().map(|n| n.span).collect();
    assert_eq!(
        spans,
        [Span::point(ByteOffset(5)), Span::point(ByteOffset(9))]
    );
}

#[test]
fn diag_display_is_its_message() {
    let diag = Diag::engine(EngineCode::StoreError, Span::none(), "store blew up");
    assert_eq!(diag.to_string(), "store blew up");
}

#[test]
fn diag_new_starts_with_no_notes() {
    let diag = Diag::new(
        Severity::Warning,
        StableCode::compose(7),
        Span::none(),
        "minor",
    );
    assert!(diag.notes.is_empty());
    assert_eq!(diag.severity, Severity::Warning);
}

#[test]
fn diagnostic_sink_extend_merges_diagnostics_in_order() {
    let mut a = DiagnosticSink::new();
    a.push(Diag::engine(
        EngineCode::NoRuleForInterface,
        Span::none(),
        "a1",
    ));
    let mut b = DiagnosticSink::new();
    b.push(Diag::engine(EngineCode::AmbiguousRule, Span::none(), "b1"));
    b.push(Diag::engine(
        EngineCode::DependencyCycle,
        Span::none(),
        "b2",
    ));

    a.extend(b);
    let messages: Vec<String> = a.iter().map(|d| d.to_string()).collect();
    assert_eq!(messages, ["a1", "b1", "b2"]);
}

#[test]
fn diagnostic_sink_into_inner_yields_pushed_diagnostics() {
    let mut sink = DiagnosticSink::new();
    sink.push(Diag::engine(
        EngineCode::InternalInvariant,
        Span::none(),
        "x",
    ));
    let diags = sink.into_inner();
    let messages: Vec<String> = diags.iter().map(|d| d.to_string()).collect();
    assert_eq!(messages, ["x"]);
}

#[test]
fn diagnostic_sink_empty_has_no_errors_and_no_warnings() {
    let sink = DiagnosticSink::new();
    assert!(!sink.has_errors());
    assert_eq!(sink.warnings().count(), 0);
    assert_eq!(sink.iter().count(), 0);
}

#[test]
fn diagnostic_sink_warnings_filter_out_errors_and_infos() {
    let mut sink = DiagnosticSink::new();
    sink.push(Diag::new(
        Severity::Info,
        StableCode::compose(1),
        Span::none(),
        "info",
    ));
    sink.push(Diag::new(
        Severity::Warning,
        StableCode::compose(2),
        Span::none(),
        "warn",
    ));
    sink.push(Diag::engine(EngineCode::PolicyDenied, Span::none(), "err"));

    let warnings: Vec<String> = sink.warnings().map(|d| d.to_string()).collect();
    assert_eq!(warnings, ["warn"]);
    assert!(sink.has_errors());
}

#[test]
fn stable_code_compose_reserves_in_the_two_thousand_namespace() {
    assert_eq!(StableCode::compose(0).0, 2000);
    assert_eq!(StableCode::compose(99).0, 2099);
}

#[test]
fn stable_code_from_engine_uses_the_thousand_namespace() {
    assert_eq!(StableCode::from(EngineCode::NoRuleForInterface).0, 1101);
}

#[test]
fn text_new_owns_its_contents() {
    let t = Text::new("hello");
    assert_eq!(t.0.as_ref(), "hello");
}

#[test]
fn note_carries_its_span_and_message() {
    let n = Note {
        span: Span::point(ByteOffset(3)),
        message: Text::new("see here"),
    };
    assert_eq!(n.span, Span::point(ByteOffset(3)));
    assert_eq!(n.message.0.as_ref(), "see here");
}
