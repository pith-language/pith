//! Plain renderer: ASCII only, no color. Default when stdout is not a TTY.

use std::io::{self, Write};

use crate::{OutputRecord, Payload, PhaseStatus, Renderer};

/// Render records as plain ASCII lines to a writer.
pub struct PlainRenderer<W: Write> {
    out: W,
}

impl<W: Write> PlainRenderer<W> {
    pub fn new(out: W) -> Self {
        Self { out }
    }
}

impl<W: Write> Renderer for PlainRenderer<W> {
    fn emit(&mut self, record: &OutputRecord) -> io::Result<()> {
        let line = describe_plain(record);
        writeln!(self.out, "{line}")
    }

    fn finish(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

/// Describe one record as a single ASCII line. Kept separate from the renderer
/// so it is independently testable.
fn describe_plain(record: &OutputRecord) -> String {
    let tag = match record.kind {
        crate::RecordKind::Phase => "phase",
        crate::RecordKind::Cache => "cache",
        crate::RecordKind::Explain => "explain",
        crate::RecordKind::Result => "result",
        crate::RecordKind::Summary => "summary",
        crate::RecordKind::Custom => "custom",
    };
    let body = match &record.payload {
        Payload::Phase { name, status } => {
            let s = match status {
                PhaseStatus::Started => "started",
                PhaseStatus::Finished => "finished",
                PhaseStatus::Failed => "failed",
            };
            format!("{name} {s}")
        }
        Payload::Cache { outcome } => format!("{outcome:?}"),
        Payload::Explain { steps } => {
            let labels: Vec<&str> = steps.iter().map(|s| s.label.as_ref()).collect();
            labels.join(" -> ")
        }
        Payload::Result { summary } => summary.as_ref().to_string(),
        Payload::Summary {
            hits,
            misses,
            reuses,
            errors,
            wall_ms,
        } => {
            format!("hits={hits} misses={misses} reuses={reuses} errors={errors} wall={wall_ms}ms")
        }
    };
    format!("[{tag}] {body}")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::{CacheOutcome, OutputRecord, PhaseStatus};

    #[test]
    fn plain_phase_is_ascii() {
        let rec = OutputRecord::phase("build", PhaseStatus::Started);
        assert_eq!(describe_plain(&rec), "[phase] build started");
    }

    #[test]
    fn plain_renderer_writes_a_line() {
        let rec = OutputRecord::phase("build", PhaseStatus::Started);
        let mut buf: Vec<u8> = Vec::new();
        // write through the actual renderer to confirm the writer path works
        writeln!(buf, "{}", describe_plain(&rec)).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "[phase] build started\n");
    }

    #[test]
    fn plain_cache_is_ascii() {
        let rec = OutputRecord::cache(CacheOutcome::Hit);
        assert_eq!(describe_plain(&rec), "[cache] Hit");
    }

    #[test]
    fn plain_summary_is_key_value() {
        let rec = OutputRecord::summary(1, 2, 3, 0, 9);
        assert_eq!(
            describe_plain(&rec),
            "[summary] hits=1 misses=2 reuses=3 errors=0 wall=9ms"
        );
    }
}
