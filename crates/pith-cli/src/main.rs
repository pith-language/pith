#![deny(dead_code_pub_in_binary)]

mod command;
mod exit;
mod style;
mod terminal;

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ColorChoice, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use pith_output::palette::Palette;
use pith_output::{OutputRecord, OutputShape, PhaseStatus, Renderer, Sink};
use termprofile::TermProfile;

use command::{CommandOutput, Context, OutputKind, Report, Runnable};
use exit::Failure;

#[derive(Parser)]
#[command(
    name = "pith",
    version,
    about = "the pith kernel",
    propagate_version = true
)]
struct Cli {
    #[command(flatten)]
    globals: Globals,

    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Args)]
struct Globals {
    /// Output shape. Defaults to pretty on a TTY, plain otherwise.
    #[arg(long, global = true, value_enum)]
    output: Option<ShapeArg>,

    /// Content store root. Defaults to $PITH_HOME/store.
    #[arg(long, global = true, env = "PITH_STORE", value_name = "DIR")]
    store: Option<PathBuf>,

    /// Engine state root. Defaults to $PITH_HOME/state.db.
    #[arg(long, global = true, env = "PITH_STATE", value_name = "FILE")]
    state: Option<PathBuf>,
}

#[derive(Copy, Clone, ValueEnum)]
enum ShapeArg {
    Pretty,
    Plain,
    Json,
}

impl ShapeArg {
    const fn shape(self) -> OutputShape {
        match self {
            Self::Pretty => OutputShape::Pretty,
            Self::Plain => OutputShape::Plain,
            Self::Json => OutputShape::Json,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Elaborate the module at PATH and report its errors and warnings.
    Check(command::Check),
    /// Show what a module declares: types, rules, and which tier answers.
    Explore(command::Explore),
    /// Write the canonical spelling of the module at PATH.
    Fmt(command::Fmt),
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
    /// Unstable implementation diagnostics.
    #[command(hide = true, disable_version_flag = true)]
    Debug {
        #[command(subcommand)]
        command: DebugCommand,
    },
}

#[derive(Subcommand)]
enum StoreCommand {
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
enum StateCommand {
    /// Schema versions, adapter, and record counts.
    Info(command::StateInfo),
    /// Decode every durable record the state database holds.
    Check(command::StateCheck),
}

#[derive(Subcommand)]
enum DebugCommand {
    /// Show terminal capability, theme-query, and selected-palette details.
    #[command(disable_version_flag = true)]
    Terminal,
}

enum Action {
    Stable(Box<dyn Runnable>),
    Debug(DebugCommand),
}

fn main() -> ExitCode {
    init_tracing();
    let stdout = io::stdout();
    let terminal = terminal::OutputTerminal::detect(&stdout);
    let profile = terminal.profile();
    let palette = terminal.palette();
    let cli = parse(palette, profile);
    let Cli { globals, command } = cli;

    let runnable = match command.into_action() {
        Action::Debug(command) => return run_debug(command, &terminal, stdout.lock()),
        Action::Stable(runnable) => runnable,
    };

    let shape = resolve_shape(globals.output, terminal.pretty_by_default());
    match shape {
        OutputShape::Pretty => run(
            globals,
            runnable,
            pith_output::PrettyRenderer::new(stdout.lock(), palette),
        ),
        OutputShape::Plain => run(
            globals,
            runnable,
            pith_output::PlainRenderer::new(stdout.lock()),
        ),
        OutputShape::Json => run(
            globals,
            runnable,
            pith_output::JsonRenderer::new(stdout.lock()),
        ),
    }
}

fn parse(palette: Palette, profile: TermProfile) -> Cli {
    let color = match profile {
        TermProfile::NoTty => ColorChoice::Never,
        TermProfile::NoColor
        | TermProfile::Ansi16
        | TermProfile::Ansi256
        | TermProfile::TrueColor => ColorChoice::Always,
    };
    let mut matches = Cli::command()
        .styles(style::help(palette))
        .color(color)
        .get_matches();
    Cli::from_arg_matches_mut(&mut matches).unwrap_or_else(|error| error.exit())
}

fn run(globals: Globals, runnable: Box<dyn Runnable>, renderer: impl Renderer) -> ExitCode {
    let mut sink = Sink::new(renderer);
    let label = runnable.label();
    let output_kind = runnable.output_kind();

    if output_kind == OutputKind::Records
        && let Err(error) = sink.emit(&OutputRecord::phase(label, PhaseStatus::Started))
    {
        return output_error(error);
    }

    let mut context = Context::new(globals.store, globals.state);
    let Report { output, failure } = match runnable.run(&mut context) {
        Ok(report) => report,
        Err(failure) => Report {
            output: CommandOutput::empty(output_kind),
            failure: Some(failure),
        },
    };

    if output.kind() != output_kind {
        return fail(Failure::internal(format!(
            "command `{label}` returned the wrong output kind"
        )));
    }

    let write_result = match output {
        CommandOutput::Records(records) => {
            let mut result = Ok(());
            for record in &records {
                if let Err(error) = sink.emit(record) {
                    result = Err(error);
                    break;
                }
            }
            if result.is_ok() {
                let status = if failure.is_some() {
                    PhaseStatus::Failed
                } else {
                    PhaseStatus::Finished
                };
                result = sink.emit(&OutputRecord::phase(label, status));
            }
            result
        }
        CommandOutput::RawBytes(bytes) => sink.write_raw(&bytes),
    };

    if let Err(error) = write_result {
        return output_error(error);
    }
    if let Err(error) = sink.finish() {
        return output_error(error);
    }

    match failure {
        None => ExitCode::SUCCESS,
        Some(failure) => fail(failure),
    }
}

fn output_error(error: io::Error) -> ExitCode {
    if error.kind() == io::ErrorKind::BrokenPipe {
        return ExitCode::SUCCESS;
    }
    fail(Failure::internal(format!("cannot write output: {error}")))
}

fn fail(failure: Failure) -> ExitCode {
    report(&failure);
    failure.exit_code()
}

impl Command {
    fn into_action(self) -> Action {
        match self {
            Self::Check(command) => Action::Stable(Box::new(command)),
            Self::Explore(command) => Action::Stable(Box::new(command)),
            Self::Fmt(command) => Action::Stable(Box::new(command)),
            Self::Store { command } => Action::Stable(command.into_runnable()),
            Self::State { command } => Action::Stable(command.into_runnable()),
            Self::Gc(command) => Action::Stable(Box::new(command)),
            Self::Debug { command } => Action::Debug(command),
        }
    }
}

impl StoreCommand {
    fn into_runnable(self) -> Box<dyn Runnable> {
        match self {
            Self::Add(command) => Box::new(command),
            Self::Cat(command) => Box::new(command),
            Self::Ls(command) => Box::new(command),
            Self::Materialize(command) => Box::new(command),
        }
    }
}

impl StateCommand {
    fn into_runnable(self) -> Box<dyn Runnable> {
        match self {
            Self::Info(command) => Box::new(command),
            Self::Check(command) => Box::new(command),
        }
    }
}

fn run_debug(
    command: DebugCommand,
    terminal: &terminal::OutputTerminal,
    out: impl Write,
) -> ExitCode {
    let result = match command {
        DebugCommand::Terminal => terminal.write_debug_report(out),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(
                io::stderr(),
                "error: cannot write terminal debug report: {error}"
            );
            ExitCode::FAILURE
        }
    }
}

fn report(failure: &Failure) {
    for diagnostic in failure.diagnostics() {
        let handler = miette::GraphicalReportHandler::new();
        let report = miette::Report::new(diagnostic.clone());
        let mut rendered = String::new();
        let _ = handler.render_report(&mut rendered, report.as_ref());
        let _ = io::stderr().write_all(rendered.as_bytes());
        let _ = io::stderr().write_all(b"\n");
    }
    let _ = writeln!(io::stderr(), "error: {}", failure.message());
}

fn resolve_shape(flag: Option<ShapeArg>, pretty_by_default: bool) -> OutputShape {
    match flag {
        Some(shape) => shape.shape(),
        None if pretty_by_default => OutputShape::Pretty,
        None => OutputShape::Plain,
    }
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(io::stderr)
        .try_init();
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use clap::CommandFactory;
    use command::Execute;

    struct MarkExecuted(Arc<AtomicBool>);

    impl Execute for MarkExecuted {
        const LABEL: &'static str = "mark-executed";

        fn execute(self, _context: &mut Context) -> Result<Report, Failure> {
            self.0.store(true, Ordering::SeqCst);
            Ok(Report::of(Vec::new()))
        }
    }

    struct FailingRenderer;

    impl Renderer for FailingRenderer {
        fn emit(&mut self, _out: &OutputRecord) -> io::Result<()> {
            Err(io::Error::other("writer failed"))
        }

        fn write_raw(&mut self, _bytes: &[u8]) -> io::Result<()> {
            Err(io::Error::other("writer failed"))
        }

        fn finish(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn empty_globals() -> Globals {
        Globals {
            output: None,
            store: None,
            state: None,
        }
    }

    #[test]
    fn an_initial_output_failure_prevents_command_execution() {
        let executed = Arc::new(AtomicBool::new(false));
        let exit = run(
            empty_globals(),
            Box::new(MarkExecuted(Arc::clone(&executed))),
            FailingRenderer,
        );

        assert_eq!(exit, ExitCode::from(2));
        assert!(!executed.load(Ordering::SeqCst));
    }

    #[test]
    fn the_argument_surface_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn every_command_has_its_own_label() {
        let labels = [
            command::Check::LABEL,
            command::Explore::LABEL,
            command::Fmt::LABEL,
            command::StoreAdd::LABEL,
            command::StoreCat::LABEL,
            command::StoreLs::LABEL,
            command::StoreMaterialize::LABEL,
            command::StateInfo::LABEL,
            command::StateCheck::LABEL,
            command::Gc::LABEL,
        ];
        let mut seen: Vec<&str> = Vec::new();
        for label in labels {
            assert!(!seen.contains(&label), "two commands report as `{label}`");
            seen.push(label);
        }
    }

    #[test]
    fn the_output_flag_is_accepted_after_the_subcommand() {
        let parsed = Cli::try_parse_from(["pith", "check", "a.pi", "--output", "json"]);

        assert!(parsed.is_ok(), "{:?}", parsed.err());
    }

    #[test]
    fn the_hidden_terminal_debug_command_is_parseable() {
        let parsed = Cli::try_parse_from(["pith", "debug", "terminal"]);

        assert!(parsed.is_ok(), "{:?}", parsed.err());
    }

    #[test]
    fn terminal_presence_decides_only_the_default_shape() {
        assert!(matches!(resolve_shape(None, false), OutputShape::Plain));
        assert!(matches!(resolve_shape(None, true), OutputShape::Pretty));
        assert!(matches!(
            resolve_shape(Some(ShapeArg::Json), true),
            OutputShape::Json
        ));
    }
}
