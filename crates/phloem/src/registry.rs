//! The first source adapter: a registry read as a caller-side effect
//! (decision 0044).
//!
//! The adapter's reads are the discovery a pure evaluation must not do
//! (0007), so every function here is a caller-side effect in the position
//! 0041 put the lock's write: reading the index produces the candidate
//! universe as a declared input, fetching an entry's archive produces
//! bytes and their measured identity, reading the log produces the
//! evidence a binding's verification consumes. The engine never learns
//! that sources exist, and the reconciliation with 0040 is direct: the
//! query is a separate step whose result becomes the declared input, and a
//! registry whose answer moved between two runs produces a different
//! universe digest, which the lock's diff names like any other moved
//! input.
//!
//! The on-disk shape is the sparse-index arrangement crates.io fixed,
//! miniaturized to a directory: `index/<domain>/<name>` holds one line per
//! version (`<version> <features> sha256:<digest>`), and
//! `pkg/<domain>/<name>-<version>.tar` holds the bytes. Beside it a log
//! directory holds `checkpoint` and `leaves`; the leaves are binding lines
//! in the lock's own spelling, so the line the log witnesses and the line
//! the file writes are one line. The digests in the index are the
//! registry's claims, verified against fetched bytes and against the log;
//! the whole claim structure is decided in 0044, not here.

use std::path::Path;

use pith_diag::PithResult;
use pith_ids::ContentId;

use crate::diag;
use crate::identity::{DomainIdentity, PackageIdentity};
use crate::lock::{LockEntry, Origin};
use crate::locktext::{parse_digest, parse_features, tokenize};
use crate::universe::{Candidate, CandidateUniverse};
use crate::witness::{self, Checkpoint, Inclusion};

const INDEX: &str = "index";
const PACKAGE: &str = "pkg";
const CHECKPOINT_FILE: &str = "checkpoint";
const LEAVES_FILE: &str = "leaves";

/// Read a registry's index as a candidate universe, recording `registry`
/// as the origin every candidate was read from.
///
/// A caller-side effect: this reads the index directory and nothing else,
/// and the universe it returns is the declared input a resolution runs
/// against. Names come from the index layout itself, so a candidate's
/// coordinates are the registry's own naming, in one canonical order.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming the path and the failure when the
/// index cannot be read, and the line when one does not parse.
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
/// where the digest is the registry's claim about the archive; the fetch
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

#[derive(Clone, Debug)]
pub struct FetchedPlaceholder;
/// from them. The measurement is the fact; whether it matches what the
/// entry binds is [`LockEntry::verify_resolution`]'s question, and whether
/// it matches what the log witnessed is the witness verification's.
#[derive(Debug)]
pub struct Fetched {
    pub bytes: Vec<u8>,
    pub measured: ContentId,
}

/// Fetch the archive an entry binds, reading the bytes and measuring them.
///
/// A caller-side effect: this reads one package file and nothing else. The
/// coordinates become path components, so each must be a single component.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming the path and the failure when the
/// archive cannot be read, and the coordinate that cannot become a path
/// component.
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

/// Read the log's evidence for one entry's coordinates.
///
/// A caller-side effect over the log directory: the checkpoint, the leaf
/// lines, and the proof derived from them. In the deployed arrangement the
/// proof arrives from the log's server; here it is derived from the same
/// leaves the checkpoint commits to.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming the entry and the log when the
/// checkpoint or leaves cannot be read or parsed, and naming the
/// coordinates when the log holds no line for them.
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
        if found.is_none() && same_coordinates(line, entry)? {
            found = Some((records.len(), line_digest(line)?, line.into()));
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

/// Verify one entry's binding against the evidence the log served and the
/// checkpoint a person's configuration pinned: the served checkpoint is
/// the pinned one, the inclusion proof carries the line the log served —
/// the line it holds for these coordinates — into the tree the checkpoint
/// commits to, and the digest that line witnesses is the digest the entry
/// binds. The first leg is the policy leg — nothing vouches for the pinned
/// checkpoint but the configuration naming it (0044) — and the other two
/// are facts, in the order a lookup answers them: first that the log
/// really committed to the line, then that the line agrees with the
/// binding.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming what was expected and what was
/// found for the first leg that disagreed.
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
fn same_coordinates(line: &str, entry: &LockEntry) -> PithResult<bool> {
    let tokens = tokenize(line).map_err(|message| diag(format!("a log leaf line: {message}")))?;
    let [_, domain, name, version, features, _digest] = tokens.as_slice() else {
        return Err(diag(format!(
            "a log leaf line carries domain, name, version, features, and a digest; found \
             {} tokens",
            tokens.len()
        )));
    };
    let mut features = parse_features(features, 0)?;
    features.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    Ok(domain == entry.package.identity().domain().as_str()
        && name == entry.package.identity().name()
        && version == entry.package.version()
        && features == entry.features)
}

fn line_digest(line: &str) -> PithResult<ContentId> {
    let tokens = tokenize(line).map_err(|message| diag(format!("a log leaf line: {message}")))?;
    let [.., digest] = tokens.as_slice() else {
        return Err(diag("a log leaf line ends in a digest".to_string()));
    };
    parse_digest(digest, "the witnessed digest", 0)
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
    let mut names: Vec<String> = std::fs::read_dir(path)
        .map_err(|error| diag(format!("reading {} failed: {error}", path.display())))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into())
        .collect();
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
