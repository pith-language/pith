mod checks;
mod files;
mod report;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let command = args.next();
    if let Some(extra) = args.next() {
        eprintln!("unexpected argument: {extra}");
        return ExitCode::FAILURE;
    }

    let Some(root) = workspace_root() else {
        eprintln!("xtask: could not determine the workspace root");
        return ExitCode::FAILURE;
    };

    match command.as_deref() {
        Some("check") => run_checks(&root, checks::ALL),
        Some("check-determinism") => run_checks(&root, &[checks::determinism::run]),
        Some("check-docs") => run_checks(&root, &[checks::docs::run]),
        Some("help") | None => {
            print_help();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown subcommand: {other}");
            print_help();
            ExitCode::FAILURE
        }
    }
}

fn workspace_root() -> Option<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
}

fn run_checks(root: &Path, checks: &[checks::Check]) -> ExitCode {
    let mut failed = false;
    for check in checks {
        let report = check(root);
        if report.is_success() {
            eprintln!("{}: {}", report.name(), report.success_message());
        } else {
            failed = true;
            eprintln!("{}: failed", report.name());
            for diagnostic in report.diagnostics() {
                eprintln!("  {diagnostic}");
            }
        }
    }

    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn print_help() {
    eprintln!("xtask subcommands:");
    eprintln!("  check              run every repository-specific check");
    eprintln!("  check-determinism  fail if HashMap is used in non-test source");
    eprintln!("  check-docs         validate Markdown frontmatter and document relations");
}
