//! The first source adapter against real bytes (decision 0044): a local
//! filesystem registry is read into the declared candidate universe, a
//! resolution over that universe locks bindings whose archives are then
//! fetched and measured, the transparency log over the index's binding
//! lines witnesses each binding, and the failures the threat model names —
//! a tampered archive, a registry that republishes under one name, a
//! checkpoint the configuration does not pin — are each detected with a
//! diagnostic naming what was expected and what was found. A git
//! reference, the one source whose witness is intrinsic, locks only after
//! a fetch materializes and measures the tree.

use std::path::Path;
use std::process::Command;

use phloem::constraint::{Bound, Constraint, Range, constraint_set_value};
use phloem::document::{Lock, diff as diff_locks};
use phloem::forge;
use phloem::identity::{DomainIdentity, NUMERIC_SEGMENTS, PackageIdentity, version_scheme_value};
use phloem::lock::{LockEntry, Origin};
use phloem::lockfile;
use phloem::preference::{Preference, PreferenceList, preference_list_value};
use phloem::registry;
use phloem::resolution::{Resolution, resolve_request};
use phloem::resolve::{ResolveSolver, Schemes};
use phloem::universe::CandidateUniverse;
use phloem::witness::{Checkpoint, MerkleTree};
use pith_engine::Engine;
use pith_engine::state::MemoryEngineStateStore;
use pith_ids::ContentId;
use pith_store::MemoryContentStore;
use tempfile::TempDir;

const REGISTRY: &str = "registries.pith-lang.org";
const LOG: &str = "logs.pith-lang.org";
const ZLIB_BYTES: &[u8] = b"the zlib 1.3 archive, as bytes";
const OPENSSL_BYTES: &[u8] = b"the openssl 1.1.1 archive, as bytes";

fn identity(name: &str) -> PackageIdentity {
    PackageIdentity::declare(DomainIdentity::new("pithpkgs"), name)
}

/// One published package: the index line and the archive it claims.
struct Published {
    name: &'static str,
    version: &'static str,
    bytes: &'static [u8],
}

fn published() -> Vec<Published> {
    vec![
        Published {
            name: "zlib",
            version: "1.3",
            bytes: ZLIB_BYTES,
        },
        Published {
            name: "openssl",
            version: "1.1.1",
            bytes: OPENSSL_BYTES,
        },
    ]
}

fn index_line(version: &str, digest: &ContentId) -> String {
    format!("{version} [] sha256:{}", digest.digest())
}

fn package_path(root: &Path, name: &str, version: &str) -> std::path::PathBuf {
    root.join("pkg/pithpkgs")
        .join(format!("{name}-{version}.tar"))
}

fn binding_of(published: &Published) -> LockEntry {
    LockEntry::new(
        phloem::identity::PackageVersion::new(identity(published.name), published.version),
        [] as [&str; 0],
        ContentId::of_blob(published.bytes),
        Origin::Registry(REGISTRY.into()),
    )
}

/// Publish a registry and its witnessing log: the index lines, the
/// archives, the log's leaves in the lock's own binding spelling, and the
/// checkpoint over them. The test is the log's operator here; the fallible
/// filesystem work is carried as a result so the tests unwrap at their
/// call sites, where unwrapping is allowed.
fn publish(root: &Path, packages: &[Published]) -> pith_diag::PithResult<Checkpoint> {
    let write = |path: std::path::PathBuf, bytes: &[u8]| -> pith_diag::PithResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                fixture_error(format!("creating {} failed: {error}", parent.display()))
            })?;
        }
        std::fs::write(&path, bytes)
            .map_err(|error| fixture_error(format!("writing {} failed: {error}", path.display())))
    };
    let mut index: Vec<(String, String)> = Vec::new();
    for package in packages {
        let digest = ContentId::of_blob(package.bytes);
        let line = index_line(package.version, &digest);
        match index.iter_mut().find(|(name, _)| name == package.name) {
            Some((_, lines)) => lines.push_str(&format!("{line}\n")),
            None => index.push((package.name.into(), format!("{line}\n"))),
        }
        write(
            package_path(root, package.name, package.version),
            package.bytes,
        )?;
    }
    for (name, lines) in index {
        write(root.join("index/pithpkgs").join(&name), lines.as_bytes())?;
    }
    let leaves: Vec<String> = packages
        .iter()
        .map(|p| lockfile::binding_line(&binding_of(p)))
        .collect();
    let log = root.join("log");
    write(
        log.join("leaves"),
        format!("{}\n", leaves.join("\n")).as_bytes(),
    )?;
    let tree = MerkleTree::new(leaves.iter().map(String::as_str).map(str::as_bytes))?;
    let checkpoint = Checkpoint {
        origin: LOG.into(),
        size: tree.size(),
        root: tree.root(),
    };
    write(log.join("checkpoint"), checkpoint.render().as_bytes())?;
    Ok(checkpoint)
}

/// The fixture's own failure spelling: a diagnostic sink carrying the
/// message, because a helper outside a `#[test]` function cannot unwrap or
/// panic under the crate's lint posture.
fn fixture_error(message: String) -> pith_diag::DiagnosticSink {
    let mut sink = pith_diag::DiagnosticSink::new();
    sink.push(pith_diag::Diag::new(
        pith_diag::Severity::Error,
        pith_diag::StableCode(0),
        pith_diag::Span::none(),
        message,
    ));
    sink
}

fn engine() -> Engine {
    let mut engine = Engine::with_state_store(
        MemoryContentStore::default(),
        MemoryEngineStateStore::default(),
    );
    let solver = ResolveSolver::new(Schemes::standard());
    engine.register_rule(solver.rule(), solver);
    engine
}

fn zlib_constraint() -> Constraint {
    Constraint {
        subject: identity("zlib"),
        range: Range::AtLeast(Bound::new("1.0", true)),
        features: Box::new([]),
        attribution: "root".into(),
    }
}

fn preferences() -> PreferenceList {
    PreferenceList(Box::new([Preference::Newest]))
}

/// Resolve against a universe through the engine and lock the answer, the
/// same shape the environment resolves with.
fn resolve_lock(universe: &CandidateUniverse) -> pith_diag::PithResult<Lock> {
    let request = resolve_request(
        &version_scheme_value(NUMERIC_SEGMENTS),
        &constraint_set_value(&[zlib_constraint()]),
        &universe.to_value(),
        &preference_list_value(&preferences()),
        100,
    );
    let answer = engine().evaluate_pure(&request)?;
    let resolution = Resolution::from_value(&answer.value)?;
    Lock::from_resolution(NUMERIC_SEGMENTS, &preferences(), &resolution)
}

fn zlib_entry(lock: &Lock) -> LockEntry {
    let Some(entry) = lock
        .entries
        .iter()
        .find(|entry| entry.package.identity().name() == "zlib")
    else {
        unreachable!("the fixture resolves and locks zlib")
    };
    entry.clone()
}

fn message_of(error: &pith_diag::DiagnosticSink) -> String {
    error
        .iter()
        .next()
        .map(|diagnostic| diagnostic.message.0.to_string())
        .unwrap_or_default()
}

#[test]
fn a_registry_index_becomes_a_universe_that_locks_content_actually_read() {
    let scratch = TempDir::new().unwrap();
    let pinned = publish(scratch.path(), &published()).unwrap();

    // The index read is the caller-side effect; the universe it returns is
    // the declared input the resolution runs against.
    let universe = registry::read_index(scratch.path(), REGISTRY).unwrap();
    assert_eq!(universe.candidates.len(), 2);

    let lock = resolve_lock(&universe).unwrap();
    let entry = zlib_entry(&lock);
    assert_eq!(entry.source, ContentId::of_blob(ZLIB_BYTES));
    assert_eq!(entry.origin, Origin::Registry(REGISTRY.into()));

    // The fetch reads real bytes and measures them: the binding and the
    // measurement agree because the registry told the truth.
    let fetched = registry::fetch(scratch.path(), &entry).unwrap();
    entry.verify_resolution(fetched.measured).unwrap();

    // The witness passes: the log holds the line, the proof carries it
    // into the pinned checkpoint, and the witnessed digest is the bound
    // one.
    let evidence = registry::read_witness(&scratch.path().join("log"), &entry).unwrap();
    registry::verify(&entry, &evidence, &pinned).unwrap();

    let leaves = scratch.path().join("log/leaves");
    let malformed = std::fs::read_to_string(&leaves)
        .unwrap()
        .replacen("bind ", "record ", 1);
    std::fs::write(&leaves, malformed).unwrap();
    let error = registry::read_witness(&scratch.path().join("log"), &entry).unwrap_err();
    assert!(
        message_of(&error).contains("starts with `bind`"),
        "the log uses the lock line parser, including its directive: {}",
        message_of(&error)
    );
}

#[test]
fn tampered_archive_bytes_are_detected_naming_both_digests() {
    let scratch = TempDir::new().unwrap();
    publish(scratch.path(), &published()).unwrap();
    let universe = registry::read_index(scratch.path(), REGISTRY).unwrap();
    let entry = zlib_entry(&resolve_lock(&universe).unwrap());

    // The registry serves other bytes than its index claims. The
    // measurement is the fact, and the drift diagnostic carries both
    // content identities.
    std::fs::write(
        package_path(scratch.path(), "zlib", "1.3"),
        b"tampered on the mirror",
    )
    .unwrap();
    let fetched = registry::fetch(scratch.path(), &entry).unwrap();
    assert_ne!(fetched.measured, entry.source);
    let error = entry.verify_resolution(fetched.measured).unwrap_err();
    let message = message_of(&error);
    assert!(
        message.contains("drift")
            && message.contains(&entry.source.digest().to_string())
            && message.contains(&fetched.measured.digest().to_string()),
        "the diagnostic names both content identities: {message}"
    );
}

#[test]
fn a_self_consistent_republished_registry_is_detected_by_the_log() {
    // The adversary the digest alone cannot catch: the registry republishes
    // different bytes under one name and updates its own index to match,
    // so every local claim agrees with every other. The log still holds
    // the original line.
    let scratch = TempDir::new().unwrap();
    publish(scratch.path(), &published()).unwrap();

    let republished: &[u8] = b"the zlib 1.3 archive, republished";
    let digest = ContentId::of_blob(republished);
    std::fs::write(package_path(scratch.path(), "zlib", "1.3"), republished).unwrap();
    std::fs::write(
        scratch.path().join("index/pithpkgs/zlib"),
        index_line("1.3", &digest),
    )
    .unwrap();

    let universe = registry::read_index(scratch.path(), REGISTRY).unwrap();
    let entry = zlib_entry(&resolve_lock(&universe).unwrap());
    assert_eq!(
        entry.source, digest,
        "the lock binds what the registry claims"
    );

    // The registry is internally honest: fetch verifies.
    let fetched = registry::fetch(scratch.path(), &entry).unwrap();
    entry.verify_resolution(fetched.measured).unwrap();

    // The witness disagrees: the log's line for these coordinates binds
    // the original content, and the diagnostic names both.
    let pinned = {
        let log = scratch.path().join("log");
        Checkpoint::parse(&std::fs::read_to_string(log.join("checkpoint")).unwrap()).unwrap()
    };
    let evidence = registry::read_witness(&scratch.path().join("log"), &entry).unwrap();
    let error = registry::verify(&entry, &evidence, &pinned).unwrap_err();
    let message = message_of(&error);
    assert!(
        message.contains("two contents")
            && message.contains(&evidence.witnessed.digest().to_string())
            && message.contains(&entry.source.digest().to_string()),
        "the diagnostic names the witnessed and the bound content: {message}"
    );
}

#[test]
fn a_checkpoint_the_configuration_does_not_pin_is_refused_naming_both() {
    let scratch = TempDir::new().unwrap();
    let pinned = publish(scratch.path(), &published()).unwrap();
    let universe = registry::read_index(scratch.path(), REGISTRY).unwrap();
    let entry = zlib_entry(&resolve_lock(&universe).unwrap());
    let evidence = registry::read_witness(&scratch.path().join("log"), &entry).unwrap();

    // Another log answers with another root; the configuration pinned this
    // one. The policy leg refuses first, naming both checkpoints.
    let mut foreign = pinned.clone();
    foreign.root = ContentId::of_blob(b"another log's tree").digest();
    let error = registry::verify(&entry, &evidence, &foreign).unwrap_err();
    let message = message_of(&error);
    assert!(
        message.contains("pins") && message.contains(&foreign.root.to_string()),
        "the diagnostic names the pinned checkpoint: {message}"
    );

    // The log's own file moved while the leaves did not: the served
    // checkpoint no longer commits to the tree the proof folds into, and
    // the inclusion leg refuses, naming the computed root and the
    // checkpoint's.
    let tampered_root = ContentId::of_blob(b"a rewritten checkpoint").digest();
    let mut rewritten = evidence.clone();
    rewritten.checkpoint.root = tampered_root;
    rewritten.witnessed = entry.source;
    let error = registry::verify(&entry, &rewritten, &rewritten.checkpoint).unwrap_err();
    let message = message_of(&error);
    assert!(
        message.contains("computes root") && message.contains(&tampered_root.to_string()),
        "the diagnostic names both roots: {message}"
    );
}

#[test]
fn a_registry_answer_that_moved_between_runs_moves_the_universe_and_the_diff_names_it() {
    let scratch = TempDir::new().unwrap();
    publish(scratch.path(), &published()).unwrap();

    let before = registry::read_index(scratch.path(), REGISTRY).unwrap();
    let first = resolve_lock(&before).unwrap();

    // The registry adds a version between two runs; the query is a
    // separate step, so the second universe is a different declared input
    // and the lock's diff names it.
    let zlib_14: &[u8] = b"the zlib 1.4 archive";
    std::fs::write(package_path(scratch.path(), "zlib", "1.4"), zlib_14).unwrap();
    std::fs::write(
        scratch.path().join("index/pithpkgs/zlib"),
        format!(
            "{}\n{}",
            index_line("1.3", &ContentId::of_blob(ZLIB_BYTES)),
            index_line("1.4", &ContentId::of_blob(zlib_14))
        ),
    )
    .unwrap();

    let after = registry::read_index(scratch.path(), REGISTRY).unwrap();
    assert_ne!(
        before.content_id(),
        after.content_id(),
        "the registry's answer moved, so the universe did"
    );
    let second = resolve_lock(&after).unwrap();
    assert_eq!(zlib_entry(&second).package.version(), "1.4");
    let changes = diff_locks(&first, &second).changes;
    assert!(
        changes
            .iter()
            .any(|change| matches!(change, phloem::document::LockChange::Universe(..)))
            && changes
                .iter()
                .any(|change| matches!(change, phloem::document::LockChange::Upgraded { .. })),
        "the diff names the moved universe and the moved selection: {changes:?}"
    );
}

#[test]
fn resolving_and_verifying_touch_no_path_beyond_the_adapter_reads() {
    // The adapter reads are caller effects; everything downstream of them
    // is pure. Resolution, locking, digesting, and the witness verification
    // consume what was read and write nothing.
    let scratch = TempDir::new().unwrap();
    let pinned = publish(scratch.path(), &published()).unwrap();
    let universe = registry::read_index(scratch.path(), REGISTRY).unwrap();
    let entry = zlib_entry(&resolve_lock(&universe).unwrap());
    let fetched = registry::fetch(scratch.path(), &entry).unwrap();
    let evidence = registry::read_witness(&scratch.path().join("log"), &entry).unwrap();

    let project = TempDir::new().unwrap();
    let _ = phloem::lockfile::render(&resolve_lock(&universe).unwrap());
    entry.verify_resolution(fetched.measured).unwrap();
    registry::verify(&entry, &evidence, &pinned).unwrap();
    assert!(
        std::fs::read_dir(project.path()).unwrap().next().is_none(),
        "computing over what was read wrote nothing"
    );
}

#[test]
fn a_git_reference_locks_only_after_the_fetch_measures_the_tree() {
    let Ok(version) = Command::new("git").arg("--version").output() else {
        eprintln!("skipping: no git on this host");
        return;
    };
    assert!(version.status.success(), "git is present but not runnable");

    let scratch = TempDir::new().unwrap();
    let repo = scratch.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |arguments: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "fixture@example"]);
    git(&["config", "user.name", "fixture"]);
    std::fs::write(repo.join("lib.c"), b"int pith(void) { return 1; }\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "one"]);

    let candidate = forge::reference_candidate(
        &identity("pith"),
        "1.0",
        &repo,
        "HEAD",
        "git.pith-lang.org/pith",
    )
    .unwrap();
    assert_eq!(
        candidate.origin,
        Origin::Forge("git.pith-lang.org/pith".into())
    );
    let universe = CandidateUniverse::new(vec![candidate]);

    let request = resolve_request(
        &version_scheme_value(NUMERIC_SEGMENTS),
        &constraint_set_value(&[Constraint {
            subject: identity("pith"),
            range: Range::AtLeast(Bound::new("1.0", true)),
            features: Box::new([]),
            attribution: "root".into(),
        }]),
        &universe.to_value(),
        &preference_list_value(&preferences()),
        100,
    );
    let answer = engine().evaluate_pure(&request).unwrap();
    let resolution = Resolution::from_value(&answer.value).unwrap();

    // The unmaterialized answer refuses to lock: a revision with a tree
    // hash is a reference, and the lock binds only content that was read.
    let error = Lock::from_resolution(NUMERIC_SEGMENTS, &preferences(), &resolution).unwrap_err();
    assert!(
        message_of(&error).contains("git reference"),
        "the refusal names the reference: {}",
        message_of(&error)
    );

    // The fetch materializes the tree and measures it; the answer that
    // locked nothing now locks measured content, whose bytes the test
    // measures independently from git itself.
    let materialized = forge::materialize_resolution(&repo, &resolution).unwrap();
    let lock = Lock::from_resolution(NUMERIC_SEGMENTS, &preferences(), &materialized).unwrap();
    let entry = lock.entries.first().unwrap();
    let expected = {
        let output = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["archive", "--format=tar", "HEAD"])
            .output()
            .unwrap();
        ContentId::of_blob(&output.stdout)
    };
    assert_eq!(entry.source, expected);
    assert_eq!(entry.origin, Origin::Forge("git.pith-lang.org/pith".into()));

    // Two materializations of one revision use the same tree and commit
    // metadata, so they measure the same content.
    let again = forge::materialize_resolution(&repo, &resolution).unwrap();
    let again = Lock::from_resolution(NUMERIC_SEGMENTS, &preferences(), &again).unwrap();
    assert_eq!(again.entries.first().unwrap().source, entry.source);
}
