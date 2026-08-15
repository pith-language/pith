//! The dependency edge, resolution half (decision 0046): an index line that
//! carries a requirement is read into the candidate the universe spells, one
//! resolve request against the declared interface walks the edge — the solver
//! reaches the dependency through the dependent's requirement, not through a
//! constraint the caller wrote — and the failure derivation names the
//! candidate whose requirement emptied. The universe digest moves when a
//! requirement moves with nothing else changing, and the lock's diff says so.
//!
//! Resolution is pure, so none of this needs a toolchain: the fixture
//! publishes a registry and resolves against it. The build half of the edge
//! is `two_package_build.rs`, which is linux-gated.

use std::path::Path;

use phloem::constraint::{Bound, Constraint, Range, constraint_set_value};
use phloem::document::{Lock, LockChange, diff as diff_locks};
use phloem::identity::{DomainIdentity, NUMERIC_SEGMENTS, PackageIdentity, version_scheme_value};
use phloem::lock::Origin;
use phloem::preference::{Preference, PreferenceList, preference_list_value};
use phloem::registry;
use phloem::resolution::{Resolution, resolve_request};
use phloem::resolve::{ResolveSolver, Schemes};
use phloem::universe::{Candidate, Requirement};
use phloem::witness::{Checkpoint, MerkleTree};
use pith_engine::Engine;
use pith_engine::state::MemoryEngineStateStore;
use pith_ids::ContentId;
use pith_store::MemoryContentStore;
use tempfile::TempDir;

const REGISTRY: &str = "registries.pith-lang.org";
const LOG: &str = "logs.pith-lang.org";
const HELLO_BYTES: &[u8] = b"the hello 1.0 archive, as bytes";
const UTIL_BYTES: &[u8] = b"the util 1.0 archive, as bytes";
const UNRELATED_BYTES: &[u8] = b"the extra 1.0 archive, as bytes";

fn identity(name: &str) -> PackageIdentity {
    PackageIdentity::declare(DomainIdentity::new("pithpkgs"), name)
}

/// One published version: the bytes the archive holds and the requirements
/// the index line claims for it.
struct Published {
    name: &'static str,
    version: &'static str,
    bytes: &'static [u8],
    requires: Box<[Requirement]>,
}

fn hello(requires: Box<[Requirement]>) -> Published {
    Published {
        name: "hello",
        version: "1.0",
        bytes: HELLO_BYTES,
        requires,
    }
}

fn util() -> Published {
    Published {
        name: "util",
        version: "1.0",
        bytes: UTIL_BYTES,
        requires: Box::new([]),
    }
}

/// The requirement the dependent's index line declares: util, at least 1.0.
fn requires_util() -> Box<[Requirement]> {
    Box::new([Requirement {
        subject: identity("util"),
        range: Range::AtLeast(Bound::new("1.0", true)),
        features: Box::new([]),
    }])
}

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

/// Publish a registry and its witnessing log, authoring each index line
/// through the adapter's own spelling so the format has one writer and one
/// reader rather than a fixture's private guess at the line.
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
        let candidate = Candidate {
            identity: identity(package.name),
            version: package.version.into(),
            features: Box::new([]),
            provenance: phloem::source::SourceBinding::Archive {
                archive: ContentId::of_blob(package.bytes),
            },
            origin: Origin::Registry(REGISTRY.into()),
            requires: package.requires.clone(),
        };
        let line = registry::index_line(&candidate);
        match index.iter_mut().find(|(name, _)| name == package.name) {
            Some((_, lines)) => lines.push_str(&format!("{line}\n")),
            None => index.push((package.name.into(), format!("{line}\n"))),
        }
        write(
            root.join("pkg/pithpkgs")
                .join(format!("{}-{}.tar", package.name, package.version)),
            package.bytes,
        )?;
    }
    for (name, lines) in index {
        write(root.join("index/pithpkgs").join(&name), lines.as_bytes())?;
    }
    let leaves: Vec<String> = packages
        .iter()
        .map(|package| {
            phloem::lockfile::binding_line(&phloem::lock::LockEntry::new(
                phloem::identity::PackageVersion::new(identity(package.name), package.version),
                [] as [&str; 0],
                ContentId::of_blob(package.bytes),
                Origin::Registry(REGISTRY.into()),
            ))
        })
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

fn resolving_engine() -> Engine {
    let mut engine = Engine::with_state_store(
        MemoryContentStore::default(),
        MemoryEngineStateStore::default(),
    );
    let solver = ResolveSolver::new(Schemes::standard());
    engine.register_rule(solver.rule(), solver);
    engine
}

fn preferences() -> PreferenceList {
    PreferenceList(Box::new([Preference::Newest]))
}

/// The declared constraints: hello only. The constraint set names no
/// dependency — the edge, if it is walked, is walked by the solver reading
/// the index.
fn constraints() -> Box<pith_core::Value> {
    Box::new(constraint_set_value(&[Constraint {
        subject: identity("hello"),
        range: Range::AtLeast(Bound::new("1.0", true)),
        features: Box::new([]),
        attribution: "root".into(),
    }]))
}

fn resolve_lock(root: &Path) -> pith_diag::PithResult<(Lock, Resolution)> {
    let universe = registry::read_index(root, REGISTRY)?;
    let request = resolve_request(
        &version_scheme_value(NUMERIC_SEGMENTS),
        &constraints(),
        &universe.to_value(),
        &preference_list_value(&preferences()),
        100,
    );
    let answer = resolving_engine().evaluate_pure(&request)?.value;
    let resolution = Resolution::from_value(&answer)?;
    let lock = Lock::from_resolution(NUMERIC_SEGMENTS, &preferences(), &resolution)?;
    Ok((lock, resolution))
}

/// One request walks the edge: hello's index line carries the requirement,
/// the choice holds both packages, and the util selection is attributed to
/// hello's candidate rather than to any root constraint.
#[test]
fn the_solver_reaches_a_dependency_through_the_dependents_requirement() {
    let root = TempDir::new().unwrap();
    publish(root.path(), &[hello(requires_util()), util()]).unwrap();
    let (lock, resolution) = resolve_lock(root.path()).unwrap();
    let Resolution::Solved { choice, .. } = &resolution else {
        unreachable!("the two-package registry resolves, found {resolution:?}");
    };
    let selected: Vec<&str> = choice
        .iter()
        .map(|candidate| candidate.identity.name())
        .collect();
    assert_eq!(
        selected,
        ["hello", "util"],
        "the choice holds both packages, selected in the order the search visited them"
    );
    assert!(
        choice
            .iter()
            .find(|candidate| candidate.identity.name() == "hello")
            .is_some_and(|hello| hello.requires == requires_util()),
        "the chosen hello candidate carries the requirement the index declared"
    );

    // The lock holds one entry per package, canonically sorted over the
    // entries' encoded bytes — `util` sorts before `hello` because a shorter
    // length prefix precedes a longer one — and nothing about an entry says
    // who required it: the requirement's attribution lives in the
    // resolution's explanation, not in the pins.
    let names: Vec<&str> = lock
        .entries
        .iter()
        .map(|entry| entry.package.identity().name())
        .collect();
    assert_eq!(names, ["util", "hello"]);

    // A pinned re-resolution under the same universe reproduces both
    // selections, which is what makes the two-entry lock a lock.
    let pins: Vec<Constraint> = lock
        .entries
        .iter()
        .map(|entry| Constraint {
            subject: entry.package.identity().clone(),
            range: Range::Exactly(entry.package.version().into()),
            features: entry.features.clone(),
            attribution: "pin".into(),
        })
        .collect();
    let universe = registry::read_index(root.path(), REGISTRY).unwrap();
    let pinned = resolve_request(
        &version_scheme_value(NUMERIC_SEGMENTS),
        &constraint_set_value(&pins),
        &universe.to_value(),
        &preference_list_value(&preferences()),
        100,
    );
    let answer = resolving_engine().evaluate_pure(&pinned).unwrap().value;
    let repinned = Lock::from_resolution(
        NUMERIC_SEGMENTS,
        &preferences(),
        &Resolution::from_value(&answer).unwrap(),
    )
    .unwrap();
    assert_eq!(repinned.entries, lock.entries);
}

/// The unsatisfiable pair reports a derivation naming the requiring
/// candidate: the constraint that emptied is hello's requirement, not a root
/// constraint and not an anonymous range.
#[test]
fn an_unsatisfiable_pair_names_the_requiring_candidate() {
    let root = TempDir::new().unwrap();
    let short = Published {
        version: "0.9",
        ..util()
    };
    publish(root.path(), &[hello(requires_util()), short]).unwrap();
    let universe = registry::read_index(root.path(), REGISTRY).unwrap();
    let request = resolve_request(
        &version_scheme_value(NUMERIC_SEGMENTS),
        &constraints(),
        &universe.to_value(),
        &preference_list_value(&preferences()),
        100,
    );
    let answer = resolving_engine().evaluate_pure(&request).unwrap().value;
    let Resolution::Unsatisfiable { derivation } = Resolution::from_value(&answer).unwrap() else {
        unreachable!("util 0.9 against a >=1.0 requirement is unsatisfiable");
    };
    assert_eq!(derivation.subject, identity("util"));
    let attributions: Vec<&str> = derivation
        .constraints
        .iter()
        .map(|constraint| constraint.attribution.as_ref())
        .collect();
    assert!(
        attributions
            .iter()
            .any(|attribution| attribution == &format!("candidate pithpkgs/hello {}", "1.0")),
        "the derivation attributes the emptying constraint to the requiring candidate: \
         {derivation:?}"
    );
    assert!(
        !attributions
            .iter()
            .any(|attribution| attribution == &"root"),
        "no root constraint names util, and the derivation must not invent one"
    );
}

/// The universe digest moves when a requirement moves and nothing else does,
/// and the lock's diff names the moved universe: a requirement is index data,
/// a changed requirement is a changed registry answer, and a lock written
/// from the old one records which universe it moved from.
#[test]
fn a_moved_requirement_moves_the_universe_digest_and_the_diff_names_it() {
    let before = TempDir::new().unwrap();
    publish(before.path(), &[hello(requires_util()), util()]).unwrap();
    let (first_lock, _) = resolve_lock(before.path()).unwrap();

    let after = TempDir::new().unwrap();
    let bounded = Box::new([Requirement {
        subject: identity("util"),
        range: Range::Between {
            lower: Bound::new("1.0", true),
            upper: Bound::new("2.0", false),
        },
        features: Box::new([]),
    }]);
    publish(after.path(), &[hello(bounded), util()]).unwrap();
    let universe_before = registry::read_index(before.path(), REGISTRY).unwrap();
    let universe_after = registry::read_index(after.path(), REGISTRY).unwrap();
    assert_ne!(
        universe_before.content_id(),
        universe_after.content_id(),
        "the requirement is part of the candidate the universe spells"
    );

    let (second_lock, _) = resolve_lock(after.path()).unwrap();
    let changes = diff_locks(&first_lock, &second_lock).changes;
    assert!(
        changes
            .iter()
            .any(|change| matches!(change, LockChange::Universe(..))),
        "the diff names the moved universe: {changes:?}"
    );
    assert!(
        !changes
            .iter()
            .any(|change| matches!(change, LockChange::Drifted { .. })),
        "the archives did not move, so no entry drifts: {changes:?}"
    );
}

/// A registry that gains a package nothing requires moves the universe —
/// the digest is over every candidate — without moving any entry, which is
/// the other half of what the diff separates.
#[test]
fn an_unrelated_publication_moves_the_universe_and_no_entry() {
    let before = TempDir::new().unwrap();
    publish(before.path(), &[hello(requires_util()), util()]).unwrap();
    let (first_lock, _) = resolve_lock(before.path()).unwrap();

    let after = TempDir::new().unwrap();
    let extra = Published {
        name: "extra",
        version: "1.0",
        bytes: UNRELATED_BYTES,
        requires: Box::new([]),
    };
    publish(after.path(), &[hello(requires_util()), util(), extra]).unwrap();
    let (second_lock, _) = resolve_lock(after.path()).unwrap();
    assert_eq!(
        first_lock.entries, second_lock.entries,
        "nothing the resolution reads moved"
    );
    let changes = diff_locks(&first_lock, &second_lock).changes;
    assert!(
        changes
            .iter()
            .any(|change| matches!(change, LockChange::Universe(..)))
    );
}

/// A malformed requirement is refused at the read, naming the package and
/// the line, because a registry that spells a requirement this format cannot
/// parse is a registry answer to refuse rather than silently narrow.
#[test]
fn a_malformed_requirement_is_refused_naming_the_line() {
    let root = TempDir::new().unwrap();
    publish(root.path(), &[hello(requires_util()), util()]).unwrap();
    let index = root.path().join("index/pithpkgs/hello");
    let text = std::fs::read_to_string(&index).unwrap();
    std::fs::write(
        &index,
        text.replace("requires pithpkgs/util >=1.0", "requires pithpkgs/util 1.0"),
    )
    .unwrap();
    let error = registry::read_index(root.path(), REGISTRY).unwrap_err();
    assert!(
        error
            .iter()
            .any(|diagnostic| diagnostic.message.0.contains("hello")
                && diagnostic.message.0.contains("range")),
        "the diagnostic names the package and the malformed clause: {error:?}"
    );
}
