//! The forge adapter: a git reference resolved and materialized as
//! caller-side effects (decision 0044).
//!
//! Git is the one source whose witness is intrinsic. An object's name is
//! the hash of its exact bytes, a tree's hash covers everything beneath
//! it, and a commit's hash covers its tree and parents, so a revision
//! already authenticates the content it names, and a fetch verifies every
//! object it receives against its own hash. The ref is the part git does
//! not authenticate: a branch is a mutable pointer no commit records, so
//! the candidate carries the concrete revision and tree hash the ref
//! resolved to, and nothing downstream re-reads the ref. That payload is
//! the shape a lock refuses to bind, and the fetch materializes the tree
//! into bytes pith measures.
//!
//! Running `git` is a caller-side effect on the same ground as reading a
//! registry directory: it happens before any request exists, and what it
//! produces, the candidate and then the measured archive, is a declared
//! input. The engine never runs here.

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
pub enum ForgeError {
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

/// One candidate for one package at one git reference: the revision the
/// reference resolved to and the tree hash it names, with `forge` recorded
/// as where the reference was read. The candidate carries a reference, so
/// a lock refuses to bind it until [`materialize_resolution`] has read the
/// tree. resolution chooses among references, and only the choice is
/// fetched.
///
/// A caller-side effect: this resolves the reference by running git.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] when git is absent or the reference
/// does not resolve.
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

/// Materialize every git reference a solved resolution chose: archive each
/// revision's tree to bytes and measure them, rewriting each chosen git
/// candidate into the content its tree measured to, which is what a lock
/// binds.
///
/// A caller-side effect: this runs git once per chosen reference and
/// reads its output. The choice, the trail, and the universe digest are
/// unchanged, so the materialized answer locks where the unmaterialized
/// one refuses, and on nothing else.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming the candidate when a reference
/// cannot be archived, propagated from git.
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

/// The archive of one revision's tree, as bytes, measured. `git archive`
/// output is a function of the tree, because timestamps clamp to the
/// commit's, so the measured archive and the tree hash name one content.
///
/// A caller-side effect: this runs git.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming the revision when the archive
/// cannot be produced.
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
