//! Toolchain closure discovery.
//!
//! Decision 0030 assigns closure discovery to the build library: the kernel
//! sees only the declared list of host paths in `ActionSpec::toolchain`, and
//! this module is what produces that list. Discovery runs before the engine
//! does, never inside `plan()` — ambient filesystem or process discovery during
//! evaluation is forbidden (decision 0007), so a toolchain is host
//! configuration the caller discovers and hands in.
//!
//! The nix store path is the supported case: `nix path-info --recursive` reads
//! the closure the store already recorded. Outside the store the loader trace
//! (`LD_TRACE_LOADED_OBJECTS`) covers what the loader opens and nothing a
//! program opens later, so [`Toolchain::discover`] declines rather than
//! returning a partial answer that would fail on an undeclared read.
//!
//! The closure walk shells out to the `nix` CLI, which is already on `PATH` in
//! any environment that has a nix-store toolchain to discover. The established
//! upgrade path if the subprocess becomes a cost is
//! `nix-bindings-store::Store::get_fs_closure`, which computes the same closure
//! in-process through the Nix C library; it links `libnix` at build time, which
//! is the trade-off that makes the CLI the lighter choice today.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use pith_core::Value;

use crate::types;

/// A discovered C toolchain: the driver the action execves, the closure the
/// executor confines it to, and the two search paths the driver needs to find
/// the rest of itself.
#[derive(Clone, Debug)]
pub struct Toolchain {
    /// Absolute host path of the `cc` driver the action execves.
    pub driver: Box<str>,
    /// Every host path the driver reads at runtime. Declared as
    /// `ActionSpec::toolchain`; the executor adds one landlock rule per path.
    pub closure: Box<[Box<str>]>,
    /// Where the driver looks for its own compiler proper before it looks at
    /// `PATH`, declared to the action as `COMPILER_PATH`.
    ///
    /// `None` for a driver that has no separate compiler to find. gcc execs
    /// `cc1`; clang compiles in its own process and answers
    /// `-print-prog-name=cc1` with a bare name, which is it saying there is no
    /// such program to declare.
    pub program_path: Option<Box<str>>,
    /// The directory the driver came from, where `as` and `ld` live on a
    /// distribution compiler.
    pub tool_directory: Box<str>,
}

/// The toolchains a build may use, resolved by the driver path a request names.
///
/// One registration of each rule serves every toolchain. The toolchain value in a
/// request carries the driver path, and `plan()` looks its closure up here.
/// Holding a single toolchain per registration would mean one engine per
/// compiler, and registering the same rule twice would give two rules one
/// interface and collide as `E-1102` (decision 0015). Neither makes a toolchain
/// a request input in more than name.
#[derive(Clone, Debug)]
pub struct Toolchains {
    entries: Box<[Toolchain]>,
}

impl Toolchains {
    /// A set over `entries`. A driver appearing twice is the caller's to avoid;
    /// resolution takes the first match, so a duplicate is shadowed.
    #[must_use]
    pub fn new(entries: Box<[Toolchain]>) -> Self {
        Self { entries }
    }

    /// The common case: a build over one discovered toolchain.
    #[must_use]
    pub fn one(toolchain: Toolchain) -> Self {
        Self {
            entries: Box::new([toolchain]),
        }
    }

    /// The toolchain whose driver is `driver`, or `None` when this build was not
    /// registered with it.
    #[must_use]
    pub fn resolve(&self, driver: &str) -> Option<&Toolchain> {
        self.entries
            .iter()
            .find(|toolchain| toolchain.driver.as_ref() == driver)
    }

    /// The registered toolchains, in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &Toolchain> {
        self.entries.iter()
    }
}

/// Why toolchain discovery did not produce a usable [`Toolchain`].
#[derive(Debug)]
pub enum DiscoveryError {
    /// No program of that name on `PATH` or at that path.
    NotFound,
    /// The driver is outside `/nix/store`, where discovery cannot see past the
    /// loader to `cc1` and the fixed includes.
    Incomplete,
    /// A discovery command failed to run or returned a non-zero status.
    CommandFailed(String),
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => formatter.write_str("no toolchain driver found on PATH"),
            Self::Incomplete => {
                formatter.write_str("the driver is outside /nix/store; closure is not discoverable")
            }
            Self::CommandFailed(message) => {
                write!(formatter, "toolchain discovery command failed: {message}")
            }
        }
    }
}

impl std::error::Error for DiscoveryError {}

impl Toolchain {
    /// Discover the toolchain whose driver is found on `PATH` under
    /// `driver_name` (for example `cc`).
    ///
    /// # Errors
    /// [`DiscoveryError::NotFound`] when the driver is absent,
    /// [`DiscoveryError::Incomplete`] when it is outside `/nix/store`,
    /// [`DiscoveryError::CommandFailed`] when a discovery subprocess fails.
    pub fn discover(driver_name: &str) -> Result<Self, DiscoveryError> {
        let driver_path = find_in_path(driver_name).ok_or(DiscoveryError::NotFound)?;
        let driver = driver_path.to_str().ok_or(DiscoveryError::Incomplete)?;
        let store_root = nix_store_root(driver).ok_or(DiscoveryError::Incomplete)?;
        let program_path = match print_program_path(&driver_path, "cc1")? {
            Some(cc1) => Some(directory_of(&cc1).ok_or(DiscoveryError::Incomplete)?),
            None => None,
        };
        let tool_directory = directory_of(&driver_path).ok_or(DiscoveryError::Incomplete)?;
        let mut closure = BTreeSet::new();
        closure.extend(nix_closure(&store_root)?);
        Ok(Self {
            driver: driver.into(),
            closure: closure.into_iter().map(Into::into).collect(),
            program_path,
            tool_directory,
        })
    }

    /// The nominal toolchain value pairing with this toolchain — the driver
    /// path as its identity, for the rule graph to dispatch on.
    #[must_use]
    pub fn value(&self) -> Value {
        types::toolchain(&self.driver)
    }
}

fn find_in_path(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        return PathBuf::from(program)
            .is_file()
            .then(|| PathBuf::from(program));
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

/// Ask the driver where it keeps one of its own programs. `Ok(None)` is the
/// driver answering with a bare name, which is how it says it has no such
/// program of its own: there is nothing to declare, and nothing missing.
fn print_program_path(driver: &Path, program: &str) -> Result<Option<PathBuf>, DiscoveryError> {
    let output = Command::new(driver)
        .arg(format!("-print-prog-name={program}"))
        .output()
        .map_err(|error| DiscoveryError::CommandFailed(error.to_string()))?;
    if !output.status.success() {
        return Err(DiscoveryError::CommandFailed(format!(
            "`{driver:?} -print-prog-name={program}` exited {status}",
            status = output.status
        )));
    }
    let answer = String::from_utf8_lossy(&output.stdout);
    let answer = PathBuf::from(answer.trim());
    Ok(answer.is_absolute().then_some(answer))
}

fn directory_of(path: &Path) -> Option<Box<str>> {
    Some(path.parent()?.to_str()?.into())
}

/// `/nix/store/<hash>-gcc-wrapper-15.2.0/bin/cc` reduces to
/// `/nix/store/<hash>-gcc-wrapper-15.2.0`.
fn nix_store_root(path: &str) -> Option<String> {
    const STORE_PREFIX: &str = "/nix/store/";
    let entry = path.strip_prefix(STORE_PREFIX)?.split('/').next()?;
    (!entry.is_empty()).then(|| format!("{STORE_PREFIX}{entry}"))
}

fn nix_closure(store_root: &str) -> Result<Vec<String>, DiscoveryError> {
    let output = Command::new("nix")
        .args(["path-info", "--recursive", store_root])
        .output()
        .map_err(|error| DiscoveryError::CommandFailed(error.to_string()))?;
    if !output.status.success() {
        return Err(DiscoveryError::CommandFailed(format!(
            "`nix path-info --recursive {store_root}` exited {status}: {stderr}",
            status = output.status,
            stderr = String::from_utf8_lossy(&output.stderr)
        )));
    }
    let closure: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    if closure.is_empty() {
        return Err(DiscoveryError::CommandFailed(format!(
            "the closure of {store_root} came back empty"
        )));
    }
    Ok(closure)
}
