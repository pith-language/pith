use std::path::PathBuf;

use pith_ids::ContentId;
use pith_output::OutputRecord;
use pith_output::dto::QueryView;

use super::{Context, Execute, OutputKind, Report};
use crate::exit::Failure;

#[derive(clap::Args)]
pub struct StoreAdd {
    /// The file or directory to admit.
    #[arg(value_name = "PATH")]
    path: PathBuf,
}

impl Execute for StoreAdd {
    const LABEL: &'static str = "store-add";

    fn execute(self, context: &mut Context) -> Result<Report, Failure> {
        let mut session = context.writable()?;
        let stored = session.add(&self.path)?;
        Ok(Report::of(vec![OutputRecord::query(QueryView::Content(
            stored,
        ))]))
    }
}

#[derive(clap::Args)]
pub struct StoreCat {
    /// The blob to write.
    #[arg(value_name = "ID")]
    id: ContentId,
}

impl Execute for StoreCat {
    const LABEL: &'static str = "store-cat";
    const OUTPUT_KIND: OutputKind = OutputKind::RawBytes;

    fn execute(self, context: &mut Context) -> Result<Report, Failure> {
        let session = context.read_only()?;
        let blob = session.blob(self.id)?;
        Ok(Report::raw(blob.as_bytes()))
    }
}

#[derive(clap::Args)]
pub struct StoreLs {
    /// The tree to list.
    #[arg(value_name = "ID")]
    id: ContentId,
}

impl Execute for StoreLs {
    const LABEL: &'static str = "store-ls";

    fn execute(self, context: &mut Context) -> Result<Report, Failure> {
        let session = context.read_only()?;
        let listing = session.list_tree(self.id)?;
        Ok(Report::of(vec![OutputRecord::query(QueryView::Tree(
            listing,
        ))]))
    }
}

#[derive(clap::Args)]
pub struct StoreMaterialize {
    /// The tree to render.
    #[arg(value_name = "ID")]
    id: ContentId,

    /// The directory to create. Named `directory` and not `output`, because a
    /// positional sharing an id with the global `--output` resolves to
    /// whichever clap looked up last — and `debug_assert` does not catch it.
    #[arg(value_name = "DIR")]
    directory: PathBuf,
}

impl Execute for StoreMaterialize {
    const LABEL: &'static str = "store-materialize";

    fn execute(self, context: &mut Context) -> Result<Report, Failure> {
        let session = context.read_only()?;
        let stored = session.materialize(self.id, &self.directory)?;
        Ok(Report::of(vec![OutputRecord::query(QueryView::Content(
            stored,
        ))]))
    }
}
