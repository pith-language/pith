//! A package builds against another package's artifact (decision 0046): two
//! packages in one registry, the dependent's index line carrying the
//! requirement, the resolution reaching the dependency through it, and the
//! dependent's build consuming the dependency's library in-graph — its
//! objects linked after the dependent's own, its headers provided to the
//! dependent's compiles as request data over an engine whose registered
//! header universe is empty, which is what makes the header half of the edge
//! real rather than registration-fabricated.
//!
//! The reuse asymmetry is the claim the fixture exists for: republishing
//! only the dependency moves the dependent's artifact — the dependent's own
//! tree and build unchanged, its request input moved — while republishing a
//! package neither uses moves no artifact at all. The first is what makes
//! the edge a graph edge; the second is what makes it selective.
//!
//! Linux-gated with `base_case.rs`'s discipline: skip only on
//! `DiscoveryError::NotFound`, fail on a driver that is present but
//! undiscoverable, and keep a sentinel that fails outright when no compiler
//! exists at all so a compiler-less host cannot read as green. The host
//! these were written on is darwin: this file compiled to nothing there, and
//! nothing below is measured until it runs on linux.

#![cfg(target_os = "linux")]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use phloem::build::{
    self, Dependency, PackageBuild, PackageBuildRule, PackageLibraryRule, SourceTree,
};
use phloem::constraint::{Bound, Constraint, Range};
use phloem::document::Lock;
use phloem::identity::{DomainIdentity, NUMERIC_SEGMENTS, PackageIdentity, version_scheme_value};
use phloem::lock::Origin;
use phloem::preference::{Preference, PreferenceList, preference_list_value};
use phloem::registry;
use phloem::resolution::{Resolution, resolve_request};
use phloem::resolve::{ResolveSolver, Schemes};
use phloem::source::SourceBinding;
use phloem::universe::{Candidate, Requirement};
use phloem::witness::{Checkpoint, MerkleTree};
use pith_core::{Pure, Request, Value};
use pith_engine::state::MemoryEngineStateStore;
use pith_engine::{
    AllowAllActions, ComputationKind, Engine, Evaluation, EvaluationSource, TokioRuntime,
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

/// The dependency's tree: one source, one header it offers.
const UTIL_C: (&str, &[u8]) = (
    "util-1.0/util.c",
    b"#include \"util-1.0/util.h\"\nint util(void) { return 7; }\n",
);
const UTIL_H: (&str, &[u8]) = ("util-1.0/util.h", b"int util(void);\n");

/// The dependent: includes the dependency's header by its tree path — the
/// include spelling is the staged path — and exits with what util returns.
const HELLO_C: (&str, &[u8]) = (
    "hello-1.0/hello.c",
    b"#include \"util-1.0/util.h\"\nint main(void) { return util() == 7 ? 0 : 1; }\n",
);

fn identity(name: &str) -> PackageIdentity {
    PackageIdentity::declare(DomainIdentity::new("pithpkgs"), name)
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

/// One published revision of one package: the files its archive holds and
/// the requirements its index line declares.
struct Published {
    name: &'static str,
    version: &'static str,
    files: Vec<(&'static str, &'static [u8])>,
    requires: Box<[Requirement]>,
}

fn hello(requires: Box<[Requirement]>) -> Published {
    Published {
        name: "hello",
        version: "1.0",
        files: vec![HELLO_C],
        requires,
    }
}

fn util(util_c: &'static [u8]) -> Published {
    Published {
        name: "util",
        version: "1.0",
        files: vec![("util-1.0/util.c", util_c), ("util-1.0/util.h", UTIL_H.1)],
        requires: Box::new([]),
    }
}

fn requires_util() -> Box<[Requirement]> {
    Box::new([Requirement {
        subject: identity("util"),
        range: Range::AtLeast(Bound::new("1.0", true)),
        features: Box::new([]),
    }])
}

/// A ustar archive over `files`, authored the way a publisher authors a
/// release tarball: the canonical checksum, spaced before the sum and
/// overwritten after it.
fn archive_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (path, data) in files {
        let mut header = [0_u8; 512];
        header
            .get_mut(..path.len())
            .unwrap_or_else(|| unreachable!("a fixture path fits the name field"))
            .copy_from_slice(path.as_bytes());
        header[257..263].copy_from_slice(b"ustar\0");
        let octal = format!("{:011o}\0", data.len());
        header[124..136].copy_from_slice(octal.as_bytes());
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
    bytes.extend(std::iter::repeat_n(0_u8, 1024));
    bytes
}

fn archive_of_published(published: &Published) -> Vec<u8> {
    archive_of(&published.files)
}

/// Publish a registry and its witnessing log, authoring every index line
/// through the adapter's own spelling so the requirement format has one
/// writer and one reader.
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
    let mut leaves = Vec::new();
    for package in packages {
        let archive = archive_of_published(package);
        let digest = ContentId::of_blob(&archive);
        let candidate = Candidate {
            identity: identity(package.name),
            version: package.version.into(),
            features: Box::new([]),
            provenance: SourceBinding::Archive { archive: digest },
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
            &archive,
        )?;
        leaves.push(phloem::lockfile::binding_line(
            &phloem::lock::LockEntry::new(
                phloem::identity::PackageVersion::new(identity(package.name), package.version),
                [] as [&str; 0],
                digest,
                Origin::Registry(REGISTRY.into()),
            ),
        ));
    }
    for (name, lines) in index {
        write(root.join("index/pithpkgs").join(&name), lines.as_bytes())?;
    }
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

/// The packages' declarations, authored beside the registry the way a
/// package author authors them. The dependency's build names the include it
/// offers; the dependent's names only its own source.
fn util_build() -> PackageBuild {
    PackageBuild {
        sources: Box::new([UTIL_C.0.into()]),
        includes: Box::new([UTIL_H.0.into()]),
    }
}

fn hello_build() -> PackageBuild {
    PackageBuild {
        sources: Box::new([HELLO_C.0.into()]),
        includes: Box::new([]),
    }
}

/// The build engine over a durable root. The header universe is empty on
/// purpose: every header this build sees arrives as request data — the
/// dependency's through its library value, the dependency's own through its
/// build declaration — so an edge that depended on engine registration
/// would fail here rather than pass quietly.
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
    engine.register_rule(PackageLibraryRule::rule(), PackageLibraryRule);
    Ok(engine)
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

/// The declared constraints name the dependent only; the dependency, if it
/// enters, enters through the index line's requirement.
fn resolve_lock(root: &Path) -> pith_diag::PithResult<Lock> {
    let universe = registry::read_index(root, REGISTRY)?;
    let request = resolve_request(
        &version_scheme_value(NUMERIC_SEGMENTS),
        &Value::List(Box::new([Constraint {
            subject: identity("hello"),
            range: Range::AtLeast(Bound::new("1.0", true)),
            features: Box::new([]),
            attribution: "root".into(),
        }
        .to_value()])),
        &universe.to_value(),
        &preference_list_value(&preferences()),
        100,
    );
    let answer = resolving_engine().evaluate_pure(&request)?.value;
    let resolution = Resolution::from_value(&answer)?;
    Lock::from_resolution(NUMERIC_SEGMENTS, &preferences(), &resolution)
}

/// The one request the whole two-package build is: the dependent's tree and
/// build, with the dependency named as `(tree, build)` — the caller derives
/// that from the lock and the declarations, and the graph does the rest.
fn dependent_request(
    toolchain_value: Value,
    hello_tree: &SourceTree,
    util_tree: &SourceTree,
) -> Request<Pure> {
    build::build_request(
        toolchain_value,
        hello_tree,
        &hello_build(),
        &[Dependency {
            tree: util_tree.clone(),
            build: util_build(),
        }],
    )
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

/// Runs a built executable from the store and returns its exit code, with
/// the filesystem work carried as a result so the test bodies unwrap where
/// unwrapping is allowed.
fn exit_code_of(engine_root: &Path, artifact: ContentId) -> pith_diag::PithResult<i32> {
    let bytes = FilesystemContentStore::open(engine_root)
        .map_err(|error| fixture_error(format!("opening the store failed: {error}")))?
        .get_blob(artifact)
        .map_err(|error| fixture_error(format!("reading the artifact failed: {error}")))?
        .ok_or_else(|| fixture_error("the artifact is not in the store".into()))?;
    let run_dir = TempDir::new()
        .map_err(|error| fixture_error(format!("creating a run directory failed: {error}")))?;
    let program = run_dir.path().join("program");
    std::fs::write(&program, bytes.as_bytes())
        .map_err(|error| fixture_error(format!("writing the program failed: {error}")))?;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
        .map_err(|error| fixture_error(format!("chmodding the program failed: {error}")))?;
    let status = std::process::Command::new(&program)
        .status()
        .map_err(|error| fixture_error(format!("running the program failed: {error}")))?;
    status.code().ok_or_else(|| {
        fixture_error(format!(
            "the fixture's programs exit rather than signal: {status}"
        ))
    })
}

/// Resolve, fetch, verify, and unpack both packages over one engine, and
/// return the lock with both trees keyed by package name.
struct Fetched {
    lock: Lock,
    hello_tree: SourceTree,
    util_tree: SourceTree,
}

fn resolve_fetch_unpack(
    registry_root: &Path,
    engine: &mut Engine,
    pinned: &Checkpoint,
) -> pith_diag::PithResult<Fetched> {
    let lock = resolve_lock(registry_root)?;
    let mut trees: Vec<(&'static str, SourceTree)> = Vec::new();
    for name in ["hello", "util"] {
        let entry = lock
            .entries
            .iter()
            .find(|entry| entry.package.identity().name() == name)
            .unwrap_or_else(|| unreachable!("the lock binds {name}"));
        let fetched = registry::fetch(registry_root, entry)?;
        entry.verify_resolution(fetched.measured)?;
        let evidence = registry::read_witness(&registry_root.join("log"), entry)?;
        registry::verify(entry, &evidence, pinned)?;
        trees.push((name, build::unpack(engine, &fetched.bytes)?));
    }
    let tree_of = |name: &str| {
        trees
            .iter()
            .find(|(entry_name, _)| *entry_name == name)
            .map(|(_, tree)| tree.clone())
            .unwrap_or_else(|| unreachable!("the fixture unpacked {name}"))
    };
    Ok(Fetched {
        lock,
        hello_tree: tree_of("hello"),
        util_tree: tree_of("util"),
    })
}

fn toolchain_or_skip(driver: &str) -> Result<Option<Toolchain>, String> {
    match Toolchain::discover(driver) {
        Ok(toolchain) => Ok(Some(toolchain)),
        Err(DiscoveryError::NotFound) => {
            eprintln!("skipping: no {driver} driver on this host");
            Ok(None)
        }
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

/// The round's whole claim, build half: two index lines become one running
/// executable with nothing fabricated between — one resolution reads the
/// requirement, both archives are fetched and measured, the dependency
/// builds as a library in-graph, the dependent's compile sees the
/// dependency's header as request data over an empty registered universe,
/// and the program returns what the dependency's code makes it return.
#[test]
fn a_dependent_builds_against_a_dependency_from_index_lines_to_a_running_program() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let registry_root = TempDir::new().unwrap();
    let pinned = publish(
        registry_root.path(),
        &[hello(requires_util()), util(UTIL_C.1)],
    )
    .unwrap();

    let root = TempDir::new().unwrap();
    let mut engine = engine_at(root.path(), &toolchain).unwrap();
    let fetched = resolve_fetch_unpack(registry_root.path(), &mut engine, &pinned).unwrap();
    assert_eq!(
        fetched.lock.entries.len(),
        2,
        "one request selected both packages"
    );

    let request = dependent_request(toolchain.value(), &fetched.hello_tree, &fetched.util_tree);
    let evaluation = run_build(&mut engine, &request).unwrap();
    assert_eq!(evaluation.source, EvaluationSource::Computed);

    let artifact = blob_of(&evaluation.value).unwrap();
    let bytes = FilesystemContentStore::open(root.path())
        .unwrap()
        .get_blob(artifact)
        .unwrap()
        .expect("the artifact is in the store");
    assert!(bytes.as_bytes().starts_with(ELF_MAGIC));
    assert_eq!(
        exit_code_of(root.path(), artifact).unwrap(),
        0,
        "the program runs and returns what the dependency's code makes it return"
    );

    // The fine-grained shape: the dependent's discovery, the dependent's
    // compile, the dependency's discovery, the dependency's compile, and the
    // link — five actions, the same count a two-source package build runs.
    assert_eq!(
        action_computations(&engine),
        5,
        "two sources, each discovered and compiled, linked once"
    );
}

/// The edge reuses on the engine's machinery alone: the second build of the
/// same two-package lock is `Reused` planning no action, and a fresh engine
/// over the same state root is `Hydrated` allocating none.
#[test]
fn a_two_package_build_reuses_and_hydrates_on_the_engine_alone() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let registry_root = TempDir::new().unwrap();
    let pinned = publish(
        registry_root.path(),
        &[hello(requires_util()), util(UTIL_C.1)],
    )
    .unwrap();

    let root = TempDir::new().unwrap();
    let mut engine = engine_at(root.path(), &toolchain).unwrap();
    let fetched = resolve_fetch_unpack(registry_root.path(), &mut engine, &pinned).unwrap();
    let request = dependent_request(toolchain.value(), &fetched.hello_tree, &fetched.util_tree);
    let computed = run_build(&mut engine, &request).unwrap();
    let actions_after_first = action_computations(&engine);
    assert!(actions_after_first > 0);

    let second = run_build(&mut engine, &request).unwrap();
    assert_eq!(second.source, EvaluationSource::Reused);
    assert_eq!(second.value, computed.value);
    assert_eq!(action_computations(&engine), actions_after_first);

    drop(engine);
    let mut fresh = engine_at(root.path(), &toolchain).unwrap();
    let hydrated = run_build(&mut fresh, &request).unwrap();
    assert_eq!(hydrated.source, EvaluationSource::Hydrated);
    assert_eq!(hydrated.value, computed.value);
    assert_eq!(
        action_computations(&fresh),
        0,
        "a hydrated two-package build allocates no action beneath its root"
    );
}

/// The asymmetry the round exists for. Republishing only the dependency
/// moves the dependent's artifact — the dependent's own tree and build
/// unchanged — and moves it selectively: the dependent's own compile is
/// served, and what runs is the dependency's discovery and compile plus the
/// link. Republishing a package neither uses moves nothing.
#[test]
fn republishing_the_dependency_moves_the_dependent_and_republishing_an_unrelated_moves_nothing() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let registry_root = TempDir::new().unwrap();
    let pinned = publish(
        registry_root.path(),
        &[hello(requires_util()), util(UTIL_C.1)],
    )
    .unwrap();

    let root = TempDir::new().unwrap();
    let mut engine = engine_at(root.path(), &toolchain).unwrap();
    let fetched = resolve_fetch_unpack(registry_root.path(), &mut engine, &pinned).unwrap();
    let request = dependent_request(toolchain.value(), &fetched.hello_tree, &fetched.util_tree);
    let first = run_build(&mut engine, &request).unwrap();
    let first_artifact = blob_of(&first.value).unwrap();
    let actions_after_first = action_computations(&engine);
    assert_eq!(exit_code_of(root.path(), first_artifact).unwrap(), 0);

    // The dependency republishes under one name: different bytes, its index
    // line rewritten to agree. The hello entry is untouched; the util entry
    // drifts; the universe the adapter reads moves. The log moves with the
    // republish, and the configuration re-pins — the same fixture's lying
    // registry is 0044's, where the pin refuses the moved log.
    const UTIL_C_V2: &[u8] = b"#include \"util-1.0/util.h\"\nint util(void) { return 9; }\n";
    let repinned = publish(
        registry_root.path(),
        &[hello(requires_util()), util(UTIL_C_V2)],
    )
    .unwrap();
    let second_lock = resolve_lock(registry_root.path()).unwrap();
    let drifted_util = second_lock
        .entries
        .iter()
        .find(|entry| entry.package.identity().name() == "util")
        .unwrap();
    assert_ne!(drifted_util.source, {
        fetched
            .lock
            .entries
            .iter()
            .find(|entry| entry.package.identity().name() == "util")
            .unwrap()
            .source
    });
    let unchanged_hello = second_lock
        .entries
        .iter()
        .find(|entry| entry.package.identity().name() == "hello")
        .unwrap();
    let first_hello = fetched
        .lock
        .entries
        .iter()
        .find(|entry| entry.package.identity().name() == "hello")
        .unwrap();
    assert_eq!(
        unchanged_hello.source, first_hello.source,
        "the dependent's own binding did not move"
    );

    // The dependent's request input moved — the dependency's tree — so the
    // build recomputes, the artifact moves, and the program now returns what
    // the republished code makes it return.
    let drifted_fetched =
        resolve_fetch_unpack(registry_root.path(), &mut engine, &repinned).unwrap();
    assert!(
        drifted_fetched.util_tree != fetched.util_tree,
        "the dependency's tree is different content"
    );
    assert_eq!(
        drifted_fetched.hello_tree, fetched.hello_tree,
        "the dependent's tree is the same content"
    );
    let second_request = dependent_request(
        toolchain.value(),
        &drifted_fetched.hello_tree,
        &drifted_fetched.util_tree,
    );
    let second = run_build(&mut engine, &second_request).unwrap();
    assert_eq!(second.source, EvaluationSource::Computed);
    let second_artifact = blob_of(&second.value).unwrap();
    assert_ne!(
        second_artifact, first_artifact,
        "a moved dependency is a moved dependent artifact"
    );
    assert_eq!(
        exit_code_of(root.path(), second_artifact).unwrap(),
        1,
        "the program returns what the republished dependency makes it return"
    );

    // Selective: the dependent's discovery and compile are served, and what
    // ran is the dependency's discovery, the dependency's compile, and the
    // link over the new object — three actions, not five.
    assert_eq!(
        action_computations(&engine),
        actions_after_first + 3,
        "the dependent's own compile is served across the moved edge"
    );

    // A package neither uses republishes into the registry: the universe
    // digest moves — it is over every candidate — and nothing the build
    // consumes does, so the engine serves the same artifact.
    let extra = Published {
        name: "extra",
        version: "1.0",
        files: vec![("extra-1.0/extra.c", b"int extra(void) { return 1; }\n")],
        requires: Box::new([]),
    };
    publish(
        registry_root.path(),
        &[hello(requires_util()), util(UTIL_C_V2), extra],
    )
    .unwrap();
    let third_lock = resolve_lock(registry_root.path()).unwrap();
    assert_ne!(
        third_lock.universe, second_lock.universe,
        "the universe is over every candidate the registry answers with"
    );
    let unchanged: Vec<_> = third_lock
        .entries
        .iter()
        .map(|entry| entry.source)
        .collect();
    let second_sources: Vec<_> = second_lock
        .entries
        .iter()
        .map(|entry| entry.source)
        .collect();
    assert_eq!(
        unchanged, second_sources,
        "nothing the resolution selected moved"
    );
    let third = run_build(&mut engine, &second_request).unwrap();
    assert_eq!(
        third.source,
        EvaluationSource::Reused,
        "a registry change no package consumes moves no artifact"
    );
    assert_eq!(blob_of(&third.value).unwrap(), second_artifact);
}

/// The library the dependency builds is the artifact a second dependent
/// consumes: its objects link and its headers compile against, held as one
/// value the graph produced rather than something the caller assembled.
#[test]
fn a_dependency_builds_as_a_library_value() {
    let Some(toolchain) = toolchain_or_skip("cc").unwrap() else {
        return;
    };
    let registry_root = TempDir::new().unwrap();
    let pinned = publish(
        registry_root.path(),
        &[hello(requires_util()), util(UTIL_C.1)],
    )
    .unwrap();

    let root = TempDir::new().unwrap();
    let mut engine = engine_at(root.path(), &toolchain).unwrap();
    let fetched = resolve_fetch_unpack(registry_root.path(), &mut engine, &pinned).unwrap();

    let library_request =
        build::library_request(toolchain.value(), &fetched.util_tree, &util_build());
    let evaluation = run_build(&mut engine, &library_request).unwrap();
    let library = phloem::build::Library::from_value(&evaluation.value).unwrap();
    assert_eq!(library.objects.len(), 1, "the dependency's one source");
    assert_eq!(
        library.headers.len(),
        1,
        "the dependency's one offered include"
    );
    assert_eq!(
        library.headers.first().unwrap().0.as_ref(),
        UTIL_H.0,
        "the include spelling is the tree path"
    );
    assert_eq!(
        library.headers.first().unwrap().1,
        ContentId::of_blob(UTIL_H.1),
        "the header is measured content"
    );
}
