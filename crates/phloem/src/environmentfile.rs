use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use pith_core::Value;
use pith_diag::PithResult;

use crate::codec::FIELD_TOOLCHAIN;
use crate::diag;
use crate::environment::{DEFAULT_ENVIRONMENT, EnvironmentDocument};
use crate::locktext::{SHA256, token};

const DEFAULT_LOCK_FILE: &str = "pith.lock";
const DEFAULT_RECORD_FILE: &str = "pith.env";
const LOCK_SUFFIX: &str = ".pith.lock";
const RECORD_SUFFIX: &str = ".pith.env";
const LOCK: &str = "lock";
const PLATFORM: &str = "platform";
const ENV: &str = "env";
const SUBSTITUTE: &str = "substitute";

/// The toolchain in this format's token spelling. The declaration's type is
/// xylem's nominal toolchain, whose representation is the driver path, and
/// the driver gets the same quoting every other text field on the line
/// gets, so a path with a space renders a line this grammar still accepts.
fn toolchain_token(toolchain: &Value) -> String {
    let driver = match toolchain {
        Value::Nominal { representation, .. } => match representation.as_ref() {
            Value::Text(driver) => return token(driver),
            // A toolchain value xylem never produces, quoted so the line
            // still parses.
            other => other.describe(),
        },
        other => other.describe(),
    };
    token(&driver)
}

/// The environment's rendered record: its projection, deterministic and
/// line-oriented, on the terms of the lock's file. The text names the lock
/// by digest, the realization coordinates, and every served substitution;
/// the lock's own file is unchanged by all of it and stays the place the
/// selection is read.
///
/// The record is a report over the value, not a format with a reader: the
/// document is the artifact that crosses processes, and a consumer for
/// this text waits until one exists (0043's unresolved).
#[must_use]
pub fn render(document: &EnvironmentDocument) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{ENV} {} {PLATFORM} {}/{} {FIELD_TOOLCHAIN} {} {LOCK} {SHA256}{}",
        token(&document.name),
        document.platform.operating_system,
        document.platform.architecture,
        toolchain_token(&document.toolchain),
        document.lock.content_id().digest(),
    );
    let mut lines: Vec<String> = document
        .substitutions
        .iter()
        .map(|substitution| {
            format!(
                "{SUBSTITUTE} {} {} {} [{}] {SHA256}{} {}",
                token(substitution.package.identity().domain().as_str()),
                token(substitution.package.identity().name()),
                token(substitution.package.version()),
                substitution
                    .features
                    .iter()
                    .map(|feature| token(feature))
                    .collect::<Vec<_>>()
                    .join(","),
                substitution.measured.digest(),
                substitution.authorized_by,
            )
        })
        .collect();
    lines.sort();
    for line in lines {
        let _ = writeln!(out, "{line}");
    }
    out
}

/// The derivation's one rule: a declared environment name becomes a file
/// name beside the lock, so it must be a single path component. A name with
/// a separator or a `..` would derive a path outside the project root. The
/// refusal surfaces at the derivation, before any caller acts on a path.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming the name and why it cannot name an
/// environment when it is empty or not one path component.
fn component_name(environment: &str) -> PithResult<()> {
    let mut components = Path::new(environment).components();
    let single = matches!(
        (components.next(), components.next()),
        (Some(std::path::Component::Normal(_)), None)
    );
    if single {
        return Ok(());
    }
    Err(diag(format!(
        "the environment name `{environment}` is not a single path component, and a name \
         that becomes a file name beside the lock cannot carry a separator or climb out \
         of the project"
    )))
}

/// Where the environment's lock lives: in `project`, named for the
/// declaration that produced it: `pith.lock` for the default environment,
/// `<name>.pith.lock` beside it for a named one. One lock per resolution,
/// and an environment is one resolution (0043). The derivation is a pure
/// function the library owns, and it refuses a name that is not a single
/// path component.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming the name when it cannot name an
/// environment.
pub fn lock_path(project: &Path, environment: &str) -> PithResult<PathBuf> {
    component_name(environment)?;
    if environment == DEFAULT_ENVIRONMENT {
        Ok(project.join(DEFAULT_LOCK_FILE))
    } else {
        Ok(project.join(format!("{environment}{LOCK_SUFFIX}")))
    }
}

/// Where the environment's own record lives, beside its lock.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming the name when it cannot name an
/// environment.
pub fn record_path(project: &Path, environment: &str) -> PithResult<PathBuf> {
    component_name(environment)?;
    if environment == DEFAULT_ENVIRONMENT {
        Ok(project.join(DEFAULT_RECORD_FILE))
    } else {
        Ok(project.join(format!("{environment}{RECORD_SUFFIX}")))
    }
}
