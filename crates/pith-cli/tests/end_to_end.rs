//! The binary, driven the way a shell drives it.
//!
//! `CARGO_BIN_EXE_pith` locates the built binary, so these need no
//! `assert_cmd`. What they pin is the part no unit test can see: the exit code
//! a script branches on, and the JSON a machine parses.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct Run {
    output: Output,
}

impl Run {
    fn code(&self) -> i32 {
        self.output.status.code().unwrap_or(-1)
    }

    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout).into_owned()
    }

    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.output.stderr).into_owned()
    }

    /// Every JSON line the run produced, parsed.
    fn records(&self) -> TestResult<Vec<serde_json::Value>> {
        self.stdout()
            .lines()
            .map(|line| {
                serde_json::from_str(line).map_err(|error| {
                    test_error(format!("`{line}` is not one JSON record: {error}"))
                })
            })
            .collect()
    }

    /// The single query record, if there is one.
    fn query(&self) -> TestResult<Option<serde_json::Value>> {
        Ok(self
            .records()?
            .into_iter()
            .find(|record| record.get("kind").and_then(serde_json::Value::as_str) == Some("query"))
            .and_then(|record| record.get("query").cloned()))
    }
}

fn pith(home: &Path, arguments: &[&str]) -> io::Result<Run> {
    let output = pith_command(home, arguments).output()?;
    Ok(Run { output })
}

fn pith_command(home: &Path, arguments: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pith"));
    command
        .args(arguments)
        .env("PITH_HOME", home)
        // Cleared so the ambient developer environment cannot reach the run.
        .env_remove("PITH_STORE")
        .env_remove("PITH_STATE")
        .env_remove("XDG_CACHE_HOME")
        .env_remove("NO_COLOR")
        .env_remove("FORCE_COLOR")
        .env_remove("CLICOLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("TTY_FORCE");
    command
}

fn workspace_file(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn write(directory: &Path, name: &str, text: &str) -> io::Result<PathBuf> {
    let path = directory.join(name);
    std::fs::write(&path, text)?;
    Ok(path)
}

/// A throwaway directory. Used both as a store root and as a place to put
/// fixture files, so the ambient home never takes part.
fn scratch() -> io::Result<TempDir> {
    TempDir::new()
}

fn test_error(message: impl Into<String>) -> Box<dyn std::error::Error> {
    io::Error::other(message.into()).into()
}

#[test]
fn a_module_that_elaborates_succeeds_and_reports_its_abi() -> TestResult {
    let home = scratch()?;
    let module = workspace_file("crates/xylem/xylem.pi");

    let run = pith(
        home.path(),
        &["--output", "json", "check", &module.display().to_string()],
    )?;

    assert_eq!(run.code(), 0, "{}", run.stderr());
    let query = run.query()?.ok_or_else(|| test_error("no query record"))?;
    assert_eq!(
        query.get("view").and_then(serde_json::Value::as_str),
        Some("check")
    );
    assert_eq!(
        query.get("errors").and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert!(
        query
            .get("abi_digest")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|digest| digest.len() == 64),
        "a module that elaborated reported no ABI digest: {query}"
    );
    Ok(())
}

/// `check` is the one command that has to produce useful output when
/// elaboration fails. A non-zero exit with no report would be a worse
/// failure than the one being reported.
#[test]
fn a_module_that_does_not_elaborate_still_reports_before_failing() -> TestResult {
    let home = scratch()?;
    let source = scratch()?;
    let module = write(source.path(), "broken.pi", "nominal Thing = Nope\n")?;

    let run = pith(
        home.path(),
        &["--output", "json", "check", &module.display().to_string()],
    )?;

    assert_eq!(run.code(), 1, "{}", run.stderr());
    let query = run.query()?.ok_or_else(|| test_error("no query record"))?;
    assert!(
        query
            .get("errors")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|errors| errors > 0),
        "the report claimed no errors while the run failed: {query}"
    );
    assert_eq!(
        query.get("abi_digest"),
        Some(&serde_json::Value::Null),
        "a module that did not elaborate reported an ABI digest"
    );
    Ok(())
}

/// Severity lives in `check`. Without the flag, warnings are reported and the
/// run succeeds; with it, the same report exits non-zero.
#[test]
fn denying_warnings_changes_the_exit_code_and_not_the_report() -> TestResult {
    let home = scratch()?;
    let module = workspace_file("crates/xylem/xylem.pi");
    let path = module.display().to_string();

    let permissive = pith(home.path(), &["--output", "json", "check", &path])?;
    let denying = pith(
        home.path(),
        &["--output", "json", "check", &path, "--deny", "warnings"],
    )?;

    assert_eq!(permissive.query()?, denying.query()?, "the report differed");
    assert_eq!(permissive.code(), 0);
    // The corpus module warns about nothing today, so denying changes nothing.
    // The flag is pinned here so the day a lint lands, this test says which
    // half moved.
    let warnings = permissive
        .query()?
        .and_then(|query| query.get("warnings").and_then(serde_json::Value::as_u64))
        .unwrap_or_default();
    assert_eq!(denying.code(), i32::from(warnings > 0));
    Ok(())
}

#[test]
fn explore_reports_the_tier_that_answers_each_rule() -> TestResult {
    let home = scratch()?;
    let module = workspace_file("crates/xylem/xylem.pi");

    let run = pith(
        home.path(),
        &["--output", "json", "explore", &module.display().to_string()],
    )?;

    assert_eq!(run.code(), 0, "{}", run.stderr());
    let query = run.query()?.ok_or_else(|| test_error("no query record"))?;
    let rules = query
        .get("rules")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| test_error(format!("no rules in {query}")))?;
    assert!(!rules.is_empty());
    for rule in rules {
        assert_eq!(
            rule.get("tier").and_then(serde_json::Value::as_str),
            Some("host"),
            "the rule did not report its implementation tier: {rule}"
        );
    }
    Ok(())
}

#[test]
fn entry_commands_share_the_versioned_evaluation_and_recorded_graph() -> TestResult {
    let home = scratch()?;
    let source = scratch()?;
    let module = write(
        source.path(),
        "root.pi",
        "pure rule echo(value: Text) -> Text = { value }\n\nentry main : Text = ask (\"hello\")\n",
    )?;
    let path = module.display().to_string();

    let selected = pith(
        home.path(),
        &[
            "--output", "json", "graph", "select", "main", "--module", &path,
        ],
    )?;
    assert_eq!(selected.code(), 0, "{}", selected.stderr());
    assert_eq!(
        selected
            .query()?
            .and_then(|query| query.get("rule").cloned()),
        Some(serde_json::Value::from("root::entry.main"))
    );
    assert!(
        !home.path().join("state.db").exists(),
        "read-only selection created durable state"
    );

    let first = pith(
        home.path(),
        &["--output", "json", "run", "main", "--module", &path],
    )?;
    let second = pith(
        home.path(),
        &["--output", "json", "run", "main", "--module", &path],
    )?;
    assert_eq!(first.code(), 0, "{}", first.stderr());
    assert_eq!(second.code(), 0, "{}", second.stderr());
    assert_eq!(
        first
            .query()?
            .and_then(|query| query.get("source").cloned()),
        Some(serde_json::Value::from("computed"))
    );
    let second_query = second.query()?.ok_or_else(|| test_error("no run query"))?;
    assert_eq!(
        second_query
            .get("source")
            .and_then(serde_json::Value::as_str),
        Some("hydrated")
    );
    assert_eq!(
        second_query
            .pointer("/value/s")
            .and_then(serde_json::Value::as_str),
        Some("hello")
    );

    let dependencies = pith(
        home.path(),
        &[
            "--output", "json", "graph", "deps", "main", "--module", &path,
        ],
    )?;
    assert_eq!(dependencies.code(), 0, "{}", dependencies.stderr());
    let dependency_query = dependencies
        .query()?
        .ok_or_else(|| test_error("no dependency query"))?;
    assert_eq!(
        dependency_query
            .pointer("/root/status")
            .and_then(serde_json::Value::as_str),
        Some("complete")
    );
    assert!(
        dependency_query
            .pointer("/root/children")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|children| !children.is_empty()),
        "the recorded dependency subtree was empty: {dependency_query}"
    );

    let explained = pith(
        home.path(),
        &["--output", "json", "explain", "main", "--module", &path],
    )?;
    assert_eq!(explained.code(), 0, "{}", explained.stderr());
    assert!(explained.records()?.iter().any(|record| {
        record.get("kind").and_then(serde_json::Value::as_str) == Some("explain")
            && record
                .get("steps")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|steps| !steps.is_empty())
    }));
    Ok(())
}

#[test]
fn graph_plan_refuses_an_unbound_domain_action_by_coordinate() -> TestResult {
    let home = scratch()?;
    let source = scratch()?;
    let module = write(
        source.path(),
        "root.pi",
        "action rule compile(Text) -> Text = host\n\npure rule build(name: Text) -> Text = {\n  run Text (name)\n}\n\nentry main : Text = ask (\"input\")\n",
    )?;

    let run = pith(
        home.path(),
        &[
            "graph",
            "plan",
            "main",
            "--module",
            &module.display().to_string(),
        ],
    )?;

    assert_eq!(run.code(), 1, "{}", run.stderr());
    assert!(run.stderr().contains("root.compile"), "{}", run.stderr());
    assert!(
        run.stderr().contains("links no domain crate"),
        "{}",
        run.stderr()
    );
    Ok(())
}

#[test]
fn exec_reports_the_only_return_from_process_replacement() -> TestResult {
    let home = scratch()?;
    let source = scratch()?;
    let module = write(
        source.path(),
        "root.pi",
        "import pith\n\npure rule command(program: Text) -> pith.Exec = {\n  Exec({arguments: [\"fixture\"], program: program})\n}\n\nentry main : pith.Exec = ask (\"/path/that/does/not/exist\")\n",
    )?;

    let run = pith(
        home.path(),
        &["exec", "main", "--module", &module.display().to_string()],
    )?;

    assert_eq!(run.code(), 1, "{}", run.stderr());
    assert!(
        run.stderr().contains("cannot exec the derived program"),
        "{}",
        run.stderr()
    );
    Ok(())
}

#[test]
fn m14_workspace_commands_are_visible_refusals() -> TestResult {
    let home = scratch()?;
    for command in ["diff", "update", "add"] {
        let run = pith(home.path(), &[command])?;
        assert_eq!(run.code(), 1, "{}", run.stderr());
        assert!(
            run.stderr().contains("requires a workspace"),
            "{}",
            run.stderr()
        );
    }
    Ok(())
}

/// A tree `pith store add` produced and a tree an action captured are the same
/// kind of thing, so the walk has to agree with the executor's capture path:
/// a symlink is an entry in its own right, and the executable bit is part of
/// the identity.
#[test]
fn a_directory_round_trips_through_the_store_with_its_links_and_modes() -> TestResult {
    let home = scratch()?;
    let source = scratch()?;
    let package = source.path().join("pkg");
    std::fs::create_dir_all(package.join("include"))?;
    write(&package, "main.c", "int main(){}\n")?;
    write(&package.join("include"), "a.h", "#pragma once\n")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        symlink("../build/main", package.join("latest"))?;
        let script = write(&package, "run.sh", "#!/bin/sh\n")?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))?;
    }

    let added = pith(
        home.path(),
        &[
            "--output",
            "json",
            "store",
            "add",
            &package.display().to_string(),
        ],
    )?;
    assert_eq!(added.code(), 0, "{}", added.stderr());
    let query = added
        .query()?
        .ok_or_else(|| test_error("no query record"))?;
    assert_eq!(
        query.get("kind").and_then(serde_json::Value::as_str),
        Some("tree"),
        "a directory was admitted as something other than a tree"
    );
    let id = query
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| test_error(format!("no identity in {query}")))?
        .to_owned();

    // Adding the same bytes again derives the same identity: the filesystem's
    // unspecified iteration order does not reach the tree.
    let again = pith(
        home.path(),
        &[
            "--output",
            "json",
            "store",
            "add",
            &package.display().to_string(),
        ],
    )?;
    assert_eq!(
        again.query()?.and_then(|query| query.get("id").cloned()),
        Some(serde_json::Value::String(id.clone()))
    );

    let listed = pith(home.path(), &["--output", "json", "store", "ls", &id])?;
    assert_eq!(listed.code(), 0, "{}", listed.stderr());
    let entries = listed
        .query()?
        .and_then(|query| query.get("entries").cloned())
        .ok_or_else(|| test_error("no entries"))?;
    let empty = Vec::new();
    let kinds: Vec<(&str, &str)> = entries
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter_map(|entry| {
            Some((
                entry.get("name")?.as_str()?,
                entry.get("entry_kind")?.as_str()?,
            ))
        })
        .collect();
    assert!(
        kinds.contains(&("include", "tree")) && kinds.contains(&("main.c", "file")),
        "{kinds:?}"
    );
    #[cfg(unix)]
    assert!(
        kinds.contains(&("latest", "symlink")),
        "a symlink was followed rather than stored as an entry: {kinds:?}"
    );

    let destination = source.path().join("rendered");
    let rendered = pith(
        home.path(),
        &[
            "store",
            "materialize",
            &id,
            &destination.display().to_string(),
        ],
    )?;
    assert_eq!(rendered.code(), 0, "{}", rendered.stderr());
    assert_eq!(
        std::fs::read_to_string(destination.join("include/a.h")).ok(),
        Some("#pragma once\n".to_owned())
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::read_link(destination.join("latest")).ok(),
            Some(PathBuf::from("../build/main")),
            "the link did not survive the round trip"
        );
        let mode = std::fs::metadata(destination.join("run.sh"))
            .map(|metadata| metadata.permissions().mode() & 0o111)
            .unwrap_or_default();
        assert_ne!(mode, 0, "the executable bit did not survive the round trip");
    }
    Ok(())
}

/// `store cat`'s stdout is the content. A record on the same stream would
/// corrupt the bytes it describes.
#[test]
fn cat_writes_the_blob_and_nothing_else() -> TestResult {
    let home = scratch()?;
    let source = scratch()?;
    let file = write(source.path(), "payload.txt", "the bytes\n")?;

    let added = pith(
        home.path(),
        &[
            "--output",
            "json",
            "store",
            "add",
            &file.display().to_string(),
        ],
    )?;
    let id = added
        .query()?
        .and_then(|query| {
            query
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .ok_or_else(|| test_error(format!("no identity: {}", added.stdout())))?;

    let read = pith(home.path(), &["--output", "json", "store", "cat", &id])?;

    assert_eq!(read.code(), 0, "{}", read.stderr());
    assert_eq!(read.stdout(), "the bytes\n");
    Ok(())
}

/// `fmt` writes the canonical spelling and `--check` verifies it, so the two
/// modes and the exit code a script branches on are pinned together: a
/// non-canonical module under `--check` is the refusal, and a second run over
/// canonical text is a no-op.
#[test]
fn fmt_writes_the_canonical_spelling_and_check_verifies_it() -> TestResult {
    let home = scratch()?;
    let source = scratch()?;
    let module = write(
        source.path(),
        "messy.pi",
        "nominal   Message   =   Text\n\n\npure rule   render(Message)   -> Text = host\n",
    )?;
    let path = module.display().to_string();

    let written = pith(home.path(), &["--output", "json", "fmt", &path])?;
    assert_eq!(written.code(), 0, "{}", written.stderr());
    let query = written
        .query()?
        .ok_or_else(|| test_error("no query record"))?;
    assert_eq!(
        query.get("view").and_then(serde_json::Value::as_str),
        Some("format")
    );
    assert_eq!(
        query.get("status").and_then(serde_json::Value::as_str),
        Some("formatted")
    );
    assert_eq!(
        std::fs::read_to_string(&module).ok(),
        Some("nominal Message = Text\n\npure rule render(Message) -> Text = host\n".to_owned())
    );

    let settled = pith(home.path(), &["--output", "json", "fmt", "--check", &path])?;
    assert_eq!(settled.code(), 0, "{}", settled.stderr());
    assert_eq!(
        settled
            .query()?
            .and_then(|query| query.get("status").cloned()),
        Some(serde_json::Value::from("unchanged"))
    );

    write(source.path(), "messy.pi", "nominal   Message   =   Text\n")?;
    let refused = pith(home.path(), &["--output", "json", "fmt", "--check", &path])?;
    assert_eq!(refused.code(), 1, "{}", refused.stderr());
    assert_eq!(
        refused
            .query()?
            .and_then(|query| query.get("status").cloned()),
        Some(serde_json::Value::from("would_format"))
    );
    assert_eq!(
        std::fs::read_to_string(&module).ok(),
        Some("nominal   Message   =   Text\n".to_owned()),
        "--check wrote to the file it was only naming"
    );
    Ok(())
}

/// The formatter's measured claim, on the one path a shell can reach it: a
/// formatted corpus module elaborates to the ABI digest it had before.
#[test]
fn a_formatted_module_keeps_its_abi_digest() -> TestResult {
    let home = scratch()?;
    let source = scratch()?;
    let module = write(
        source.path(),
        "xylem.pi",
        &std::fs::read_to_string(workspace_file("crates/xylem/xylem.pi"))?,
    )?;
    let before = pith(
        home.path(),
        &["--output", "json", "check", &module.display().to_string()],
    )?;

    let formatted = pith(
        home.path(),
        &["--output", "json", "fmt", &module.display().to_string()],
    )?;
    assert_eq!(formatted.code(), 0, "{}", formatted.stderr());

    let after = pith(
        home.path(),
        &["--output", "json", "check", &module.display().to_string()],
    )?;
    assert_eq!(
        before.query()?,
        after.query()?,
        "formatting moved the ABI digest"
    );
    Ok(())
}

/// The three outcomes a shell branches on. Content the store does not hold is
/// the caller's problem; a store that contradicts itself is not.
#[test]
fn the_three_exit_codes_are_distinguishable() -> TestResult {
    let home = scratch()?;
    let module = workspace_file("crates/xylem/xylem.pi");
    let absent = "0".repeat(64);

    let success = pith(home.path(), &["check", &module.display().to_string()])?;
    assert_eq!(success.code(), 0, "{}", success.stderr());

    let user = pith(home.path(), &["store", "cat", &absent])?;
    assert_eq!(user.code(), 1, "{}", user.stderr());

    // A blob file whose bytes do not hash to the name it is filed under. The
    // store re-derives the identity on read and refuses the mismatch, which is
    // a failure the caller cannot fix.
    let named = "a".repeat(64);
    let blobs = home.path().join("store/blobs");
    std::fs::create_dir_all(&blobs)?;
    write(&blobs, &named, "not what the name claims")?;
    let internal = pith(home.path(), &["store", "cat", &named])?;
    assert_eq!(internal.code(), 2, "{}", internal.stderr());
    Ok(())
}

/// `--store` overrides its half alone, which is what makes a hermetic run and
/// a test fixture possible without moving the other half.
#[test]
fn the_store_flag_overrides_only_its_half() -> TestResult {
    let home = scratch()?;
    let elsewhere = scratch()?;
    let source = scratch()?;
    let file = write(source.path(), "payload.txt", "elsewhere\n")?;

    let run = pith(
        home.path(),
        &[
            "--output",
            "json",
            "--store",
            &elsewhere.path().display().to_string(),
            "store",
            "add",
            &file.display().to_string(),
        ],
    )?;

    assert_eq!(run.code(), 0, "{}", run.stderr());
    assert!(
        elsewhere.path().join("blobs").is_dir(),
        "the blob did not land under --store"
    );
    assert!(
        !home.path().join("store").exists(),
        "the default store root was touched despite --store"
    );
    Ok(())
}

/// Every line of `--output json` is one record, and every query record names
/// the contract version it was written against.
#[test]
fn json_output_is_one_versioned_record_per_line() -> TestResult {
    let home = scratch()?;
    let module = workspace_file("crates/xylem/xylem.pi");

    let run = pith(
        home.path(),
        &["--output", "json", "explore", &module.display().to_string()],
    )?;

    let records = run.records()?;
    assert!(records.len() >= 3, "{records:?}");
    let query = records
        .iter()
        .find(|record| record.get("kind").and_then(serde_json::Value::as_str) == Some("query"))
        .ok_or_else(|| test_error("no query record"))?;
    assert!(
        query
            .get("api_version")
            .and_then(serde_json::Value::as_u64)
            .is_some(),
        "a query record went out without the contract version"
    );
    Ok(())
}

/// The renderer receives the profile detected at the CLI boundary. A pipe is
/// unstyled by default, while termprofile's documented force values let a
/// caller select and test each richer level deterministically.
#[test]
fn pretty_output_and_help_adapt_to_every_supported_color_depth() -> TestResult {
    let home = scratch()?;
    let module = workspace_file("crates/xylem/xylem.pi");
    let arguments = ["--output", "pretty", "check", &module.display().to_string()];

    let unforced = pith(home.path(), &arguments)?;
    assert_eq!(unforced.code(), 0, "{}", unforced.stderr());
    assert!(
        !unforced.stdout().contains("\u{1b}["),
        "a non-TTY received terminal escapes: {:?}",
        unforced.stdout()
    );

    for (profile, output_escape, help_escape) in [
        (
            "truecolor",
            "\u{1b}[38;2;79;128;95m",
            "\u{1b}[38;2;113;123;33m",
        ),
        ("ansi256", "\u{1b}[38;5;29m", "\u{1b}[38;5;64m"),
        ("ansi16", "\u{1b}[32m", "\u{1b}[32m"),
    ] {
        let output = pith_command(home.path(), &arguments)
            .env("FORCE_COLOR", profile)
            .output()?;
        let run = Run { output };
        assert_eq!(run.code(), 0, "{}", run.stderr());
        assert!(
            run.stdout().contains(output_escape),
            "{profile} did not use {output_escape:?}: {:?}",
            run.stdout()
        );

        let help = pith_command(home.path(), &["check", "--help"])
            .env("FORCE_COLOR", profile)
            .output()?;
        let help = Run { output: help };
        assert_eq!(help.code(), 0, "{}", help.stderr());
        assert!(
            help.stdout().contains(help_escape),
            "{profile} help did not use {help_escape:?}: {:?}",
            help.stdout()
        );
    }
    Ok(())
}

/// Debug reports intentionally bypass the record renderer: they are useful to
/// a developer looking at one terminal and are not a second machine API.
#[test]
fn terminal_debug_output_is_hidden_unversioned_and_pipe_safe() -> TestResult {
    let home = scratch()?;

    let run = pith(home.path(), &["debug", "terminal"])?;

    assert_eq!(run.code(), 0, "{}", run.stderr());
    assert!(
        run.stdout().contains("warning=unstable-debug-output\n"),
        "{}",
        run.stdout()
    );
    assert!(
        run.stdout().contains("stdout_is_terminal=false\n"),
        "{}",
        run.stdout()
    );
    assert!(
        run.stdout().contains("color_profile=no_tty\n"),
        "{}",
        run.stdout()
    );
    assert!(!run.stdout().contains("api_version"), "{}", run.stdout());
    assert!(!run.stdout().contains("[phase]"), "{}", run.stdout());
    Ok(())
}

/// A positional sharing an id with a global flag resolves to whichever clap
/// looked up last, and panics on the downcast. `debug_assert` does not catch
/// it, so it is pinned where it can be observed.
#[test]
fn a_positional_does_not_collide_with_a_global_flag() -> TestResult {
    let home = scratch()?;
    let absent = "0".repeat(64);

    let run = pith(
        home.path(),
        &["store", "materialize", &absent, "some-directory"],
    )?;

    assert_eq!(
        run.code(),
        1,
        "materialize did not reach its query: {}",
        run.stderr()
    );
    assert!(
        !run.stderr()
            .contains("Mismatch between definition and access"),
        "{}",
        run.stderr()
    );
    Ok(())
}

/// A machine that has recorded no engine state still gets a report, which is
/// what makes `state info` safe on a fresh checkout and in CI before any
/// build has run.
#[test]
fn state_info_reports_a_machine_that_recorded_nothing() -> TestResult {
    let home = scratch()?;

    let run = pith(home.path(), &["--output", "json", "state", "info"])?;

    assert_eq!(run.code(), 0, "{}", run.stderr());
    let query = run.query()?.ok_or_else(|| test_error("no query record"))?;
    assert_eq!(
        query.get("view").and_then(serde_json::Value::as_str),
        Some("state")
    );
    assert_eq!(
        query
            .pointer("/attempts/total")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        query
            .get("reusable_index")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        query
            .get("schema_version")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        query.get("adapter").and_then(serde_json::Value::as_str),
        Some("sqlite")
    );

    let check = pith(home.path(), &["--output", "json", "state", "check"])?;
    assert_eq!(check.code(), 0, "{}", check.stderr());
    assert_eq!(
        check
            .query()?
            .and_then(|query| query.get("records").cloned()),
        Some(serde_json::Value::from(0))
    );
    Ok(())
}

/// `pith gc` without `--dry-run` is a refusal and not a quiet no-op: nothing
/// in the tree prunes anything, and pretending otherwise would be the one
/// command whose failure mode is deleting evidence.
#[test]
fn gc_without_dry_run_refuses_rather_than_deleting() -> TestResult {
    let home = scratch()?;

    let run = pith(home.path(), &["gc"])?;

    assert_eq!(run.code(), 1, "{}", run.stderr());
    assert!(
        run.stderr().contains("--dry-run"),
        "the refusal did not name the flag: {}",
        run.stderr()
    );
    Ok(())
}

/// Content admitted through `store add` with no engine state behind it is the
/// reclaimable half of the dry run.
#[test]
fn gc_dry_run_names_admitted_content_as_reclaimable() -> TestResult {
    let home = scratch()?;
    let source = scratch()?;
    let file = write(
        source.path(),
        "payload.txt",
        "bytes a collection would drop\n",
    )?;

    let added = pith(
        home.path(),
        &[
            "--output",
            "json",
            "store",
            "add",
            &file.display().to_string(),
        ],
    )?;
    assert_eq!(added.code(), 0, "{}", added.stderr());

    let run = pith(home.path(), &["--output", "json", "gc", "--dry-run"])?;

    assert_eq!(run.code(), 0, "{}", run.stderr());
    let query = run.query()?.ok_or_else(|| test_error("no query record"))?;
    assert_eq!(
        query.get("roots").and_then(serde_json::Value::as_u64),
        Some(0),
        "a fresh machine has roots: {query}"
    );
    assert_eq!(
        query
            .pointer("/content/reclaimable_blobs")
            .and_then(serde_json::Value::as_u64),
        Some(1),
        "the admitted blob was not named reclaimable: {query}"
    );
    assert_eq!(
        query
            .pointer("/content/live_blobs")
            .and_then(serde_json::Value::as_u64),
        Some(0),
        "nothing retained the admitted blob: {query}"
    );
    Ok(())
}
