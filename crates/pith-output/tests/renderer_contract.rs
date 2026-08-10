//! Renderer acceptance tests, including underlying writer failures.

use std::io::{self, Write};

use pith_output::{
    CacheOutcome, ExplainStep, JsonRenderer, OutputRecord, Payload, PhaseStatus, PlainRenderer,
    PrettyRenderer, RecordKind, Renderer, Sink,
};

#[derive(Default)]
struct FailingWriter {
    fail_write: bool,
    fail_flush: bool,
}

impl Write for FailingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.fail_write {
            Err(io::Error::other("write refused"))
        } else {
            Ok(bytes.len())
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.fail_flush {
            Err(io::Error::other("flush refused"))
        } else {
            Ok(())
        }
    }
}

fn every_record_shape() -> [OutputRecord; 5] {
    [
        OutputRecord::phase("build", PhaseStatus::Finished),
        OutputRecord::cache(CacheOutcome::Miss),
        OutputRecord::explain([ExplainStep {
            label: "dependency".into(),
            detail: "changed".into(),
        }]),
        OutputRecord::result("artifact"),
        OutputRecord::summary(1, 2, 3, 4, 5),
    ]
}

#[test]
fn every_constructor_uses_the_default_success_code() {
    for record in every_record_shape() {
        assert_eq!(record.code, 0);
    }
}

#[test]
fn json_emits_every_shape_as_an_independent_object() {
    let mut bytes = Vec::new();
    {
        let mut renderer = JsonRenderer::new(&mut bytes);
        for record in every_record_shape() {
            assert!(renderer.emit(&record).is_ok());
        }
        assert!(renderer.finish().is_ok());
    }

    let values = String::from_utf8(bytes)
        .ok()
        .map(|text| {
            text.lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let kinds = values
        .iter()
        .filter_map(|value| value.get("kind").and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["phase", "cache", "explain", "result", "summary"]);
}

#[test]
fn json_escapes_multiline_user_text_without_breaking_json_lines() {
    let mut bytes = Vec::new();
    let mut renderer = JsonRenderer::new(&mut bytes);

    assert!(
        renderer
            .emit(&OutputRecord::result("first\nsecond\t\"quoted\""))
            .is_ok()
    );

    let text = String::from_utf8(bytes).unwrap_or_default();
    assert_eq!(text.lines().count(), 1);
    let value = serde_json::from_str::<serde_json::Value>(text.trim()).ok();
    assert_eq!(
        value
            .as_ref()
            .and_then(|value| value.get("summary"))
            .and_then(serde_json::Value::as_str),
        Some("first\nsecond\t\"quoted\"")
    );
}

#[test]
fn plain_renderer_emits_one_terminated_record_per_call() {
    let mut bytes = Vec::new();
    {
        let mut renderer = PlainRenderer::new(&mut bytes);
        for record in every_record_shape() {
            assert!(renderer.emit(&record).is_ok());
        }
    }

    let text = String::from_utf8(bytes).unwrap_or_default();
    assert_eq!(text.lines().count(), 5);
    assert!(text.ends_with('\n'));
    assert!(text.lines().all(|line| line.is_ascii()));
}

#[test]
fn an_empty_explanation_is_still_a_well_formed_record() {
    let record = OutputRecord::explain(Vec::<ExplainStep>::new().into_boxed_slice());

    assert_eq!(record.kind, RecordKind::Explain);
    assert!(matches!(record.payload, Payload::Explain { ref steps } if steps.is_empty()));
}

#[test]
fn plain_renderer_propagates_write_failure() {
    let mut renderer = PlainRenderer::new(FailingWriter {
        fail_write: true,
        fail_flush: false,
    });

    assert_eq!(
        renderer
            .emit(&OutputRecord::result("value"))
            .err()
            .map(|error| error.kind()),
        Some(io::ErrorKind::Other)
    );
}

#[test]
fn json_renderer_propagates_write_failure() {
    let mut renderer = JsonRenderer::new(FailingWriter {
        fail_write: true,
        fail_flush: false,
    });

    assert_eq!(
        renderer
            .emit(&OutputRecord::result("value"))
            .err()
            .map(|error| error.kind()),
        Some(io::ErrorKind::Other)
    );
}

#[test]
fn pretty_renderer_propagates_write_failure() {
    let mut renderer = PrettyRenderer::new(FailingWriter {
        fail_write: true,
        fail_flush: false,
    });

    assert_eq!(
        renderer
            .emit(&OutputRecord::result("value"))
            .err()
            .map(|error| error.kind()),
        Some(io::ErrorKind::Other)
    );
}

#[test]
fn sink_finish_propagates_flush_failure() {
    let writer = FailingWriter {
        fail_write: false,
        fail_flush: true,
    };
    let sink = Sink::new(PlainRenderer::new(writer));

    assert_eq!(
        sink.finish().err().map(|error| error.kind()),
        Some(io::ErrorKind::Other)
    );
}

#[test]
fn sink_emit_propagates_renderer_failure() {
    let writer = FailingWriter {
        fail_write: true,
        fail_flush: false,
    };
    let mut sink = Sink::new(JsonRenderer::new(writer));

    assert!(sink.emit(&OutputRecord::cache(CacheOutcome::Hit)).is_err());
}
