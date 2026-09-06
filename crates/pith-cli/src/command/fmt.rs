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
        let reports = pith_query::format(&self.path, mode)?;
        let not_canonical = reports
            .iter()
            .any(|report| report.status == FmtStatus::WouldFormat);
        let records = reports
            .into_iter()
            .map(QueryView::Format)
            .map(OutputRecord::query)
            .collect::<Vec<_>>();
        if not_canonical {
            Ok(Report::refused(
                records,
                Failure::user(format!(
                    "`{}` is not canonical; run `pith fmt` without --check to write it",
                    self.path.display()
                )),
            ))
        } else {
            Ok(Report::of(records))
        }
    }
}
