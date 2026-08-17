//! Filesystem-backed package registry adapter.
//!
//! Registry reads produce candidate universes, fetched archive bytes, and
//! transparency-log evidence. The sparse index stores one version per line;
//! package archives and log data live in adjacent directories. All reads are
//! caller-side effects, and every parse refusal carries a span into the file
//! read and the file itself as its source.

use std::path::Path;
use std::sync::Arc;

use pith_diag::{PithResult, SourceFile, SourceId};
use pith_ids::ContentId;

use crate::identity::{DomainIdentity, PackageIdentity};
use crate::lock::text::{
    Token, features_token, parse_digest, parse_features, parse_range, range_token, tokenize,
};
use crate::lock::{LockEntry, Origin, parse_binding_line};
use crate::text_diag;
use crate::universe::{Candidate, CandidateUniverse, Requirement};
use crate::witness::{self, Checkpoint, Inclusion};

const INDEX: &str = "index";
const PACKAGE: &str = "pkg";
const CHECKPOINT_FILE: &str = "checkpoint";
const LEAVES_FILE: &str = "leaves";

/// Reads a registry index as a candidate universe. Each index file becomes
/// the source of the diagnostics its lines produce, named by its path.
///
/// # Errors
/// Returns a diagnostic when the index cannot be read or parsed.
pub fn read_index(root: &Path, registry: &str) -> PithResult<CandidateUniverse> {
    let index = root.join(INDEX);
    let mut candidates = Vec::new();
    let mut next_id = 0u32;
    for domain in sorted_children(&index)? {
        for name in sorted_children(&index.join(&domain))? {
            let path = index.join(&domain).join(&name);
            let text = read(&path)?;
            let file = Arc::new(SourceFile::new(
                SourceId::from_raw(next_id),
                path.display().to_string(),
                text,
            ));
            next_id = next_id.saturating_add(1);
            for line in file.lines() {
                if line.text.is_empty() {
                    continue;
                }
                candidates.push(index_candidate(&file, &domain, &name, &line, registry)?);
            }
        }
    }
    Ok(CandidateUniverse::new(candidates))
}

/// One index line from the fields a line carries — the spelling
/// [`read_index`] parses, the same one-spelling arrangement the lock's
/// binding line has, so a publisher and a reader cannot drift into two
/// formats. The domain and package name are the file's path on the read
/// side and never appear in the line, and the provenance is the archive the
/// digest claims, so a candidate of another binding shape has no line to
/// write. Requirements render in the order given.
#[must_use]
pub fn index_line(
    version: &str,
    features: &[Box<str>],
    archive: ContentId,
    requires: &[Requirement],
) -> String {
    let mut line = format!(
        "{} {} {}{}",
        version,
        features_token(features),
        crate::lock::text::BLAKE3,
        archive.digest(),
    );
    for requirement in requires {
        line.push_str(&format!(
            " requires {}/{} {}",
            requirement.subject.domain().as_str(),
            requirement.subject.name(),
            range_token(&requirement.range),
        ));
        if !requirement.features.is_empty() {
            line.push_str(&format!(" {}", features_token(&requirement.features)));
        }
    }
    line
}

/// One index line as one candidate: `<version> <features> blake3:<digest>`
/// followed by zero or more `requires <domain>/<name> <range> [<features>]`
/// clauses, where the digest is the registry's claim about the archive and
/// each clause a version's claim about another package. The fetch verifies
/// the digest against bytes and the witness against the log; the solver
/// turns the requirements into constraints, so resolution reads what the
/// index says and fetches nothing.
fn index_candidate(
    source: &Arc<SourceFile>,
    domain: &str,
    name: &str,
    line: &pith_diag::FileLine,
    registry: &str,
) -> PithResult<Candidate> {
    let tokens = tokenize(line.text, line.span.start)
        .map_err(|refusal| text_diag(source, refusal.span, refusal.message))?;
    let [version, features, digest, rest @ ..] = tokens.as_slice() else {
        return Err(text_diag(
            source,
            line.span,
            format!(
                "`{name}` index line: expected version, features, and a digest; found {} fields",
                tokens.len()
            ),
        ));
    };
    let features = parse_features(features)
        .map_err(|refusal| text_diag(source, refusal.span, refusal.message))?;
    let archive = parse_digest(digest, "the index archive digest")
        .map_err(|refusal| text_diag(source, refusal.span, refusal.message))?;
    let requires = parse_requires(source, name, rest, line.span)?;
    Ok(Candidate {
        identity: PackageIdentity::declare(DomainIdentity::new(domain), name),
        version: version.text.as_str().into(),
        features,
        provenance: crate::source::SourceBinding::Archive { archive },
        origin: Origin::Registry(registry.into()),
        requires,
    })
}

/// The `requires <domain>/<name> <range> [<features>]` clauses that follow an
/// index line's digest, as requirements. The clause's features are optional
/// and last; each next clause begins with its own `requires`.
fn parse_requires(
    source: &Arc<SourceFile>,
    name: &str,
    rest: &[Token],
    line_span: pith_diag::Span,
) -> PithResult<Box<[Requirement]>> {
    let mut clauses = Vec::new();
    let mut tokens = rest;
    while !tokens.is_empty() {
        let [keyword, subject, range, tail @ ..] = tokens else {
            return Err(text_diag(
                source,
                line_span,
                format!(
                    "`{name}` index line: a requirement is `requires <domain>/<name> <range> \
                     [<features>]`"
                ),
            ));
        };
        if keyword.text != "requires" {
            return Err(text_diag(
                source,
                keyword.span,
                format!(
                    "`{name}` index line: `{}` follows the digest, and only a requirement clause \
                     may; spell it `requires`",
                    keyword.text
                ),
            ));
        }
        let Some((subject_domain, subject_name)) = subject.text.split_once('/') else {
            return Err(text_diag(
                source,
                subject.span,
                format!(
                    "`{name}` index line: the requirement names `{}`, which is not \
                     `<domain>/<name>`",
                    subject.text
                ),
            ));
        };
        if subject_domain.is_empty() || subject_name.is_empty() {
            return Err(text_diag(
                source,
                subject.span,
                format!(
                    "`{name}` index line: the requirement names `{}`, whose domain or package \
                     side is empty",
                    subject.text
                ),
            ));
        }
        let parsed_range = parse_range(range)
            .map_err(|refusal| text_diag(source, refusal.span, refusal.message))?;
        let no_features: Box<[Box<str>]> = Box::new([]);
        let bracketed = tail
            .first()
            .filter(|token| token.text.starts_with('['))
            .map(|token| {
                parse_features(token)
                    .map_err(|refusal| text_diag(source, refusal.span, refusal.message))
            });
        let (features, consumed) = match bracketed {
            Some(parsed) => (parsed?, 4),
            None => (no_features, 3),
        };
        clauses.push(Requirement {
            subject: PackageIdentity::declare(DomainIdentity::new(subject_domain), subject_name),
            range: parsed_range,
            features,
        });
        tokens = tokens.get(consumed..).unwrap_or(&[]);
    }
    Ok(clauses.into())
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
        .map_err(|error| crate::diag(format!("reading {} failed: {error}", path.display())))?;
    let measured = ContentId::of_blob(&bytes);
    Ok(Fetched { bytes, measured })
}

/// The evidence one entry's binding was witnessed: the log's line for the
/// entry, the checkpoint that commits to the tree holding it, and the
/// inclusion proof for that line.
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
    let leaves_path = log.join(LEAVES_FILE);
    let leaves = read(&leaves_path)?;
    let file = Arc::new(SourceFile::new(
        SourceId::from_raw(0),
        leaves_path.display().to_string(),
        leaves,
    ));
    let mut records = Vec::new();
    let mut found: Option<(usize, ContentId, String)> = None;
    for line in file.lines() {
        if line.text.is_empty() {
            continue;
        }
        let binding = parse_binding_line(&file, &line)?;
        if found.is_none() && same_coordinates(&binding, entry) {
            found = Some((records.len(), binding.source, line.text.into()));
        }
        records.push(line.text.as_bytes().to_vec());
    }
    let tree = witness::MerkleTree::new(records.iter().map(Vec::as_slice))?;
    let Some((index, witnessed, line)) = found else {
        return Err(crate::diag(format!(
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
        return Err(crate::diag(format!(
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
        return Err(crate::diag(format!(
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
    Err(crate::diag(format!(
        "the {what} `{name}` is not a single path component, and a registry's naming cannot \
         name a path outside the registry"
    )))
}

fn read(path: &Path) -> PithResult<String> {
    std::fs::read_to_string(path)
        .map_err(|error| crate::diag(format!("reading {} failed: {error}", path.display())))
}

/// A directory's children in one canonical order, because a read whose
/// output depended on the directory's iteration order would be a universe
/// whose digest depended on the filesystem.
fn sorted_children(path: &Path) -> PithResult<Vec<String>> {
    let entries = std::fs::read_dir(path)
        .map_err(|error| crate::diag(format!("reading {} failed: {error}", path.display())))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            crate::diag(format!(
                "reading an entry in {} failed: {error}",
                path.display()
            ))
        })?;
        let name = entry.file_name().into_string().map_err(|_| {
            crate::diag(format!(
                "an entry name in {} is not valid UTF-8",
                path.display()
            ))
        })?;
        names.push(name);
    }
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::PackageVersion;
    use crate::lock::binding_line;

    #[test]
    fn directory_children_are_utf8_and_sorted() {
        let directory = tempfile::TempDir::new().unwrap();
        std::fs::write(directory.path().join("z-last"), b"").unwrap();
        std::fs::write(directory.path().join("a-first"), b"").unwrap();

        assert_eq!(
            sorted_children(directory.path()).unwrap(),
            ["a-first".to_string(), "z-last".to_string()]
        );
    }

    #[cfg(all(unix, not(target_vendor = "apple")))]
    #[test]
    fn a_non_utf8_directory_child_is_refused() {
        use std::os::unix::ffi::OsStringExt as _;

        let directory = tempfile::TempDir::new().unwrap();
        let name = std::ffi::OsString::from_vec(vec![0xff]);
        std::fs::write(directory.path().join(name), b"").unwrap();

        let error = sorted_children(directory.path()).unwrap_err();
        assert!(
            error
                .iter()
                .any(|diagnostic| diagnostic.message.0.contains("not valid UTF-8"))
        );
    }

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

    fn index_file(root: &Path, domain: &str, name: &str, text: &str) -> SourceFile {
        let path = root.join(INDEX).join(domain).join(name);
        SourceFile::new(SourceId::from_raw(0), path.display().to_string(), text)
    }

    fn select(source: &str, span: pith_diag::Span) -> Option<&str> {
        source.get(span.start.0 as usize..span.end.0 as usize)
    }

    #[test]
    fn an_index_line_with_the_wrong_shape_is_refused_naming_the_line() {
        let text = "1.3 only-two-fields";
        let file = index_file(Path::new("/registry"), "pithpkgs", "zlib", text);
        let source = Arc::new(file);
        let error = index_candidate(
            &source,
            "pithpkgs",
            "zlib",
            &source.lines().next().unwrap(),
            "r",
        )
        .unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("zlib") && d.message.0.contains("fields")),
            "the diagnostic names the package and the shape: {error:?}"
        );
        assert!(
            error.iter().any(|d| select(text, d.span) == Some(text)),
            "the span selects the whole index line: {error:?}"
        );
    }

    #[test]
    fn a_malformed_requirement_clause_is_refused_at_its_own_field() {
        let text = "1.3 [] blake3:0000000000000000000000000000000000000000000000000000000000000000 requires pithpkgs/util 1.0";
        let file = index_file(Path::new("/registry"), "pithpkgs", "zlib", text);
        let source = Arc::new(file);
        let error = index_candidate(
            &source,
            "pithpkgs",
            "zlib",
            &source.lines().next().unwrap(),
            "r",
        )
        .unwrap_err();
        let diagnostic = error.iter().next().unwrap();
        assert!(
            diagnostic.message.0.contains("range"),
            "the diagnostic names the clause: {error:?}"
        );
        assert!(
            diagnostic
                .source
                .as_ref()
                .is_some_and(|source| source.label.contains("zlib")),
            "the attached source names the package's index file, which the message no longer spells out: {error:?}"
        );
        assert!(
            select(text, diagnostic.span) == Some("1.0"),
            "the span selects the malformed range field, not the line: {error:?}"
        );
    }

    #[test]
    fn a_well_formed_index_line_parses_with_its_requirements() {
        let digest = "0000000000000000000000000000000000000000000000000000000000000000";
        let text = format!("1.3 [shared] blake3:{digest} requires pithpkgs/util >=1.0 [fast]");
        let file = index_file(Path::new("/registry"), "pithpkgs", "zlib", &text);
        let source = Arc::new(file);
        let candidate = index_candidate(
            &source,
            "pithpkgs",
            "zlib",
            &source.lines().next().unwrap(),
            "r",
        )
        .unwrap();
        assert_eq!(candidate.version.as_ref(), "1.3");
        assert_eq!(candidate.features.len(), 1);
        assert_eq!(candidate.requires.len(), 1);
        assert_eq!(candidate.requires.first().unwrap().subject.name(), "util");
    }

    #[test]
    fn a_log_leaf_refusal_carries_the_leaves_file_and_its_line() {
        let entry = LockEntry::new(
            PackageVersion::new(
                PackageIdentity::declare(DomainIdentity::new("pithpkgs"), "zlib"),
                "1.3",
            ),
            [] as [&str; 0],
            ContentId::of_blob(b"zlib"),
            Origin::Registry("pkgs.pith-lang.org".into()),
        );
        let good = binding_line(&entry);
        let text = format!("{good}\nbind broken\n");
        let file = SourceFile::new(SourceId::from_raw(0), "log/leaves", text.as_str());
        let source = Arc::new(file);
        let error = parse_binding_line(&source, &source.lines().nth(1).unwrap());
        assert!(error.is_err(), "a broken leaf line is refused: {error:?}");
        let error = error.unwrap_err();
        let diagnostic = error.iter().next().unwrap();
        assert_eq!(
            select(&text, diagnostic.span),
            Some("bind broken"),
            "the span selects the leaf line"
        );
    }
}
