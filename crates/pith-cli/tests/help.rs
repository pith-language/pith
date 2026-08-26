//! The help text, as a person reads it.
//!
//! Cheaper than adding `trycmd`, and it is what catches a flag that quietly
//! moved between the globals and a command, or a group that grew a verb
//! without a durable object kind behind it.

use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn help(arguments: &[&str]) -> std::io::Result<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_pith"))
        .args(arguments)
        .arg("--help")
        // `NO_COLOR` so the snapshot is the text and not the escape sequences,
        // and a fixed width so it does not move with the terminal running it.
        .env("NO_COLOR", "1")
        .env("COLUMNS", "100")
        .output()?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[test]
fn the_root_help_is_stable() -> TestResult {
    let text = help(&[])?;
    assert!(
        !text.contains("debug"),
        "the unstable debug surface leaked into root help: {text}"
    );
    insta::assert_snapshot!(text);
    Ok(())
}

#[test]
fn the_store_group_help_is_stable() -> TestResult {
    insta::assert_snapshot!(help(&["store"])?);
    Ok(())
}

#[test]
fn the_state_group_help_is_stable() -> TestResult {
    insta::assert_snapshot!(help(&["state"])?);
    Ok(())
}

#[test]
fn the_gc_help_is_stable() -> TestResult {
    insta::assert_snapshot!(help(&["gc"])?);
    Ok(())
}

#[test]
fn the_check_help_is_stable() -> TestResult {
    insta::assert_snapshot!(help(&["check"])?);
    Ok(())
}

#[test]
fn the_fmt_help_is_stable() -> TestResult {
    insta::assert_snapshot!(help(&["fmt"])?);
    Ok(())
}

/// The globals are `global = true`, which is what puts them after the
/// subcommand, where people type them. A flag that stopped propagating would
/// vanish from a subcommand's help without any other test noticing.
#[test]
fn every_global_reaches_every_command() -> TestResult {
    for command in [
        "check",
        "explore",
        "fmt",
        "store add",
        "store cat",
        "state info",
        "state check",
        "gc",
    ] {
        let arguments: Vec<&str> = command.split(' ').collect();
        let text = help(&arguments)?;
        for flag in ["--output", "--store", "--state"] {
            assert!(
                text.contains(flag),
                "`pith {command} --help` does not offer `{flag}`"
            );
        }
    }
    Ok(())
}

/// `env = ...` is what gives `PITH_STORE` and `PITH_STATE` without a
/// hand-rolled fallback, and clap renders the variable name in help only when
/// the feature is actually on.
#[test]
fn the_store_and_state_globals_name_their_environment_variables() -> TestResult {
    let text = help(&["check"])?;

    assert!(text.contains("PITH_STORE"), "{text}");
    assert!(text.contains("PITH_STATE"), "{text}");
    Ok(())
}
