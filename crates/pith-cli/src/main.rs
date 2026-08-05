use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use pith_core::{Interface, Request, Type};
use pith_diag::Span;
use pith_engine::Engine;
use pith_output::{IntoOutput, OutputRecord, OutputShape, PhaseStatus, Sink};

#[derive(Parser)]
#[command(name = "pith", version, about = "the pith kernel")]
struct Cli {
    /// Output shape. Defaults to pretty on a TTY, plain otherwise.
    #[arg(long, value_enum)]
    output: Option<ShapeArg>,

    /// Subcommand. M-1: only `eval` exists.
    #[command(subcommand)]
    command: Command,
}

#[derive(Copy, Clone, ValueEnum)]
enum ShapeArg {
    Pretty,
    Plain,
    Json,
}

#[derive(Parser)]
enum Command {
    /// Evaluate a request by label against the (currently empty) rule set.
    Eval { request: String },
}

impl Command {
    /// The phase label this command reports under. Single source for the
    /// string emitted in phase records, instead of three retyped literals.
    const fn phase_label(&self) -> &'static str {
        match self {
            Command::Eval { .. } => "eval",
        }
    }
}

fn main() -> ExitCode {
    init_tracing();
    let cli = Cli::parse();

    let shape = resolve_shape(cli.output);
    let stdout = io::stdout();
    match shape {
        OutputShape::Pretty => run(cli, pith_output::PrettyRenderer::new(stdout.lock())),
        OutputShape::Plain => run(cli, pith_output::PlainRenderer::new(stdout.lock())),
        OutputShape::Json => run(cli, pith_output::JsonRenderer::new(stdout.lock())),
    }
}

fn run(cli: Cli, renderer: impl pith_output::Renderer) -> ExitCode {
    let mut sink = Sink::new(renderer);
    let label = cli.command.phase_label();
    let _ = sink.emit(&OutputRecord::phase(label, PhaseStatus::Started));

    let result = match cli.command {
        Command::Eval { request } => eval(&mut sink, &request),
    };

    match result {
        Ok(()) => {
            let _ = sink.emit(&OutputRecord::phase(label, PhaseStatus::Finished));
            let _ = sink.finish();
            ExitCode::SUCCESS
        }
        Err(diags) => {
            let _ = sink.emit(&OutputRecord::phase(label, PhaseStatus::Failed));
            for diag in diags.iter() {
                render_diag(diag);
            }
            let _ = sink.finish();
            ExitCode::FAILURE
        }
    }
}

fn eval(
    sink: &mut Sink<impl pith_output::Renderer>,
    request: &str,
) -> Result<(), pith_diag::DiagnosticSink> {
    let mut engine = Engine::new();
    let req = Request::new(
        request,
        Interface {
            inputs: Box::new([]),
            output: Type::Unit,
        },
        [],
        Span::none(),
    );
    match engine.evaluate_pure(&req) {
        Ok(evaluation) => {
            let _ = sink.emit(&evaluation.value.to_record());
            Ok(())
        }
        Err(diags) => Err(diags),
    }
}

fn resolve_shape(flag: Option<ShapeArg>) -> OutputShape {
    match flag {
        Some(ShapeArg::Pretty) => OutputShape::Pretty,
        Some(ShapeArg::Plain) => OutputShape::Plain,
        Some(ShapeArg::Json) => OutputShape::Json,
        None => {
            if io::stdout().is_terminal() {
                OutputShape::Pretty
            } else {
                OutputShape::Plain
            }
        }
    }
}

fn render_diag(diag: &pith_diag::Diag) {
    let handler = miette::GraphicalReportHandler::new();
    let report = miette::Report::new(diag.clone());
    let mut out = String::new();
    let _ = handler.render_report(&mut out, report.as_ref());
    let _ = io::stderr().write_all(out.as_bytes());
    let _ = io::stderr().write_all(b"\n");
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(io::stderr)
        .try_init();
}
