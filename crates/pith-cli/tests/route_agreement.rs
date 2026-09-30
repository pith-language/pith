//! Real filesystem canonicalization, driven through the check command.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn project(root: &Path, name: &str, subject: &str, inputs: &str) -> std::io::Result<()> {
    let directory = root.join(name);
    fs::create_dir_all(&directory)?;
    fs::write(
        directory.join("pith.pi"),
        format!("module {subject} 1\n\ninputs {{\n{inputs}}}\n\nnominal Name = Text\n"),
    )
}

fn fixture(root: &Path, right: &str) -> std::io::Result<()> {
    project(
        root,
        "",
        "example/root",
        "  left = path \"left\"\n  right = path \"right\"\n",
    )?;
    project(root, "left", "example/left", "  dep = path \"../dep\"\n")?;
    project(
        root,
        "right",
        "example/right",
        &format!("  dep = path \"{right}\"\n"),
    )?;
    project(root, "dep", "example/dep", "")?;
    project(root, "other", "example/dep", "")
}

fn check(root: &Path, home: &Path) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_pith"))
        .args(["--output", "json", "check", "pith.pi"])
        .current_dir(root)
        .env("PITH_HOME", home)
        .env_remove("PITH_STORE")
        .env_remove("PITH_STATE")
        .env("NO_COLOR", "1")
        .output()
}

#[test]
fn check_accepts_equivalent_real_paths_without_creating_state() -> TestResult {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("project");
    let home = directory.path().join("home");
    fixture(&root, "../left/../dep")?;
    let output = check(&root, &home)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let records: Vec<serde_json::Value> = String::from_utf8(output.stdout)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    assert!(records.iter().any(|record| {
        record
            .pointer("/query/errors")
            .and_then(serde_json::Value::as_u64)
            == Some(0)
    }));
    assert!(!home.exists(), "check must not initialize engine state");
    Ok(())
}

#[test]
fn check_reports_conflicting_real_paths_with_both_project_sources() -> TestResult {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("project");
    let home = directory.path().join("home");
    fixture(&root, "../other")?;
    let output = check(&root, &home)?;
    assert!(!output.status.success());
    let records: Vec<serde_json::Value> = String::from_utf8(output.stdout)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let diagnostics = records
        .iter()
        .find_map(|record| record.pointer("/query/diagnostics"))
        .and_then(serde_json::Value::as_array)
        .ok_or("check must report its diagnostics")?;
    let labels: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.get("code").and_then(serde_json::Value::as_u64) == Some(3061))
        .filter_map(|diag| diag.get("label").and_then(serde_json::Value::as_str))
        .collect();
    assert_eq!(
        labels,
        [
            root.join("right/pith.pi").to_string_lossy(),
            root.join("left/pith.pi").to_string_lossy()
        ]
    );
    assert!(!home.exists());
    Ok(())
}
