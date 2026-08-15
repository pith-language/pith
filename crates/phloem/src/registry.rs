//! Filesystem-backed package registry adapter.
//!
//! Registry reads produce candidate universes, fetched archive bytes, and
//! transparency-log evidence. The sparse index stores one version per line;
//! package archives and log data live in adjacent directories. All reads are
//! caller-side effects.

use std::path::Path;

use pith_diag::PithResult;
use pith_ids::ContentId;

use crate::diag;
use crate::identity::{DomainIdentity, PackageIdentity};
use crate::lock::{LockEntry, Origin};
use crate::lockfile;
use crate::locktext::{parse_digest, parse_features, tokenize};
use crate::universe::{Candidate, CandidateUniverse};
use crate::witness::{self, Checkpoint, Inclusion};

const INDEX: &str = "index";
const PACKAGE: &str = "pkg";
const CHECKPOINT_FILE: &str = "checkpoint";
const LEAVES_FILE: &str = "leaves";

/// Reads a registry index as a candidate universe.
///
/// # Errors
/// Returns a diagnostic when the index cannot be read or parsed.
pub fn read_index(root: &Path, registry: &str) -> PithResult<CandidateUniverse> {
    let index = root.join(INDEX);
    let mut candidates = Vec::new();
    for domain in sorted_children(&index)? {
        for name in sorted_children(&index.join(&domain))? {
            let text = read(&index.join(&domain).join(&name))?;
            for line in text.lines() {
                let line = line.trim_end();
                if line.is_empty() {
                    continue;
                }
                candidates.push(index_candidate(&domain, &name, line, registry)?);
            }
        }
    }
    Ok(CandidateUniverse::new(candidates))
}

/// One index line as one candidate: `<version> <features> sha256:<digest>`,
/// where the digest is the registry's claim about the archive. the fetch
/// verifies it against bytes and the witness against the log.
fn index_candidate(domain: &str, name: &str, line: &str, registry: &str) -> PithResult<Candidate> {
    let tokens =
        tokenize(line).map_err(|message| diag(format!("`{name}` index line: {message}")))?;
    let [version, features, digest] = tokens.as_slice() else {
        return Err(diag(format!(
            "`{name}` index line: expected version, features, and a digest; found {} fields",
            tokens.len()
        )));
    };
    let features = parse_features(features, 0)?;
    let archive = parse_digest(digest, "the index archive digest", 0)?;
    Ok(Candidate {
        identity: PackageIdentity::declare(DomainIdentity::new(domain), name),
        version: version.as_str().into(),
        features,
        provenance: crate::source::SourceBinding::Archive { archive },
        origin: Origin::Registry(registry.into()),
        requires: Box::new([]),
    })
}

/// One fetched entry: the bytes as read, and the content identity measured
/// from them. Matching the measurement against the binding is
/// [`LockEntry::verify_resolution`], and matching it against the log is the
/// witness verification.
#[derive(Debug)]
pub struct Fetched {
    pub bytes: Vec<u8>,
    pub measured: ContentId,
}

/// Reads and measures the archive bound by a lock entry.
///
/// # Errors
/// Returns a diagnostic for an invalid coordinate or unreadable archive.
pub fn fetch(root: &Path, entry: &LockEntry) -> PithResult<Fetched> {
    let identity = entry.package.identity();
    let domain = component(identity.domain().as_str(), "domain")?;
    let name = component(identity.name(), "package name")?;
    let version = component(entry.package.version(), "version")?;
    let path = root
        .join(PACKAGE)
        .join(domain)
        .join(format!("{name}-{version}.tar"));
    let bytes = std::fs::read(&path)
        .map_err(|error| diag(format!("reading {} failed: {error}", path.display())))?;
    let measured = ContentId::of_blob(&bytes);
    Ok(Fetched { bytes, measured })
}

/// The evidence one entry's binding was witnessed: the log's line for the
/// entry's coordinates, the checkpoint that commits to the tree holding
/// it, and the inclusion proof for that line.
#[derive(Clone, Debug)]
pub struct Witnessed {
    /// The log's line, in the lock's own binding spelling.
    pub line: Box<str>,
    /// The content identity that line binds the coordinates to.
    pub witnessed: ContentId,
    pub checkpoint: Checkpoint,
    pub proof: Inclusion,
}

/// Reads transparency-log evidence for a lock entry.
///
/// # Errors
/// Returns a diagnostic when log data is unreadable, invalid, or missing the
/// entry.
pub fn read_witness(log: &Path, entry: &LockEntry) -> PithResult<Witnessed> {
    let checkpoint = Checkpoint::parse(&read(&log.join(CHECKPOINT_FILE))?)?;
    let leaves = read(&log.join(LEAVES_FILE))?;
    let mut records = Vec::new();
    let mut found: Option<(usize, ContentId, String)> = None;
    for line in leaves.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let binding = lockfile::parse_binding_line(line)?;
        if found.is_none() && same_coordinates(&binding, entry) {
            found = Some((records.len(), binding.source, line.into()));
        }
        records.push(line.as_bytes().to_vec());
    }
    let tree = witness::MerkleTree::new(records.iter().map(Vec::as_slice))?;
    let Some((index, witnessed, line)) = found else {
        return Err(diag(format!(
            "the log `{}` holds no line for `{}` in `{}` version {}",
            checkpoint.origin,
            entry.package.identity().name(),
            entry.package.identity().domain().as_str(),
            entry.package.version(),
        )));
    };
    let proof = tree.inclusion(index as u64)?;
    Ok(Witnessed {
        line: line.into(),
        witnessed,
        checkpoint,
        proof,
    })
}

/// Verifies an entry against a pinned checkpoint and inclusion evidence.
///
/// # Errors
/// Returns a diagnostic for the first mismatched checkpoint, proof, or
/// binding.
pub fn verify(entry: &LockEntry, evidence: &Witnessed, pinned: &Checkpoint) -> PithResult<()> {
    if evidence.checkpoint != *pinned {
        return Err(diag(format!(
            "the log served a checkpoint for `{}` of {} records with root `{}`, and this \
             configuration pins `{}` of {} records with root `{}`",
            evidence.checkpoint.origin,
            evidence.checkpoint.size,
            evidence.checkpoint.root,
            pinned.origin,
            pinned.size,
            pinned.root,
        )));
    }
    witness::verify_inclusion(evidence.line.as_bytes(), &evidence.proof, pinned)?;
    if evidence.witnessed != entry.source {
        return Err(diag(format!(
            "the log `{}` witnesses content `{}` for `{}` version {}, and the entry binds \
             `{}`: the coordinates resolve to two contents, and one of the two sides moved",
            evidence.checkpoint.origin,
            evidence.witnessed.digest(),
            entry.package.identity().name(),
            entry.package.version(),
            entry.source.digest(),
        )));
    }
    Ok(())
}

/// Whether a leaf line carries the entry's coordinates, ignoring the
/// digest: a leaf that matches the coordinates under another digest is the
/// disagreement the verification exists to report.
fn same_coordinates(binding: &crate::lock::Binding, entry: &LockEntry) -> bool {
    binding.package == entry.package && binding.features == entry.features
}

/// A name that becomes a path component under `root`, refused when it is
/// not one, so a registry's naming cannot escape the registry.
fn component<'a>(name: &'a str, what: &str) -> PithResult<&'a str> {
    let mut components = Path::new(name).components();
    if matches!(
        (components.next(), components.next()),
        (Some(std::path::Component::Normal(_)), None)
    ) {
        return Ok(name);
    }
    Err(diag(format!(
        "the {what} `{name}` is not a single path component, and a registry's naming cannot \
         name a path outside the registry"
    )))
}

fn read(path: &Path) -> PithResult<String> {
    std::fs::read_to_string(path)
        .map_err(|error| diag(format!("reading {} failed: {error}", path.display())))
}

/// A directory's children in one canonical order, because a read whose
/// output depended on the directory's iteration order would be a universe
/// whose digest depended on the filesystem.
fn sorted_children(path: &Path) -> PithResult<Vec<String>> {
    let entries = std::fs::read_dir(path)
        .map_err(|error| diag(format!("reading {} failed: {error}", path.display())))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            diag(format!(
                "reading an entry in {} failed: {error}",
                path.display()
            ))
        })?;
        names.push(entry.file_name().to_string_lossy().into());
    }
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::PackageVersion;

    #[test]
    fn a_name_that_is_not_one_path_component_is_refused_at_the_fetch() {
        let entry = LockEntry::new(
            PackageVersion::new(
                PackageIdentity::declare(DomainIdentity::new("../escape"), ".."),
                "1.0",
            ),
            [] as [&str; 0],
            ContentId::of_blob(b"pkg"),
            Origin::Registry("pkgs.pith-lang.org".into()),
        );
        let error = fetch(Path::new("/registry"), &entry).unwrap_err();
        let message = error
            .iter()
            .next()
            .map(|diagnostic| diagnostic.message.0.to_string())
            .unwrap_or_default();
        assert!(
            message.contains("path component") && message.contains(".."),
            "the diagnostic names the refused coordinate: {message}"
        );
    }

    #[test]
    fn an_index_line_with_the_wrong_shape_is_refused_naming_the_line() {
        let error = index_candidate("pithpkgs", "zlib", "1.3 only-two-fields", "r").unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("zlib") && d.message.0.contains("fields")),
            "the diagnostic names the package and the shape: {error:?}"
        );
    }
}
