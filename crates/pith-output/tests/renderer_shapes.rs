//! Shape coverage for every payload variant across all three renderers.

use pith_output::{
    CacheOutcome, ExplainStep, JsonRenderer, OutputRecord, Payload, PhaseStatus, PlainRenderer,
    PrettyRenderer, RecordKind, Renderer,
};

fn every_phase_status() -> [(OutputRecord, &'static str); 3] {
    [
        (
            OutputRecord::phase("build", PhaseStatus::Started),
            "started",
        ),
        (
            OutputRecord::phase("build", PhaseStatus::Finished),
            "finished",
        ),
        (OutputRecord::phase("build", PhaseStatus::Failed), "failed"),
    ]
}

fn every_cache_outcome() -> [(OutputRecord, &'static str); 3] {
    [
        (OutputRecord::cache(CacheOutcome::Hit), "hit"),
        (OutputRecord::cache(CacheOutcome::Miss), "miss"),
        (OutputRecord::cache(CacheOutcome::Reuse), "reuse"),
    ]
}

fn render_plain(record: &OutputRecord) -> String {
    let mut buf = Vec::new();
    let mut renderer = PlainRenderer::new(&mut buf);
    assert!(renderer.emit(record).is_ok());
    String::from_utf8(buf).unwrap_or_default()
}

fn render_pretty(record: &OutputRecord) -> String {
    let mut buf = Vec::new();
    let mut renderer = PrettyRenderer::new(&mut buf);
    assert!(renderer.emit(record).is_ok());
    String::from_utf8(buf).unwrap_or_default()
}

fn render_json(record: &OutputRecord) -> serde_json::Value {
    let mut buf = Vec::new();
    let mut renderer = JsonRenderer::new(&mut buf);
    assert!(renderer.emit(record).is_ok());
    assert!(renderer.finish().is_ok());
    let text = String::from_utf8(buf).unwrap_or_default();
    let line = text.trim_end_matches('\n');
    assert_eq!(
        line.lines().count(),
        1,
        "JSON renderer emitted more than one line"
    );
    serde_json::from_str(line).unwrap_or_default()
}

#[test]
fn plain_phase_renders_every_status_with_its_stable_word() {
    for (record, word) in every_phase_status() {
        assert_eq!(render_plain(&record), format!("[phase] build {word}\n"));
    }
}

#[test]
fn plain_cache_renders_every_outcome_with_its_stable_word() {
    for (record, word) in every_cache_outcome() {
        assert_eq!(render_plain(&record), format!("[cache] {word}\n"));
    }
}

#[test]
fn plain_explain_joins_labels_with_arrows() {
    let record = OutputRecord::explain([
        ExplainStep {
            label: "fetch".into(),
            detail: "remote".into(),
        },
        ExplainStep {
            label: "build".into(),
            detail: "local".into(),
        },
        ExplainStep {
            label: "publish".into(),
            detail: "store".into(),
        },
    ]);
    assert_eq!(
        render_plain(&record),
        "[explain] fetch -> build -> publish\n"
    );
}

#[test]
fn plain_explain_with_no_steps_renders_an_empty_chain() {
    let record = OutputRecord::explain(Vec::<ExplainStep>::new().into_boxed_slice());
    assert_eq!(render_plain(&record), "[explain] \n");
}

#[test]
fn plain_result_renders_the_summary_verbatim() {
    let record = OutputRecord::result("artifact built");
    assert_eq!(render_plain(&record), "[result] artifact built\n");
}

#[test]
fn plain_summary_renders_every_counter() {
    let record = OutputRecord::summary(10, 20, 30, 40, 500);
    assert_eq!(
        render_plain(&record),
        "[summary] hits=10 misses=20 reuses=30 errors=40 wall=500ms\n"
    );
}

#[test]
fn pretty_phase_renders_every_status_glyph() {
    for (record, glyph) in [
        (OutputRecord::phase("build", PhaseStatus::Started), "•"),
        (OutputRecord::phase("build", PhaseStatus::Finished), "✓"),
        (OutputRecord::phase("build", PhaseStatus::Failed), "✗"),
    ] {
        let line = render_pretty(&record);
        assert!(line.starts_with(glyph), "{line:?}");
        assert!(line.contains("build"), "{line:?}");
    }
}

#[test]
fn pretty_summary_reports_ok_when_there_are_no_errors() {
    let record = OutputRecord::summary(1, 0, 0, 0, 9);
    let line = render_pretty(&record);
    assert!(line.contains("ok"), "{line:?}");
    assert!(!line.contains("errors"), "{line:?}");
}

#[test]
fn pretty_summary_reports_errors_when_present() {
    let record = OutputRecord::summary(0, 1, 0, 2, 9);
    let line = render_pretty(&record);
    assert!(line.contains("errors"), "{line:?}");
}

#[test]
fn pretty_explain_renders_every_step_label_and_detail() {
    let record = OutputRecord::explain([
        ExplainStep {
            label: "fetch".into(),
            detail: "remote store".into(),
        },
        ExplainStep {
            label: "build".into(),
            detail: "local toolchain".into(),
        },
    ]);
    let line = render_pretty(&record);
    assert!(line.contains("fetch"), "{line:?}");
    assert!(line.contains("remote store"), "{line:?}");
    assert!(line.contains("build"), "{line:?}");
    assert!(line.contains("local toolchain"), "{line:?}");
}

#[test]
fn json_phase_serializes_name_and_status() {
    let record = OutputRecord::phase("build", PhaseStatus::Failed);
    let value = render_json(&record);
    assert_eq!(value.get("kind").and_then(|v| v.as_str()), Some("phase"));
    assert_eq!(value.get("name").and_then(|v| v.as_str()), Some("build"));
    assert_eq!(value.get("status").and_then(|v| v.as_str()), Some("failed"));
    assert_eq!(value.get("code").and_then(|v| v.as_u64()), Some(0));
}

#[test]
fn json_explain_serializes_steps_array() {
    let record = OutputRecord::explain([
        ExplainStep {
            label: "a".into(),
            detail: "x".into(),
        },
        ExplainStep {
            label: "b".into(),
            detail: "y".into(),
        },
    ]);
    let value = render_json(&record);
    let steps = value.get("steps").and_then(|v| v.as_array());
    assert_eq!(steps.map(|s| s.len()), Some(2));
    let first = steps.and_then(|s| s.first());
    assert_eq!(
        first.and_then(|v| v.get("label")).and_then(|v| v.as_str()),
        Some("a")
    );
    assert_eq!(
        first.and_then(|v| v.get("detail")).and_then(|v| v.as_str()),
        Some("x")
    );
}

#[test]
fn json_summary_serializes_every_counter() {
    let record = OutputRecord::summary(1, 2, 3, 4, 5);
    let value = render_json(&record);
    assert_eq!(value.get("kind").and_then(|v| v.as_str()), Some("summary"));
    assert_eq!(value.get("hits").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(value.get("misses").and_then(|v| v.as_u64()), Some(2));
    assert_eq!(value.get("reuses").and_then(|v| v.as_u64()), Some(3));
    assert_eq!(value.get("errors").and_then(|v| v.as_u64()), Some(4));
    assert_eq!(value.get("wall_ms").and_then(|v| v.as_u64()), Some(5));
}

#[test]
fn json_result_serializes_the_summary_string() {
    let record = OutputRecord::result("done");
    let value = render_json(&record);
    assert_eq!(value.get("kind").and_then(|v| v.as_str()), Some("result"));
    assert_eq!(value.get("summary").and_then(|v| v.as_str()), Some("done"));
}

#[test]
fn json_cache_serializes_every_outcome() {
    for (record, word) in every_cache_outcome() {
        let value = render_json(&record);
        assert_eq!(value.get("kind").and_then(|v| v.as_str()), Some("cache"));
        assert_eq!(value.get("outcome").and_then(|v| v.as_str()), Some(word));
    }
}

#[test]
fn finish_is_a_no_op_for_an_empty_renderer_stream() {
    let mut buf = Vec::new();
    let mut renderer = PlainRenderer::new(&mut buf);
    assert!(renderer.finish().is_ok());
    assert!(buf.is_empty());
}

#[test]
fn payload_record_kind_covers_every_constructor_kind() {
    let records = every_record_set();
    let mut kinds = Vec::new();
    for record in records {
        assert_eq!(record.kind, record.payload.record_kind());
        kinds.push(record.kind);
    }
    assert_eq!(
        kinds,
        [
            RecordKind::Phase,
            RecordKind::Cache,
            RecordKind::Explain,
            RecordKind::Result,
            RecordKind::Summary,
        ]
    );
}

#[test]
fn payload_record_kind_is_exhaustive_over_the_enum() {
    let kinds = [
        Payload::Phase {
            name: "p".into(),
            status: PhaseStatus::Started,
        },
        Payload::Cache {
            outcome: CacheOutcome::Hit,
        },
        Payload::Explain {
            steps: Box::new([]),
        },
        Payload::Result {
            summary: "r".into(),
        },
        Payload::Summary {
            hits: 0,
            misses: 0,
            reuses: 0,
            errors: 0,
            wall_ms: 0,
        },
    ]
    .into_iter()
    .map(|payload| payload.record_kind());
    let mut seen = Vec::new();
    for kind in kinds {
        assert!(
            !seen.contains(&kind),
            "two Payload variants mapped to the same RecordKind"
        );
        seen.push(kind);
    }
}

fn every_record_set() -> [OutputRecord; 5] {
    [
        OutputRecord::phase("build", PhaseStatus::Started),
        OutputRecord::cache(CacheOutcome::Hit),
        OutputRecord::explain([ExplainStep {
            label: "x".into(),
            detail: "y".into(),
        }]),
        OutputRecord::result("done"),
        OutputRecord::summary(0, 0, 0, 0, 0),
    ]
}
