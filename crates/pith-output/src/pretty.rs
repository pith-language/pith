//! Pretty renderer: unicode plus a palette adapted to the output terminal.
//!
//! Every style comes from [`crate::palette::Palette`], which the CLI's help
//! also reads.

use std::io::{self, Write};

use crate::palette::{self, Palette, Role};
use crate::{OutputRecord, Payload, PhaseStatus, Renderer};

pub struct PrettyRenderer<W: Write> {
    out: W,
    palette: Palette,
}

impl<W: Write> PrettyRenderer<W> {
    pub fn new(out: W, palette: Palette) -> Self {
        Self { out, palette }
    }
}

impl<W: Write> Renderer for PrettyRenderer<W> {
    fn emit(&mut self, record: &OutputRecord) -> io::Result<()> {
        let line = describe_pretty(record, self.palette);
        writeln!(self.out, "{line}")
    }

    fn write_raw(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.out.write_all(bytes)
    }

    fn finish(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

fn describe_pretty(record: &OutputRecord, palette: Palette) -> String {
    match &record.payload {
        Payload::Phase { name, status } => {
            let (glyph, colored) = match status {
                PhaseStatus::Started => ("•", styled(palette, palette::SUCCESS, name)),
                PhaseStatus::Finished => (
                    "✓",
                    palette::paint(palette.emphasized(palette::SUCCESS), name),
                ),
                PhaseStatus::Failed => ("✗", styled(palette, palette::FAILURE, name)),
            };
            format!("{glyph} {colored}")
        }
        Payload::Cache { outcome } => {
            let style = match outcome {
                crate::CacheOutcome::Hit => palette::SUCCESS,
                crate::CacheOutcome::Miss => palette::ATTENTION,
                crate::CacheOutcome::Reuse => palette::REUSE,
            };
            format!(
                "{} {}",
                styled(palette, palette::MUTED, "cache"),
                styled(palette, style, outcome.as_str())
            )
        }
        Payload::Explain { steps } => {
            let mut described = String::from("why:\n");
            for step in steps.iter() {
                described.push_str(&format!(
                    "  {} {}\n",
                    styled(palette, palette::MUTED, "·"),
                    styled(palette, palette::HEADING, &step.label)
                ));
                described.push_str(&format!(
                    "      {}\n",
                    styled(palette, palette::MUTED, &step.detail)
                ));
            }
            described.trim_end().to_string()
        }
        Payload::Result { summary } => {
            format!("{} {summary}", styled(palette, palette::REUSE, "=>"))
        }
        Payload::Summary {
            hits,
            misses,
            reuses,
            errors,
            wall_ms,
        } => {
            let verdict = if *errors > 0 {
                styled(palette, palette::FAILURE, &format!("{errors} errors"))
            } else {
                styled(palette, palette::SUCCESS, "ok")
            };
            format!("{verdict} {hits} hits, {misses} misses, {reuses} reuses in {wall_ms}ms")
        }
        Payload::Query { query, .. } => crate::describe::query_view(query).render(Some(palette)),
    }
}

fn styled(palette: Palette, role: Role, text: &str) -> String {
    palette::paint(palette.style(role), text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{
        DeclarationBodyRepr, DeclarationView, ModuleView, QueryView, SumConstructorRepr, TypeRepr,
    };
    use crate::{CacheOutcome, OutputRecord, PhaseStatus};
    use termprofile::TermProfile;

    fn true_color() -> Palette {
        Palette::for_profile(TermProfile::TrueColor)
    }

    #[test]
    fn pretty_phase_started_has_glyph() {
        let rec = OutputRecord::phase("build", PhaseStatus::Started);
        let described = describe_pretty(&rec, true_color());
        assert!(described.starts_with('•'), "{described}");
        assert!(described.contains("build"), "{described}");
    }

    #[test]
    fn pretty_summary_reports_errors_when_present() {
        let rec = OutputRecord::summary(1, 0, 0, 3, 9);
        let described = describe_pretty(&rec, true_color());
        assert!(described.contains("errors"), "{described}");
    }

    #[test]
    fn pretty_cache_reuse_uses_reuse_word() {
        let rec = OutputRecord::cache(CacheOutcome::Reuse);
        let described = describe_pretty(&rec, true_color());
        assert!(described.contains("reuse"), "{described}");
    }

    /// The pretty renderer and the CLI's help must read one table. This pins
    /// that the renderer emits the palette's escape sequence rather than one
    /// it built itself.
    #[test]
    fn pretty_paints_from_the_palette() {
        let rec = OutputRecord::phase("build", PhaseStatus::Started);
        let palette = true_color();
        let described = describe_pretty(&rec, palette);
        assert!(
            described.contains(&palette.style(palette::SUCCESS).to_string()),
            "{described:?}"
        );
    }

    fn version_bound() -> TypeRepr {
        TypeRepr::Record {
            fields: Box::new([
                ("inclusive".into(), TypeRepr::Bool),
                ("version".into(), TypeRepr::Text),
            ]),
        }
    }

    /// A module description breaks a type that does not fit on one line into
    /// one construct per line rather than one long line, and shortens the
    /// digests a person only compares. This is the human contract for
    /// `explore`; the JSON surface keeps full digests and flat renderings.
    #[test]
    fn pretty_module_breaks_long_types_and_shortens_digests() {
        let digest = "13e68a5b642bf49fc4ed28d527abe43f3bca517f0f742af916910104019a0ce0";
        let view = QueryView::Module(ModuleView {
            module: "example".into(),
            path: "fixtures/example.pi".into(),
            abi_digest: digest.into(),
            imports: Box::new([]),
            declarations: Box::new([DeclarationView {
                name: "Range".into(),
                body: DeclarationBodyRepr::Sum {
                    constructors: Box::new([
                        SumConstructorRepr {
                            name: "Any".into(),
                            payload: None,
                        },
                        SumConstructorRepr {
                            name: "Between".into(),
                            payload: Some(Box::new(TypeRepr::Record {
                                fields: Box::new([
                                    ("lower".into(), version_bound()),
                                    ("upper".into(), version_bound()),
                                ]),
                            })),
                        },
                    ]),
                },
                rendered: "irrelevant to the description".into(),
                digest: digest.into(),
                documentation: String::new().into(),
            }]),
            rules: Box::new([]),
        });
        let described = describe_pretty(
            &OutputRecord::query(view),
            Palette::for_profile(TermProfile::NoColor),
        );
        assert!(described.contains(&digest[..12]), "{described}");
        assert!(!described.contains(digest), "{described}");
        assert!(
            described
                .lines()
                .any(|line| line.trim_start().starts_with("| Any")),
            "{described}"
        );
        assert!(
            described
                .lines()
                .any(|line| line.trim_start().starts_with("| Between(")),
            "{described}"
        );
    }
}
