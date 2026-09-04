use pith_output::OutputRecord;
use pith_output::dto::QueryView;

use super::entry::EntryTarget;
use super::{Context, Execute, Report};
use crate::exit::Failure;

#[derive(clap::Args)]
pub struct GraphSelect {
    #[command(flatten)]
    target: EntryTarget,
}

#[derive(clap::Args)]
pub struct GraphPlan {
    #[command(flatten)]
    target: EntryTarget,
}

#[derive(clap::Args)]
pub struct GraphDeps {
    #[command(flatten)]
    target: EntryTarget,
}

impl Execute for GraphSelect {
    const LABEL: &'static str = "graph-select";

    fn execute(self, context: &mut Context) -> Result<Report, Failure> {
        let (module, entry) = self.target.parts();
        let view = context.query_read_only(|session| session.select_entry(module, entry))?;
        Ok(Report::of(vec![OutputRecord::query(QueryView::Selection(
            view,
        ))]))
    }
}

impl Execute for GraphPlan {
    const LABEL: &'static str = "graph-plan";

    fn execute(self, context: &mut Context) -> Result<Report, Failure> {
        let (module, entry) = self.target.parts();
        let view = context.query_writable(|session| session.plan_entry(module, entry))?;
        Ok(Report::of(vec![OutputRecord::query(
            QueryView::ActionPlan(view),
        )]))
    }
}

impl Execute for GraphDeps {
    const LABEL: &'static str = "graph-deps";

    fn execute(self, context: &mut Context) -> Result<Report, Failure> {
        let (module, entry) = self.target.parts();
        let view = context.query_read_only(|session| session.entry_dependencies(module, entry))?;
        Ok(Report::of(vec![OutputRecord::query(
            QueryView::Dependencies(view),
        )]))
    }
}
