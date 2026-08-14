use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use pith_core::{Interface, Request, Type};
use pith_diag::{Diag, DiagnosticSink, EngineCode, Span};
use pith_engine::Engine;
use pith_ids::{ContentDigest, ContentId, DIGEST_LEN};
use pith_output::{IntoOutput, OutputRecord, OutputShape, PhaseStatus, Sink};
use pith_store::{FilesystemContentStore, StoreError, materialize_tree};

#[derive(Parser)]
#[command(name = "pith", version, about = "the pith kernel")]
struct Cli {
    /// Output shape. Defaults to pretty on a TTY, plain otherwise.
    #[arg(long, value_enum)]
    output: Option<ShapeArg>,

    /// Operation to perform.
    #[command(subcommand)]
    command: Command,
}

#[derive(Copy, Clone, ValueEnum)]
enum ShapeArg {
    Pretty,
    Plain,
    Json,
}

#[derive(Subcommand)]
enum Command {
    /// Evaluate a request by label against the (currently empty) rule set.
    Eval { request: String },
    /// Inspect and materialize content from a filesystem store.
    Store {
        #[command(subcommand)]
        command: StoreCommand,
    },
}

#[derive(Subcommand)]
enum StoreCommand {
    /// Render a stored tree into a new filesystem directory.
    Materialize {
        /// Root containing the store's `blobs` and `trees` directories.
        #[arg(long)]
        store: PathBuf,

        /// Digest of the root tree to render.
        #[arg(long, value_parser = parse_content_id)]
        tree: ContentId,

        /// New directory to create from the tree.
        #[arg(long)]
        output: PathBuf,
    },
}

impl Command {
    /// The phase label this command reports under. Single source for the
    /// string emitted in phase records, instead of three retyped literals.
    const fn phase_label(&self) -> &'static str {
        match self {
            Command::Eval { .. } => "eval",
            Command::Store {
                command: StoreCommand::Materialize { .. },
            } => "store-materialize",
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
        Command::Store {
            command:
                StoreCommand::Materialize {
                    store,
                    tree,
                    output,
                },
        } => materialize(&mut sink, &store, tree, &output),
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

fn materialize(
    sink: &mut Sink<impl pith_output::Renderer>,
    store_root: &std::path::Path,
    tree: ContentId,
    output: &std::path::Path,
) -> Result<(), DiagnosticSink> {
    let store = FilesystemContentStore::open(store_root).map_err(store_diagnostic)?;
    materialize_tree(&store, tree, output).map_err(store_diagnostic)?;
    let _ = sink.emit(&OutputRecord::result(format!(
        "materialized tree {} at {}",
        tree.digest(),
        output.display()
    )));
    Ok(())
}

fn parse_content_id(value: &str) -> Result<ContentId, String> {
    if value.len() != DIGEST_LEN.saturating_mul(2) {
        return Err("tree digest must contain exactly 64 hexadecimal characters".to_string());
    }
    let mut bytes = [0u8; DIGEST_LEN];
    for (slot, pair) in bytes.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        let pair = std::str::from_utf8(pair)
            .map_err(|_| "tree digest must contain only hexadecimal characters".to_string())?;
        *slot = u8::from_str_radix(pair, 16)
            .map_err(|_| "tree digest must contain only hexadecimal characters".to_string())?;
    }
    Ok(ContentId::from_digest(ContentDigest::from_bytes(bytes)))
}

fn store_diagnostic(error: StoreError) -> DiagnosticSink {
    let mut diagnostics = DiagnosticSink::new();
    diagnostics.push(Diag::engine(
        EngineCode::StoreError,
        Span::none(),
        error.to_string(),
    ));
    diagnostics
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_id_parser_accepts_a_full_hex_digest() {
        let text = "13e68a5b642bf49fc4ed28d527abe43f3bca517f0f742af916910104019a0ce0";

        let parsed = parse_content_id(text);

        assert_eq!(
            parsed.map(|id| id.digest().to_string()).as_deref(),
            Ok(text)
        );
    }

    #[test]
    fn content_id_parser_rejects_wrong_length_and_non_hex_text() {
        assert!(parse_content_id("abcd").is_err());
        assert!(
            parse_content_id("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz")
                .is_err()
        );
    }
}
