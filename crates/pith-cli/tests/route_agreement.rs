//! Real filesystem canonicalization, driven through the check command.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn module(root: &Path, name: &str, manifest: &str) -> std::io::Result<()> {
    let directory = root.join(name);
    fs::create_dir_all(directory.join("src"))?;
    fs::write(directory.join("module.pi"), manifest)?;
    fs::write(directory.join("src/main.pi"), "nominal Name = Text\n")
}

fn fixture(root: &Path, right: &str) -> std::io::Result<()> {
    module(
        root,
        "",
        "module example/root 1\nuse left = example/left from path \"left\"\nuse right = example/right from path \"right\"\n",
    )?;
    module(
        root,
        "left",
        "module example/left 1\nuse dep = example/dep from path \"../dep\"\n",
    )?;
    module(
        root,
        "right",
        &format!("module example/right 1\nuse dep = example/dep from path \"{right}\"\n"),
    )?;
    module(root, "dep", "module example/dep 1\n")?;
    module(root, "other", "module example/dep 2\n")
}

fn check(root: &Path, home: &Path) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_pith"))
        .args(["--output", "json", "check", "module.pi"])
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
fn check_reports_conflicting_real_paths_with_both_manifest_sources() -> TestResult {
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
            root.join("right/module.pi").to_string_lossy(),
            root.join("left/module.pi").to_string_lossy()
        ]
    );
    assert!(!home.exists());
    Ok(())
}
