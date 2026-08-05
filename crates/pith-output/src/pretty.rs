//! Pretty renderer: unicode + color for terminals. Color emission is wrapped
//! by `anstream` at the CLI boundary, which handles TTY detection and
//! `NO_COLOR`, so this module emits color unconditionally.

use std::io::{self, Write};

use owo_colors::OwoColorize;

use crate::{OutputRecord, Payload, PhaseStatus, Renderer};

pub struct PrettyRenderer<W: Write> {
    out: W,
}

impl<W: Write> PrettyRenderer<W> {
    pub fn new(out: W) -> Self {
        Self { out }
    }
}

impl<W: Write> Renderer for PrettyRenderer<W> {
    fn emit(&mut self, record: &OutputRecord) -> io::Result<()> {
        let line = describe_pretty(record);
        writeln!(self.out, "{line}")
    }

    fn finish(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

fn describe_pretty(record: &OutputRecord) -> String {
    match &record.payload {
        Payload::Phase { name, status } => {
            let (glyph, colored) = match status {
                PhaseStatus::Started => ("•", name.green().to_string()),
                PhaseStatus::Finished => ("✓", name.green().bold().to_string()),
                PhaseStatus::Failed => ("✗", name.red().bold().to_string()),
            };
            format!("{glyph} {colored}")
        }
        Payload::Cache { outcome } => match outcome {
            crate::CacheOutcome::Hit => format!("{} {}", "cache".dimmed(), "hit".green()),
            crate::CacheOutcome::Miss => format!("{} {}", "cache".dimmed(), "miss".yellow()),
            crate::CacheOutcome::Reuse => format!("{} {}", "cache".dimmed(), "reuse".cyan()),
        },
        Payload::Explain { steps } => {
            let mut s = String::from("why:\n");
            for step in steps.iter() {
                s.push_str(&format!("  {} {}\n", "·".dimmed(), step.label.bold()));
                s.push_str(&format!("      {}\n", step.detail.dimmed()));
            }
            s.trim_end().to_string()
        }
        Payload::Result { summary } => format!("{} {summary}", "=>".cyan()),
        Payload::Summary {
            hits,
            misses,
            reuses,
            errors,
            wall_ms,
        } => {
            let err_part = if *errors > 0 {
                format!("{errors} errors").red().bold().to_string()
            } else {
                "ok".green().to_string()
            };
            format!("{err_part} {hits} hits, {misses} misses, {reuses} reuses in {wall_ms}ms")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CacheOutcome, OutputRecord, PhaseStatus};

    #[test]
    fn pretty_phase_started_has_glyph() {
        let rec = OutputRecord::phase("build", PhaseStatus::Started);
        let described = describe_pretty(&rec);
        assert!(described.starts_with('•'), "{described}");
        assert!(described.contains("build"), "{described}");
    }

    #[test]
    fn pretty_summary_reports_errors_when_present() {
        let rec = OutputRecord::summary(1, 0, 0, 3, 9);
        let described = describe_pretty(&rec);
        // the "errors" word appears when there are errors
        assert!(described.contains("errors"), "{described}");
    }

    #[test]
    fn pretty_cache_reuse_uses_reuse_word() {
        let rec = OutputRecord::cache(CacheOutcome::Reuse);
        let described = describe_pretty(&rec);
        assert!(described.contains("reuse"), "{described}");
    }
}
