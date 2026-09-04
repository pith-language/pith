use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::command;

#[derive(Parser)]
#[command(
    name = "pith",
    version,
    about = "the pith kernel",
    propagate_version = true,
    after_help = "Workspace commands require a workspace; diff, update, and add are visible here but arrive with M-14."
)]
pub(crate) struct Cli {
    #[command(flatten)]
    pub(crate) globals: Globals,

    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(clap::Args)]
pub(crate) struct Globals {
    /// Output shape. Defaults to pretty on a TTY, plain otherwise.
    #[arg(long, global = true, value_enum)]
    pub(crate) output: Option<ShapeArg>,

    /// Content store root. Defaults to $PITH_HOME/store.
    #[arg(long, global = true, env = "PITH_STORE", value_name = "DIR")]
    pub(crate) store: Option<PathBuf>,

    /// Engine state root. Defaults to $PITH_HOME/state.db.
    #[arg(long, global = true, env = "PITH_STATE", value_name = "FILE")]
    pub(crate) state: Option<PathBuf>,
}

#[derive(Copy, Clone, ValueEnum)]
pub(crate) enum ShapeArg {
    Pretty,
    Plain,
    Json,
}

impl ShapeArg {
    pub(crate) const fn shape(self) -> pith_output::OutputShape {
        match self {
            Self::Pretty => pith_output::OutputShape::Pretty,
            Self::Plain => pith_output::OutputShape::Plain,
            Self::Json => pith_output::OutputShape::Json,
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Elaborate the module at PATH and report its errors and warnings.
    Check(command::Check),
    /// Show types, rules, entries, interfaces, metadata, and implementation tiers.
    Explore(command::Explore),
    /// Write the canonical spelling of the module at PATH.
    Fmt(command::Fmt),
    /// Evaluate a named entry and render its value.
    Run(command::Run),
    /// Evaluate a named entry and explain its last invalidation.
    Explain(command::Explain),
    /// Inspect entry selection, action plans, and recorded dependencies.
    Graph {
        #[command(subcommand)]
        command: GraphCommand,
    },
    /// Evaluate an entry to pith.Exec and replace this process with it.
    Exec(command::Exec),
    /// Inspect and admit content.
    Store {
        #[command(subcommand)]
        command: StoreCommand,
    },
    /// Inspect engine state.
    State {
        #[command(subcommand)]
        command: StateCommand,
    },
    /// Report what a collection would reclaim.
    Gc(command::Gc),
    /// Compare the workspace declaration with its lock; requires a workspace.
    Diff,
    /// Resolve and write the workspace lock; requires a workspace.
    Update,
    /// Add a dependency to the workspace; requires a workspace.
    Add,
    /// Unstable implementation diagnostics.
    #[command(hide = true, disable_version_flag = true)]
    Debug {
        #[command(subcommand)]
        command: DebugCommand,
    },
}

#[derive(Subcommand)]
pub(crate) enum GraphCommand {
    /// Show which rule serves the entry's request.
    Select(command::GraphSelect),
    /// Derive the first action contract without executing it.
    Plan(command::GraphPlan),
    /// Show the last attempt's recorded dependency subtree.
    Deps(command::GraphDeps),
}

#[derive(Subcommand)]
pub(crate) enum StoreCommand {
    /// Put a file or directory into the store and print its identity.
    Add(command::StoreAdd),
    /// Write a blob to stdout.
    Cat(command::StoreCat),
    /// List a tree's entries.
    Ls(command::StoreLs),
    /// Render a tree into a new directory.
    Materialize(command::StoreMaterialize),
}

#[derive(Subcommand)]
pub(crate) enum StateCommand {
    /// Schema versions, adapter, and record counts.
    Info(command::StateInfo),
    /// Decode every durable record the state database holds.
    Check(command::StateCheck),
}

#[derive(Subcommand)]
pub(crate) enum DebugCommand {
    /// Show terminal capability, theme-query, and selected-palette details.
    #[command(disable_version_flag = true)]
    Terminal,
}
