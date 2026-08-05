use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check-determinism") => check_determinism(),
        Some("help") | None => {
            eprintln!("xtask subcommands:");
            eprintln!("  check-determinism  fail if HashMap is used in non-test source");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown subcommand: {other}");
            ExitCode::FAILURE
        }
    }
}

fn check_determinism() -> ExitCode {
    let offenders = grep_hashmap_in_crates();
    if offenders.is_empty() {
        eprintln!("determinism: no HashMap use in crate source");
        ExitCode::SUCCESS
    } else {
        eprintln!("determinism: HashMap found in crate source (decision 0021 forbids it):");
        for line in offenders {
            eprintln!("  {line}");
        }
        ExitCode::FAILURE
    }
}

fn grep_hashmap_in_crates() -> Vec<String> {
    let output = Command::new("rg")
        .args([
            "-n",
            r"HashMap<|use std::collections::HashMap|use ::std::collections::HashMap",
            "crates/",
            "--glob",
            "!**/snapshots/**",
        ])
        .output();
    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(String::from)
            .collect(),
        Err(_) => {
            eprintln!("determinism: ripgrep not found, skipping");
            Vec::new()
        }
    }
}
