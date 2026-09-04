use pith_output::OutputRecord;
use pith_output::dto::QueryView;

use super::entry::EntryTarget;
use super::{Context, Execute, Report};
use crate::exit::Failure;

#[derive(clap::Args)]
pub struct Run {
    #[command(flatten)]
    target: EntryTarget,
}

impl Execute for Run {
    const LABEL: &'static str = "run";

    fn execute(self, context: &mut Context) -> Result<Report, Failure> {
        let (module, entry) = self.target.parts();
        let view = context.query_writable(|session| session.run_entry(module, entry))?;
        Ok(Report::of(vec![OutputRecord::query(QueryView::Run(view))]))
    }
}
