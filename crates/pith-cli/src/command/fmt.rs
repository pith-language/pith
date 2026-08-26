use std::path::PathBuf;

use pith_output::OutputRecord;
use pith_output::dto::{FmtStatus, QueryView};

use super::{Context, Execute, Report};
use crate::exit::Failure;

#[derive(clap::Args)]
pub struct Fmt {
    /// The module whose canonical spelling is written or verified.
    #[arg(value_name = "PATH")]
    path: PathBuf,

    /// Verify the module is already canonical, without writing.
    #[arg(long)]
    check: bool,
}

impl Execute for Fmt {
    const LABEL: &'static str = "fmt";

    fn execute(self, _context: &mut Context) -> Result<Report, Failure> {
        let mode = if self.check {
            pith_query::FormatMode::Check
        } else {
            pith_query::FormatMode::Write
        };
        let report = pith_query::format(&self.path, mode)?;
        let status = report.status;
        let records = vec![OutputRecord::query(QueryView::Format(report))];
        match status {
            FmtStatus::WouldFormat => Ok(Report::refused(
                records,
                Failure::user(format!(
                    "`{}` is not canonical; run `pith fmt` without --check to write it",
                    self.path.display()
                )),
            )),
            FmtStatus::Unchanged | FmtStatus::Formatted => Ok(Report::of(records)),
        }
    }
}
