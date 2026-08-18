//! Discovering the paths the assembly tools need at run time.
//!
//! Decision 0030 gives closure discovery to the library that declares the
//! tools, and this library declares five: a shell, `mkdir`, `cat`, `chmod`,
//! and `ln`. The closure is what a confined child opens to run them — the ELF
//! interpreter above all — and a contract that names only the binaries cannot
//! start, which is 0030's own finding measured again on the assembly path.
//!
//! Under `/nix/store` the answer is exact, because nix records it: one
//! `path-info --recursive` per store root. Elsewhere the answer covers what
//! the loader opens and nothing a program opens later, the same limit the
//! executor's own fixtures record, and a caller that needs more declares it.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

/// Every host path `tools` need at run time. `tools` are absolute program
/// paths. The answer is sorted and duplicate-free, so it is a function of the
/// set of tools.
#[must_use]
pub fn tools_closure(tools: &[&str]) -> Vec<String> {
    let mut closure = BTreeSet::new();
    for tool in tools {
        let Some(store_root) = nix_store_root(tool) else {
            closure.extend(loaded_objects(tool));
            closure.insert((*tool).to_owned());
            continue;
        };
        if let Some(paths) = nix_closure(&store_root) {
            closure.extend(paths);
        } else {
            closure.extend(loaded_objects(tool));
            closure.insert((*tool).to_owned());
        }
    }
    closure.into_iter().collect()
}

/// `/nix/store/<hash>-coreutils-9.11/bin/cat` reduces to
/// `/nix/store/<hash>-coreutils-9.11`.
fn nix_store_root(path: &str) -> Option<String> {
    const STORE_PREFIX: &str = "/nix/store/";
    let entry = path.strip_prefix(STORE_PREFIX)?.split('/').next()?;
    (!entry.is_empty()).then(|| format!("{STORE_PREFIX}{entry}"))
}

/// The recursive closure nix records for `store_root`, or `None` when nix
/// cannot answer on this host.
fn nix_closure(store_root: &str) -> Option<Vec<String>> {
    let output = Command::new("nix")
        .args(["path-info", "--recursive", store_root])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let closure: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    (!closure.is_empty()).then_some(closure)
}

/// What `program`'s dynamic loader opens to start it. Asking the loader named
/// in the binary's own `PT_INTERP` (`LD_TRACE_LOADED_OBJECTS`) is the answer
/// that matches what the kernel opens; `ldd` picks a loader from `PATH` and
/// can name libraries the binary never loads. A static binary reports
/// nothing, which is the right answer.
fn loaded_objects(program: &str) -> Vec<String> {
    let output = match Command::new(program)
        .env("LD_TRACE_LOADED_OBJECTS", "1")
        .output()
    {
        Ok(output) => output,
        Err(_) => return Vec::new(),
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .flat_map(loaded_paths)
        .collect()
}

/// The paths one trace line names: a resolved library gives one, the loader
/// names its own path bare, and a line whose left side is a path gives both,
/// since the kernel opens the name and the loader opens the target.
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

/// The absolute path of `name`, resolved through `PATH` the way a child
/// would, for a caller that has a name rather than a path.
#[must_use]
pub fn resolve_tool(name: &str) -> Option<String> {
    if name.contains('/') {
        return PathBuf::from(name).is_file().then(|| name.to_owned());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| candidate.to_str().map(str::to_owned))
}
