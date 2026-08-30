use pith_output::OutputRecord;

use super::entry::EntryTarget;
use super::{Context, Execute, Report};
use crate::exit::Failure;

#[derive(clap::Args)]
pub struct Explain {
    #[command(flatten)]
    target: EntryTarget,
}

impl Execute for Explain {
    const LABEL: &'static str = "explain";

    fn execute(self, context: &mut Context) -> Result<Report, Failure> {
        let (module, entry) = self.target.parts();
        let steps = context.query_writable(|session| session.explain_entry(module, entry))?;
        Ok(Report::of(vec![OutputRecord::explain(steps)]))
    }
}
