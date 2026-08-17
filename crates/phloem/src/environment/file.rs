use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use pith_core::Value;
use pith_diag::PithResult;

use crate::codec::FIELD_TOOLCHAIN;
use crate::diag;
use crate::lock::text::{BLAKE3, token};

use super::{DEFAULT_ENVIRONMENT, EnvironmentDocument};

const DEFAULT_LOCK_FILE: &str = "pith.lock";
const DEFAULT_RECORD_FILE: &str = "pith.env";
const LOCK_SUFFIX: &str = ".pith.lock";
const RECORD_SUFFIX: &str = ".pith.env";
const LOCK: &str = "lock";
const PLATFORM: &str = "platform";
const ENV: &str = "env";
const SUBSTITUTE: &str = "substitute";

/// Formats a toolchain value as one text token.
fn toolchain_token(toolchain: &Value) -> String {
    let driver = match toolchain {
        Value::Nominal { representation, .. } => match representation.as_ref() {
            Value::Text(driver) => return token(driver),
            other => other.describe(),
        },
        other => other.describe(),
    };
    token(&driver)
}

/// Renders an environment document as deterministic line-oriented text.
#[must_use]
pub fn render(document: &EnvironmentDocument) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{ENV} {} {PLATFORM} {}/{} {FIELD_TOOLCHAIN} {} {LOCK} {BLAKE3}{}",
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
                "{SUBSTITUTE} {} {} {} [{}] {BLAKE3}{} {}",
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

/// Validates that an environment name is one path component.
///
/// # Errors
/// Returns a diagnostic when the name is empty or contains multiple path
/// components.
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

/// Derives the lock path for an environment under `project`.
///
/// # Errors
/// Returns a diagnostic when `environment` is not one path component.
pub fn lock_path(project: &Path, environment: &str) -> PithResult<PathBuf> {
    component_name(environment)?;
    if environment == DEFAULT_ENVIRONMENT {
        Ok(project.join(DEFAULT_LOCK_FILE))
    } else {
        Ok(project.join(format!("{environment}{LOCK_SUFFIX}")))
    }
}

/// Derives the environment record path under `project`.
///
/// # Errors
/// Returns a diagnostic when `environment` is not one path component.
pub fn record_path(project: &Path, environment: &str) -> PithResult<PathBuf> {
    component_name(environment)?;
    if environment == DEFAULT_ENVIRONMENT {
        Ok(project.join(DEFAULT_RECORD_FILE))
    } else {
        Ok(project.join(format!("{environment}{RECORD_SUFFIX}")))
    }
}
