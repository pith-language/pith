use pith_output::OutputRecord;
use pith_output::dto::QueryView;

use super::{Context, Execute, Report};
use crate::exit::Failure;

#[derive(clap::Args)]
pub struct StateInfo;

impl Execute for StateInfo {
    const LABEL: &'static str = "state-info";

    fn execute(self, context: &mut Context) -> Result<Report, Failure> {
        let session = context.read_only()?;
        let info = session.state_info()?;
        Ok(Report::of(vec![OutputRecord::query(QueryView::State(
            info,
        ))]))
    }
}

#[derive(clap::Args)]
pub struct StateCheck;

impl Execute for StateCheck {
    const LABEL: &'static str = "state-check";

    fn execute(self, context: &mut Context) -> Result<Report, Failure> {
        let session = context.read_only()?;
        let check = session.state_check()?;
        Ok(Report::of(vec![OutputRecord::query(
            QueryView::StateCheck(check),
        )]))
    }
}
