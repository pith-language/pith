//! A locked source becomes a built artifact (decision 0045): a registry
//! index read by 0044's adapter resolves and locks through the engine, the
//! bound archive is fetched, measured, and unpacked into a tree, and the
//! package's declared build runs as one pure rule over xylem's compile and
//! link entries, producing an executable that runs — an index line to an
//! ELF with nothing fabricated in between.
//!
//! The claims the record makes are the ones asserted here: the artifact's
//! identity is the kernel's content identity and its computation is the
//! engine's attempt; a second build of the same lock is `Reused` and a
//! fresh engine over the same state root `Hydrated`, on 0031 and 0033's
//! machinery with nothing package-side added; a registry that republishes
//! moves the universe digest, the lock entry, and the artifact, with the
//! diff naming the input that moved; a served substitution publishes no
//! attempt where a refused offer's build publishes exactly that attempt;
//! and an environment document over the same lock names realization
//! coordinates under which a realization now exists.
//!
//! Linux-gated with `base_case.rs`'s discipline: skip only on
//! `DiscoveryError::NotFound`, fail on a driver that is present but
//! undiscoverable, and keep a sentinel that fails outright when no
//! compiler exists at all so a compiler-less host cannot read as green.

#![cfg(target_os = "linux")]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use phloem::build::{self, PackageBuild, PackageBuildRule, SourceTree};
use phloem::constraint::{Bound, Constraint, Range};
use phloem::description::Description;
use phloem::document::{Lock, LockChange, diff as diff_locks};
use phloem::environment::{self, Environment, EnvironmentDocument, Offer};
use phloem::identity::{DomainIdentity, NUMERIC_SEGMENTS, PackageIdentity, version_scheme_value};
use phloem::lock::Origin;
use phloem::preference::{Preference, PreferenceList, preference_list_value};
use phloem::registry;
use phloem::resolution::{Resolution, resolve_request};
use phloem::resolve::{ResolveSolver, Schemes};
use phloem::source::SourceBinding;
use phloem::substitution::{Admission, AdmittedOrigins, BinaryOffer, Serving, serve};
use phloem::witness::{Checkpoint, MerkleTree};
use pith_core::{Pure, Request, Value};
use pith_engine::state::MemoryEngineStateStore;
use pith_engine::{
    AllowAllActions, ComputationKind, Engine, Evaluation, EvaluationSource, ExecutionPlatform,
    TokioRuntime,
};
use pith_executor_local::LocalExecutor;
use pith_ids::ContentId;
use pith_state_sqlite::SqliteEngineStateStore;
use pith_store::{ContentStore, FilesystemContentStore, MemoryContentStore};
use tempfile::TempDir;
use xylem::{BuildEngine, DiscoveryError, HeaderUniverse, Toolchain, Toolchains};

const REGISTRY: &str = "registries.pith-lang.org";
const LOG: &str = "logs.pith-lang.org";
const ELF_MAGIC: &[u8] = b"\x7fELF";

/// The package the fixture publishes: two sources, linked in this order.
const UTIL: (&str, &[u8]) = ("hello-1.0/util.c", b"int util(void) { return 7; }\n");
const HELLO: (&str, &[u8]) = (
    "hello-1.0/hello.c",
    b"int util(void);\nint main(void) { return util() == 7 ? 0 : 1; }\n",
);

fn identity() -> PackageIdentity {
    PackageIdentity::declare(DomainIdentity::new("pithpkgs"), "hello")
}

fn platform() -> ExecutionPlatform {
    ExecutionPlatform {
        operating_system: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
    }
}

/// One published revision: the archive bytes the registry serves.
struct Published {
    archive: Vec<u8>,
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

fn publish(root: &Path, published: &Published) -> pith_diag::PithResult<Checkpoint> {
    let write = |path: std::path::PathBuf, bytes: &[u8]| -> pith_diag::PithResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                fixture_error(format!("creating {} failed: {error}", parent.display()))
            })?;
        }
        std::fs::write(&path, bytes)
            .map_err(|error| fixture_error(format!("writing {} failed: {error}", path.display())))
    };
    let digest = ContentId::of_blob(&published.archive);
    write(
        root.join("index/pithpkgs/hello"),
        format!("1.0 [] sha256:{}\n", digest.digest()).as_bytes(),
    )?;
    write(root.join("pkg/pithpkgs/hello-1.0.tar"), &published.archive)?;
    let binding = phloem::lockfile::binding_line(&phloem::lock::LockEntry::new(
        phloem::identity::PackageVersion::new(identity(), "1.0"),
        [] as [&str; 0],
        digest,
        Origin::Registry(REGISTRY.into()),
    ));
    let tree = MerkleTree::new([binding.as_bytes()])?;
    let checkpoint = Checkpoint {
        origin: LOG.into(),
        size: tree.size(),
        root: tree.root(),
    };
    write(root.join("log/leaves"), format!("{binding}\n").as_bytes())?;
    write(root.join("log/checkpoint"), checkpoint.render().as_bytes())?;
    Ok(checkpoint)
}

/// A ustar archive holding the two sources, authored by the fixture the
/// way a publisher authors a release tarball.
fn archive() -> Vec<u8> {
    let mut bytes = Vec::new();
    for (path, data) in [UTIL, HELLO] {
        let mut header = [0_u8; 512];
        header
            .get_mut(..path.len())
            .unwrap_or_else(|| unreachable!("a fixture path fits the name field"))
            .copy_from_slice(path.as_bytes());
        header[257..263].copy_from_slice(b"ustar\0");
        // The size field is eleven octal digits and a NUL, twelve bytes.
        let octal = format!("{:011o}\0", data.len());
        header[124..136].copy_from_slice(octal.as_bytes());
        // A ustar checksum sums the header with the checksum field read as
        // eight spaces — the reading `archive.rs` checks against — so the
        // field is spaced before the sum and overwritten after it.
        header[148..156].fill(b' ');
        let sum: u64 = header.iter().copied().map(u64::from).sum();
        let checksum = format!("{sum:06o}\0 ");
        header[148..156].copy_from_slice(checksum.as_bytes());
        bytes.extend_from_slice(&header);
        bytes.extend_from_slice(data);
        let padding = data
            .len()
            .div_ceil(512)
            .checked_mul(512)
            .and_then(|padded| padded.checked_sub(data.len()))
            .unwrap_or_else(|| unreachable!("a fixture file's padding fits the machine"));
        bytes.extend(std::iter::repeat_n(0, padding));
    }
    bytes.extend_from_slice(&[0_u8; 1024]);
    bytes
}

/// The package's own declaration, authored beside the registry the way a
/// package author authors it: the source is the archive the index claims,
/// the build names the sources in link order.
fn description(archive: ContentId) -> Description {
    Description {
        name: "hello".into(),
        source: SourceBinding::Archive { archive },
        build: PackageBuild {
            sources: Box::new([UTIL.0.into(), HELLO.0.into()]),
            includes: Box::new([]),
        },
    }
}

/// The engine the build runs on: the durable substrate at `root` — a
/// filesystem content store and a sqlite state database — with the
/// resolver, xylem's rules, and the package-build rule registered. Two
/// engines over one root are successive runs of the same build (0033).
fn engine_at(root: &Path, toolchain: &Toolchain) -> pith_diag::PithResult<Engine> {
    let store = FilesystemContentStore::open(root)
        .map_err(|error| fixture_error(format!("opening the store failed: {error}")))?;
    let state = SqliteEngineStateStore::open(root.join("state.db"))
        .map_err(|error| fixture_error(format!("opening the state database failed: {error}")))?;
    let mut engine = Engine::with_state_store(store, state);
    let solver = ResolveSolver::new(Schemes::standard());
    engine.register_rule(solver.rule(), solver);
    engine.register_xylem(Toolchains::one(toolchain.clone()), HeaderUniverse::empty());
    engine.register_rule(PackageBuildRule::rule(), PackageBuildRule);
    Ok(engine)
}

/// An in-memory engine for resolutions that never touch the build's
/// durable root, the shape `source_adapter` resolves with.
fn resolving_engine() -> Engine {
    let mut engine = Engine::with_state_store(
        MemoryContentStore::default(),
        MemoryEngineStateStore::default(),
    );
    let solver = ResolveSolver::new(Schemes::standard());
    engine.register_rule(solver.rule(), solver);
    engine
}

fn constraints() -> Box<[Value]> {
    Box::new([Constraint {
        subject: identity(),
        range: Range::AtLeast(Bound::new("1.0", true)),
        features: Box::new([]),
        attribution: "root".into(),
    }
    .to_value()])
}

fn preferences() -> PreferenceList {
    PreferenceList(Box::new([Preference::Newest]))
}

/// Read the registry, resolve, and lock — the caller-side half of the
/// round, everything before the fetch.
fn resolve_lock(root: &Path) -> pith_diag::PithResult<Lock> {
    let universe = registry::read_index(root, REGISTRY)?;
    let request = resolve_request(
        &version_scheme_value(NUMERIC_SEGMENTS),
        &Value::List(constraints()),
        &universe.to_value(),
        &preference_list_value(&preferences()),
        100,
    );
    let answer = resolving_engine().evaluate_pure(&request)?.value;
    let resolution = Resolution::from_value(&answer)?;
    Lock::from_resolution(NUMERIC_SEGMENTS, &preferences(), &resolution)
}

fn build_request(toolchain_value: Value, tree: &SourceTree, build: &PackageBuild) -> Request<Pure> {
    build::build_request(toolchain_value, tree, build, &[])
}

fn run_build(engine: &mut Engine, request: &Request<Pure>) -> pith_diag::PithResult<Evaluation> {
    let runtime = TokioRuntime::new()
        .map_err(|error| fixture_error(format!("constructing the runtime failed: {error:?}")))?;
    engine
        .run(request, &runtime, &AllowAllActions, &LocalExecutor::new())
        .map_err(|error| {
            fixture_error(format!("the runtime could not drive the build: {error:?}"))
        })?
}

fn action_computations(engine: &Engine) -> usize {
    engine
        .query()
        .computations()
        .filter(|(_, node)| matches!(node.kind, ComputationKind::Action(_)))
        .count()
}

/// The blob identity a nominal content value carries.
fn blob_of(value: &Value) -> pith_diag::PithResult<ContentId> {
    match value {
        Value::Nominal { representation, .. } => match representation.as_ref() {
            Value::Blob(id) => Ok(*id),
            _ => Err(fixture_error(
                "a nominal content value carried no blob".into(),
            )),
        },
        _ => Err(fixture_error(
            "the value was not a nominal content value".into(),
        )),
    }
}

fn toolchain_or_skip(driver: &str) -> Result<Option<Toolchain>, String> {
    match Toolchain::discover(driver) {
        Ok(toolchain) => Ok(Some(toolchain)),
        Err(DiscoveryError::NotFound) => {
            eprintln!("skipping: no {driver} driver on this host");
            Ok(None)
        }
        // Reachable by design: the driver is on the host but could not be
        // resolved into a closure, which is a failure to report rather than
        // an absence to skip on. The failure surfaces where the test body
        // unwraps it, a helper's `panic!` being outside the lint posture.
        Err(error) => Err(format!("{driver} is present but discovery failed: {error}")),
    }
}

/// A host with no C compiler at all cannot run this file's other tests, and
/// a green run over skips would read as a verified round. Fail instead.
#[test]
fn a_c_toolchain_is_available() {
    let outcomes: Vec<(&str, Result<Toolchain, DiscoveryError>)> = ["cc", "gcc", "clang"]
        .into_iter()
        .map(|driver| (driver, Toolchain::discover(driver)))
        .collect();
    for (driver, outcome) in &outcomes {
        if let Err(error) = outcome {
            assert!(
                matches!(error, DiscoveryError::NotFound),
                "{driver} is present but discovery failed: {error}"
            );
        }
    }
    assert!(
        outcomes.iter().any(|(_, outcome)| outcome.is_ok()),
        "no C compiler (cc, gcc, or clang) on this host: the round cannot run, and its other \
         tests would all skip green: {outcomes:?}"
    );
}

/// The round's whole claim: an index line becomes a running executable
/// with nothing fabricated in the middle — a real registry read, a real
/// resolution, a real fetch, a real unpack, a real compile and link.
#[test]
fn an_index_line_becomes_a_running_executable() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let scratch = TempDir::new().unwrap();
    let pinned = publish(scratch.path(), &Published { archive: archive() }).unwrap();
    let lock = resolve_lock(scratch.path()).unwrap();
    let entry = lock.entries.first().unwrap().clone();
    let declared = description(entry.source);
    assert_eq!(
        declared.source,
        SourceBinding::Archive {
            archive: entry.source
        },
        "the authored declaration and the lock bind the same archive"
    );

    // The fetch reads bytes; the measurement agrees with the binding; the
    // log's witnessed line agrees with both (0044).
    let fetched = registry::fetch(scratch.path(), &entry).unwrap();
    entry.verify_resolution(fetched.measured).unwrap();
    let evidence = registry::read_witness(&scratch.path().join("log"), &entry).unwrap();
    registry::verify(&entry, &evidence, &pinned).unwrap();

    // The unpack is the adapter's second half: parse the tar, import the
    // files, measure each. It holds the two sources the build prescribes.
    let root = TempDir::new().unwrap();
    let mut engine = engine_at(root.path(), &toolchain).unwrap();
    let tree = build::unpack(&mut engine, &fetched.bytes).unwrap();
    assert_eq!(tree.files.len(), 2);
    assert_eq!(
        tree.content_at(HELLO.0),
        Some(ContentId::of_blob(HELLO.1)),
        "a tree file's identity is measured from its bytes"
    );

    let request = build_request(toolchain.value(), &tree, &declared.build);
    let evaluation = run_build(&mut engine, &request).unwrap();
    assert_eq!(
        evaluation.source,
        EvaluationSource::Computed,
        "the first build computes"
    );

    // The artifact's identity is the kernel's: a nominal executable over
    // the content identity the store holds, nothing package-side.
    let artifact = blob_of(&evaluation.value).unwrap();
    let bytes = FilesystemContentStore::open(root.path())
        .unwrap()
        .get_blob(artifact)
        .unwrap()
        .expect("the artifact is in the store");
    assert!(
        bytes.as_bytes().starts_with(ELF_MAGIC),
        "the linked artifact is an ELF executable"
    );
    let program = root.path().join("program");
    std::fs::write(&program, bytes.as_bytes()).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let status = std::process::Command::new(&program).status().unwrap();
    assert!(
        status.success(),
        "the built program ran and exited with {status}"
    );
}

/// 0039's claim, measured: a realization reuses on the engine's machinery
/// alone. The second build of the same lock is served, and a fresh engine
/// over the same state root hydrates rather than rebuilds — 0031's action
/// index and 0033's consumer walk reaching a package build unchanged.
#[test]
fn a_second_build_is_reused_and_a_fresh_engine_hydrates() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let scratch = TempDir::new().unwrap();
    publish(scratch.path(), &Published { archive: archive() }).unwrap();
    let entry = resolve_lock(scratch.path())
        .unwrap()
        .entries
        .first()
        .unwrap()
        .clone();
    let fetched = registry::fetch(scratch.path(), &entry).unwrap();

    let root = TempDir::new().unwrap();
    let mut engine = engine_at(root.path(), &toolchain).unwrap();
    let tree = build::unpack(&mut engine, &fetched.bytes).unwrap();
    let request = build_request(toolchain.value(), &tree, &description(entry.source).build);

    let computed = run_build(&mut engine, &request).unwrap();
    let actions_after_first = action_computations(&engine);
    assert!(actions_after_first > 0, "the first build ran actions");

    let second = run_build(&mut engine, &request).unwrap();
    assert_eq!(
        second.source,
        EvaluationSource::Reused,
        "the same engine serves its own recorded attempt"
    );
    assert_eq!(second.value, computed.value);
    assert_eq!(
        action_computations(&engine),
        actions_after_first,
        "the reused build planned no action"
    );

    // A fresh engine over the same durable state — the first dropped, the
    // way a second process finds the build — hydrates the attempt.
    drop(engine);
    let mut fresh = engine_at(root.path(), &toolchain).unwrap();
    let hydrated = run_build(&mut fresh, &request).unwrap();
    assert_eq!(hydrated.source, EvaluationSource::Hydrated);
    assert_eq!(hydrated.value, computed.value);
    assert_eq!(
        action_computations(&fresh),
        0,
        "a hydrated package build allocates no action beneath it"
    );
}

/// A registry that republishes moves the universe, the entry, and the
/// artifact, and the lock's diff names the input that moved: the drift a
/// binding exists to catch, answered by a rebuild rather than absorbed.
#[test]
fn a_republished_registry_moves_the_universe_the_entry_and_the_artifact() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let scratch = TempDir::new().unwrap();
    publish(scratch.path(), &Published { archive: archive() }).unwrap();
    let first_lock = resolve_lock(scratch.path()).unwrap();
    let first_entry = first_lock.entries.first().unwrap().clone();
    let first_fetch = registry::fetch(scratch.path(), &first_entry).unwrap();

    let root = TempDir::new().unwrap();
    let mut engine = engine_at(root.path(), &toolchain).unwrap();
    let first_tree = build::unpack(&mut engine, &first_fetch.bytes).unwrap();
    let first = run_build(
        &mut engine,
        &build_request(
            toolchain.value(),
            &first_tree,
            &description(first_entry.source).build,
        ),
    )
    .unwrap();

    // The registry republishes under one name: different bytes, its index
    // rewritten to agree with them. The universe the adapter reads moves,
    // and the lock's diff names the moved universe and the drifted entry.
    let republished: &[u8] = b"int util(void) { return 9; }\n";
    let mut rewritten = archive();
    let offset: usize = 512; // the first entry's body begins after its header
    let end = offset
        .checked_add(republished.len())
        .expect("the replacement fits the archive");
    rewritten
        .get_mut(offset..end)
        .expect("the first entry's body begins after its header")
        .copy_from_slice(republished);
    let other = Published { archive: rewritten };
    publish(scratch.path(), &other).unwrap();
    let second_lock = resolve_lock(scratch.path()).unwrap();
    let second_entry = second_lock.entries.first().unwrap().clone();
    assert_ne!(second_entry.source, first_entry.source);
    let changes = diff_locks(&first_lock, &second_lock).changes;
    assert!(
        changes
            .iter()
            .any(|change| matches!(change, LockChange::Universe(..)))
            && changes
                .iter()
                .any(|change| matches!(change, LockChange::Drifted { .. })),
        "the diff names the moved universe and the drifted entry: {changes:?}"
    );

    // The new tree is new content, so the build is a different computation
    // and the artifact moves with it.
    let second_fetch = registry::fetch(scratch.path(), &second_entry).unwrap();
    let second_tree = build::unpack(&mut engine, &second_fetch.bytes).unwrap();
    assert_ne!(
        second_tree.content_at(UTIL.0),
        first_tree.content_at(UTIL.0),
        "the republished source is different content"
    );
    let second = run_build(
        &mut engine,
        &build_request(
            toolchain.value(),
            &second_tree,
            &description(second_entry.source).build,
        ),
    )
    .unwrap();
    assert_ne!(
        blob_of(&second.value).unwrap(),
        blob_of(&first.value).unwrap(),
        "a moved source is a moved artifact"
    );
}

/// 0042's central claim against a real build: a served substitution
/// publishes no attempt under the key the build's own request derives,
/// and the build a refused offer leaves running publishes exactly that
/// attempt — held on what the engine records, not on a difference of
/// literals.
#[test]
fn a_served_substitution_publishes_no_attempt_and_a_refused_ones_build_publishes_it() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let scratch = TempDir::new().unwrap();
    publish(scratch.path(), &Published { archive: archive() }).unwrap();
    let entry = resolve_lock(scratch.path())
        .unwrap()
        .entries
        .first()
        .unwrap()
        .clone();
    let fetched = registry::fetch(scratch.path(), &entry).unwrap();

    let offer = BinaryOffer::new(
        entry.package.clone(),
        entry.features.clone(),
        entry.source,
        platform(),
        toolchain.value(),
        ContentId::of_blob(b"hello-1.0 substitute binary"),
        Origin::Forge("builds.pith-lang.org".into()),
    );
    let binary: &[u8] = b"hello-1.0 substitute binary";
    let running = platform();
    let builder = offer.origin.clone();
    let admitted = AdmittedOrigins(Box::new([builder]));
    let narrow = AdmittedOrigins(Box::new([]));
    let toolchain_value = toolchain.value();

    // The refused offer's build runs and is recorded; the next run of the
    // same request is served from that attempt.
    let refused_root = TempDir::new().unwrap();
    let mut engine = engine_at(refused_root.path(), &toolchain).unwrap();
    let tree = build::unpack(&mut engine, &fetched.bytes).unwrap();
    let request = build_request(toolchain.value(), &tree, &description(entry.source).build);
    let refused = serve(
        &Admission {
            entry: &entry,
            platform: &running,
            toolchain: &toolchain_value,
            origins: &narrow,
        },
        Some((&offer, binary)),
    );
    assert!(
        matches!(&refused, Serving::Built { refused: Some(_) }),
        "an unauthorized origin builds in the binary's place"
    );
    let built = run_build(&mut engine, &request).unwrap();
    assert_eq!(built.source, EvaluationSource::Computed);
    let again = run_build(&mut engine, &request).unwrap();
    assert_eq!(
        again.source,
        EvaluationSource::Reused,
        "the build the refusal left running published exactly the attempt now served"
    );

    // The served substitution publishes nothing: over a state root where
    // only the substitution served, the same request computes, because
    // there is no attempt to reuse.
    let served_root = TempDir::new().unwrap();
    let mut engine = engine_at(served_root.path(), &toolchain).unwrap();
    let tree = build::unpack(&mut engine, &fetched.bytes).unwrap();
    let substituted = serve(
        &Admission {
            entry: &entry,
            platform: &running,
            toolchain: &toolchain_value,
            origins: &admitted,
        },
        Some((&offer, binary)),
    );
    assert!(matches!(substituted, Serving::Substituted(_)));
    let request = build_request(toolchain.value(), &tree, &description(entry.source).build);
    let computed = run_build(&mut engine, &request).unwrap();
    assert_eq!(
        computed.source,
        EvaluationSource::Computed,
        "no attempt was published under the key the build's request derives, so the \
         build runs where the refused case was served"
    );
}

/// 0043's document over a lock whose realizations now exist: the
/// coordinates the environment names are the ones the build ran under,
/// and the artifact those coordinates realize is in the store.
#[test]
fn an_environment_document_names_realizations_that_now_exist() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let scratch = TempDir::new().unwrap();
    publish(scratch.path(), &Published { archive: archive() }).unwrap();
    let lock = resolve_lock(scratch.path()).unwrap();
    let entry = lock.entries.first().unwrap().clone();
    let fetched = registry::fetch(scratch.path(), &entry).unwrap();

    let root = TempDir::new().unwrap();
    let mut engine = engine_at(root.path(), &toolchain).unwrap();
    let tree = build::unpack(&mut engine, &fetched.bytes).unwrap();
    let built = run_build(
        &mut engine,
        &build_request(toolchain.value(), &tree, &description(entry.source).build),
    )
    .unwrap();

    let universe = registry::read_index(scratch.path(), REGISTRY).unwrap();
    let declaration = Environment {
        name: environment::DEFAULT_ENVIRONMENT.into(),
        constraints: Box::new([Constraint {
            subject: identity(),
            range: Range::AtLeast(Bound::new("1.0", true)),
            features: Box::new([]),
            attribution: "root".into(),
        }]),
        platform: platform(),
        toolchain: toolchain.value(),
        origins: AdmittedOrigins(Box::new([])),
    };
    let realized = EnvironmentDocument::resolve(
        &declaration,
        &mut resolving_engine(),
        &universe,
        NUMERIC_SEGMENTS,
        &preferences(),
        100,
        &[Offer {
            offer: &BinaryOffer::new(
                entry.package.clone(),
                entry.features.clone(),
                entry.source,
                platform(),
                toolchain.value(),
                ContentId::of_blob(b"hello-1.0 substitute binary"),
                Origin::Forge("builds.pith-lang.org".into()),
            ),
            bytes: b"hello-1.0 substitute binary",
        }],
    )
    .unwrap();
    let document = realized.document;

    // The lock the environment holds is the lock the build realized, and
    // the coordinates it declares are the ones the build ran under.
    assert_eq!(document.lock, lock);
    assert_eq!(document.toolchain, toolchain.value());
    assert_eq!(document.platform, platform());

    // The realization those coordinates name now exists: the artifact the
    // build produced is in the store under the environment's own engine
    // root, the same content the build's evaluation named.
    let artifact = blob_of(&built.value).unwrap();
    assert!(
        FilesystemContentStore::open(root.path())
            .unwrap()
            .get_blob(artifact)
            .unwrap()
            .is_some(),
        "the realization the document's coordinates name exists as content"
    );
}
