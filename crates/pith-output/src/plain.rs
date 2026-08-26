//! Plain renderer: ASCII only, no color. Default when stdout is not a TTY.

use std::io::{self, Write};

use crate::{OutputRecord, Payload, Renderer};

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

    fn write_raw(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.out.write_all(bytes)
    }

    fn finish(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

/// Describe one record as a single ASCII line. Kept separate from the renderer
/// so it is independently testable.
fn describe_plain(record: &OutputRecord) -> String {
    let tag = record.kind.as_str();
    let body = match &record.payload {
        Payload::Phase { name, status } => {
            format!("{name} {}", status.as_str())
        }
        Payload::Cache { outcome } => outcome.as_str().to_string(),
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
        Payload::Query { query, .. } => crate::describe::query_view(query).render(None),
    };
    body.split('\n')
        .map(|line| format!("[{tag}] {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
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
        writeln!(buf, "{}", describe_plain(&rec)).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "[phase] build started\n");
    }

    #[test]
    fn plain_cache_matches_serde_snake_case() {
        let rec = OutputRecord::cache(CacheOutcome::Hit);
        assert_eq!(describe_plain(&rec), "[cache] hit");
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
