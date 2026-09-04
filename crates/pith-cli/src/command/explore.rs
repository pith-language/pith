use std::path::PathBuf;

use pith_output::OutputRecord;
use pith_output::dto::QueryView;

use super::{Context, Execute, Report};
use crate::exit::Failure;

#[derive(clap::Args)]
pub struct Explore {
    /// The module to describe.
    #[arg(value_name = "PATH")]
    path: PathBuf,
}

impl Execute for Explore {
    const LABEL: &'static str = "explore";

    fn execute(self, _context: &mut Context) -> Result<Report, Failure> {
        let view = pith_query::explore(&self.path)?;
        Ok(Report::of(vec![OutputRecord::query(QueryView::Module(
            view,
        ))]))
    }
}
