use std::path::PathBuf;

use pith_output::OutputRecord;
use pith_output::dto::QueryView;

use super::{Context, Execute, Report};
use crate::exit::Failure;

#[derive(Copy, Clone, clap::ValueEnum)]
enum DeniedSeverity {
    Warnings,
}

#[derive(clap::Args)]
pub struct Check {
    /// The module to elaborate.
    #[arg(value_name = "PATH")]
    path: PathBuf,

    /// Treat warnings as a failure.
    #[arg(long = "deny", value_name = "SEVERITY")]
    deny: Option<DeniedSeverity>,
}

impl Execute for Check {
    const LABEL: &'static str = "check";

    fn execute(self, _context: &mut Context) -> Result<Report, Failure> {
        let report = pith_query::check(&self.path)?;
        let (errors, warnings) = (report.errors, report.warnings);
        let records = vec![OutputRecord::query(QueryView::Check(report))];
        if errors > 0 {
            return Ok(Report::refused(
                records,
                Failure::user(format!("{errors} error(s)")),
            ));
        }
        if self.deny.is_some() && warnings > 0 {
            return Ok(Report::refused(
                records,
                Failure::user(format!("{warnings} warning(s), and warnings are denied")),
            ));
        }
        Ok(Report::of(records))
    }
}
