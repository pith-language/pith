use pith_output::OutputRecord;
use pith_output::dto::QueryView;

use super::{Context, Execute, Report};
use crate::exit::Failure;

/// Preview collection across engine state and content storage.
#[derive(clap::Args)]
pub struct Gc {
    /// Report what a collection would reclaim, and reclaim nothing.
    #[arg(long)]
    dry_run: bool,
}

impl Execute for Gc {
    const LABEL: &'static str = "gc";

    fn execute(self, context: &mut Context) -> Result<Report, Failure> {
        if !self.dry_run {
            return Err(Failure::user(
                "garbage-collection deletion is not implemented; use `pith gc --dry-run`",
            ));
        }
        let session = context.read_only()?;
        let preview = session.gc_preview()?;
        Ok(Report::of(vec![OutputRecord::query(QueryView::Gc(
            preview,
        ))]))
    }
}
