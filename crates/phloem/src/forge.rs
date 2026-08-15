//! Git source discovery and materialization.
//!
//! The adapter resolves a reference to a revision and tree, then archives
//! that tree for content measurement. Git commands run at the caller's effect
//! boundary and never during engine rule evaluation.

use std::path::Path;
use std::process::Command;

use pith_diag::{DiagnosticSink, PithResult};
use pith_ids::ContentId;

use crate::diag;
use crate::identity::PackageIdentity;
use crate::lock::Origin;
use crate::resolution::Resolution;
use crate::source::SourceBinding;
use crate::universe::Candidate;

/// Why a git invocation could not answer: the host has no git, or the
/// invocation ran and git refused. The distinction is the skip-versus-fail
/// line the toolchain fixtures drew: an absent tool skips, a present tool
/// that cannot resolve the repository fails.
#[derive(Debug)]
enum ForgeError {
    NotFound,
    Failed(String),
}

impl From<ForgeError> for pith_diag::DiagnosticSink {
    fn from(error: ForgeError) -> Self {
        match error {
            ForgeError::NotFound => diag("no `git` on this host; a forge source needs one"),
            ForgeError::Failed(message) => diag(message),
        }
    }
}

fn git(repo: &Path, arguments: &[&str]) -> Result<String, ForgeError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(arguments)
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => ForgeError::NotFound,
            _ => ForgeError::Failed(format!("running git failed: {error}")),
        })?;
    if !output.status.success() {
        return Err(ForgeError::Failed(format!(
            "git {} answered {}: {}",
            arguments.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim_end()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}

/// Resolves a Git reference to a package candidate.
///
/// # Errors
/// Returns a diagnostic when Git is unavailable or the reference does not
/// resolve.
pub fn reference_candidate(
    identity: &PackageIdentity,
    version: &str,
    repo: &Path,
    reference: &str,
    forge: &str,
) -> PithResult<Candidate> {
    let revision = git(repo, &["rev-parse", reference]).map_err(DiagnosticSink::from)?;
    let tree = git(repo, &["rev-parse", &format!("{reference}^{{tree}}")])
        .map_err(DiagnosticSink::from)?;
    Ok(Candidate {
        identity: identity.clone(),
        version: version.into(),
        features: Box::new([]),
        provenance: SourceBinding::Git {
            revision: revision.into(),
            tree: tree.into(),
        },
        origin: Origin::Forge(forge.into()),
        requires: Box::new([]),
    })
}

/// Materializes each Git reference selected by a solved resolution.
///
/// # Errors
/// Returns a diagnostic when a selected revision cannot be archived.
pub fn materialize_resolution(repo: &Path, resolution: &Resolution) -> PithResult<Resolution> {
    let Resolution::Solved {
        choice,
        trail,
        universe,
    } = resolution
    else {
        return Err(diag(
            "only a solved resolution has a choice to materialize; the other constructors \
             selected nothing to fetch",
        ));
    };
    let mut materialized = Vec::with_capacity(choice.len());
    for candidate in choice.iter() {
        let provenance = match &candidate.provenance {
            SourceBinding::Git { revision, tree } => SourceBinding::GitTree {
                revision: revision.clone(),
                tree: tree.clone(),
                content: materialize(repo, revision)?,
            },
            other => other.clone(),
        };
        materialized.push(Candidate {
            provenance,
            ..candidate.clone()
        });
    }
    Ok(Resolution::Solved {
        choice: materialized.into(),
        trail: trail.clone(),
        universe: *universe,
    })
}

/// Archives one Git revision and measures the resulting bytes.
///
/// # Errors
/// Returns a diagnostic when the revision cannot be archived.
pub fn materialize(repo: &Path, revision: &str) -> PithResult<ContentId> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["archive", "--format=tar", revision])
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => diag("no `git` on this host; a forge source needs one"),
            _ => diag(format!("archiving {revision} failed: {error}")),
        })?;
    if !output.status.success() {
        return Err(diag(format!(
            "archiving {revision} failed: {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim_end()
        )));
    }
    Ok(ContentId::of_blob(&output.stdout))
}
