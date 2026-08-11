//! Toolchain closure discovery for the executor tests.
//!
//! Decision 0030 gives this to the build library milestone M-3 opens. Until that
//! exists the tests need the same answer, so it lives here rather than in the
//! executor, which confines the closure it is handed and never discovers one.

#![allow(
    dead_code,
    reason = "each integration-test binary uses the subset of these helpers its fixtures need"
)]

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

/// Every host path `programs` need at run time, as `ActionSpec::toolchain`: each
/// program's own path plus what it pulls in. A bare name resolves through
/// `PATH`, the way the child's shell resolves it.
///
/// An action's `executable` is granted separately and need not appear here. The
/// tools a fixture *script* runs do, since nothing else in the contract names
/// them.
pub fn closure_for(programs: &[&str]) -> Box<[Box<str>]> {
    let mut closure = BTreeSet::new();
    for program in programs {
        let Some(resolved) = resolve(program) else {
            unreachable!("no program named `{program}` on PATH or at that path");
        };
        match nix_store_root(&resolved) {
            Some(store_root) => closure.extend(nix_closure(&store_root)),
            None => {
                closure.extend(loaded_objects(&resolved));
                closure.insert(resolved);
            }
        }
    }
    closure.into_iter().map(Into::into).collect()
}

/// The directories holding `programs`, for a child's `PATH`. Resolved the same
/// way [`closure_for`] resolves them, so the child looks a tool up where the
/// closure declared it.
pub fn path_for(programs: &[&str]) -> String {
    let directories: Vec<String> = programs
        .iter()
        .filter_map(|program| resolve(program))
        .filter_map(|resolved| {
            PathBuf::from(&resolved)
                .parent()
                .and_then(|parent| parent.to_str().map(str::to_owned))
        })
        .collect();
    let mut seen = BTreeSet::new();
    directories
        .into_iter()
        .filter(|directory| seen.insert(directory.clone()))
        .collect::<Vec<_>>()
        .join(":")
}

/// The absolute path of `program`, resolved the way [`closure_for`] resolves it.
/// For a script that must name a tool rather than look it up on `PATH`.
pub fn program_path(program: &str) -> String {
    match resolve(program) {
        Some(resolved) => resolved,
        None => unreachable!("no program named `{program}` on PATH or at that path"),
    }
}

/// Whether [`closure_for`] can describe `program` completely. True under
/// `/nix/store`, where nix records the closure. False elsewhere, where the
/// answer covers what the loader opens and nothing a program opens later.
pub fn closure_is_complete_for(program: &str) -> bool {
    resolve(program)
        .and_then(|resolved| nix_store_root(&resolved))
        .is_some()
}

fn resolve(program: &str) -> Option<String> {
    if program.contains('/') {
        return PathBuf::from(program).is_file().then(|| program.to_owned());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| candidate.to_str().map(str::to_owned))
}

/// `/nix/store/<hash>-gcc-wrapper-15.2.0/bin/cc` reduces to
/// `/nix/store/<hash>-gcc-wrapper-15.2.0`.
fn nix_store_root(path: &str) -> Option<String> {
    const STORE_PREFIX: &str = "/nix/store/";
    let entry = path.strip_prefix(STORE_PREFIX)?.split('/').next()?;
    (!entry.is_empty()).then(|| format!("{STORE_PREFIX}{entry}"))
}

fn nix_closure(store_root: &str) -> Vec<String> {
    let output = match Command::new("nix")
        .args(["path-info", "--recursive", store_root])
        .output()
    {
        Ok(output) => output,
        Err(error) => unreachable!("`nix path-info` did not run: {error}"),
    };
    assert!(
        output.status.success(),
        "`nix path-info --recursive {store_root}` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let closure: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    assert!(
        !closure.is_empty(),
        "the closure of {store_root} came back empty"
    );
    closure
}

/// What `program`'s dynamic loader opens to start it.
///
/// `LD_TRACE_LOADED_OBJECTS` asks the loader named in the binary's own
/// `PT_INTERP` to report what it would load and exit. `ldd` cannot answer this:
/// it picks a loader from `PATH`, so inside a nix devshell it resolves a
/// distribution binary against nix's glibc and names libraries the binary will
/// never open. A static binary reports nothing, which is the right answer.
fn loaded_objects(program: &str) -> Vec<String> {
    let output = match Command::new(program)
        .env("LD_TRACE_LOADED_OBJECTS", "1")
        .output()
    {
        Ok(output) => output,
        Err(error) => unreachable!("the loader did not trace {program}: {error}"),
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .flat_map(loaded_paths)
        .collect()
}

/// The paths one trace line names. A resolved library gives one
/// (`libc.so.6 => /lib/libc.so.6 (0x…)`). The loader gives its own path bare
/// (`/lib64/ld-linux-x86-64.so.2 (0x…)`). A line whose left side is already a
/// path gives both, since the kernel opens the name and the loader opens the
/// target. The vdso names no file and gives none.
fn loaded_paths(line: &str) -> Vec<String> {
    let (left, right) = match line.split_once(" => ") {
        Some((left, right)) => (left.trim(), Some(right)),
        None => (line.trim(), None),
    };
    let mut paths = Vec::new();
    if left.starts_with('/') {
        paths.push(strip_address(left).to_owned());
    }
    if let Some(right) = right {
        let target = strip_address(right);
        if target.starts_with('/') {
            paths.push(target.to_owned());
        }
    }
    paths
}

/// Drop the ` (0x…)` load address the loader prints after a path.
fn strip_address(field: &str) -> &str {
    field.split(" (").next().unwrap_or(field).trim()
}
