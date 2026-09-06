//! The registry index and its client: publication derives and signs, the
//! four checks refuse each at its own boundary, and the same module bytes
//! loaded locally and through the signed index elaborate identically.
//!
//! Every fixture runs one directory host with fixed seeds and fixed
//! admission times, so an index on disk is a function of what was
//! published to it and nothing else.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use pith_engine::{Engine, MemoryEngineStateStore};
use pith_hir::{FrontendCode, ModuleSubject, RootKey};
use pith_loader::registry::{Host, Record, RegistryStore, Signing, derive};
use pith_loader::{
    ElaboratedWorkspace, FrontendProjection, RegisterFrontend, ResolvedModule, Workspace,
};
use pith_store::MemoryContentStore;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const ROOT_SEED: [u8; 32] = [1; 32];
const PUBLISHER_SEED: [u8; 32] = [3; 32];
const SUCCESSOR_SEED: [u8; 32] = [5; 32];
const ADMITTED: u64 = 1_757_000_000;

const DEP_MANIFEST: &str = "module example/dep 1.2.0\n";
const DEP_TYPES: &str = "nominal Message = Text\n";
const DEP_RULES: &str = "pure rule speak(who: Message) -> Text = { \"first\" }\n";
const ROOT_MAIN: &str = "import dep\n\nentry hello : Text = ask (Message(\"hello\"))\n";

fn root() -> Signing {
    Signing::from_seed(ROOT_SEED)
}

fn publisher() -> Signing {
    Signing::from_seed(PUBLISHER_SEED)
}

fn successor() -> Signing {
    Signing::from_seed(SUCCESSOR_SEED)
}

fn pinned_key() -> RootKey {
    RootKey::parse(&root().public().spelling())
        .unwrap_or_else(|_| unreachable!("the fixture's root key spelling parses"))
}

fn file(path: &Path, text: &str) -> TestResult {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

/// The two modules every fixture publishes: a dependency and a root that
/// imports it.
fn staged_project(staging: &Path, dep_manifest: &str, root_manifest: &str) -> TestResult {
    file(&staging.join("module.pi"), root_manifest)?;
    file(&staging.join("src/main.pi"), ROOT_MAIN)?;
    file(&staging.join("dep/module.pi"), dep_manifest)?;
    file(&staging.join("dep/src/types.pi"), DEP_TYPES)?;
    file(&staging.join("dep/src/rules.pi"), DEP_RULES)?;
    Ok(())
}

/// The routed root manifest a consumer writes: the registry named, its
/// root key pinned, the domain bound, the dependency ranged.
fn routed_root() -> String {
    format!(
        "module example/root 0.1.0\n\nregistry fixture = \"dir\" root \"{}\"\ndomain example \
         from fixture\nuse dep = example/dep >= 1.2\n",
        root().public().spelling()
    )
}

/// [`derive`] with the test's error type: a derivation refusal renders as
/// its diagnostics.
fn derive_module(directory: &Path) -> TestResult<pith_loader::registry::Draft> {
    derive(directory).map_err(|diagnostics| boxed(format!("the module derives: {diagnostics:?}")))
}

fn boxed(message: String) -> Box<dyn std::error::Error> {
    message.into()
}

/// A registry serving the two published modules under the enrolled
/// publisher key.
fn serving_registry() -> TestResult<tempfile::TempDir> {
    serving_registry_with(|host, staging| {
        host.enroll(
            &root(),
            "example",
            &BTreeSet::from([publisher().public()]),
            1,
            1,
        )?;
        let dep = derive_module(&staging.join("dep"))?;
        host.publish(&dep, &publisher(), ADMITTED)?;
        let root_module = derive_module(staging)?;
        host.publish(&root_module, &publisher(), ADMITTED + 60)?;
        Ok(())
    })
}

/// A registry host plus its staging project, after `publish` has enrolled
/// and published what it chose to. The staged root carries the consumer's
/// own routing clauses, because the fixture resolves it through the
/// registry it names.
fn serving_registry_with(
    publish: impl FnOnce(&Host, &Path) -> TestResult,
) -> TestResult<tempfile::TempDir> {
    let registry = tempfile::tempdir()?;
    let host = Host::new(registry.path());
    host.initialize(&root())?;
    let staging = tempfile::tempdir()?;
    staged_project(staging.path(), DEP_MANIFEST, &routed_root())?;
    publish(&host, staging.path())?;
    Ok(registry)
}

/// An opened store over a serving registry, with its diagnostics intact
/// for the refusals a test asserts by code.
fn opened(directory: &Path) -> Result<(RegistryStore, Record), Box<[pith_diag::Diag]>> {
    RegistryStore::open(directory.to_path_buf(), &pinned_key(), &Record::empty())
}

fn opened_ok(directory: &Path) -> TestResult<(RegistryStore, Record)> {
    opened(directory).map_err(|diagnostics| boxed(format!("the index verifies: {diagnostics:?}")))
}

fn opened_with_prior(
    directory: &Path,
    prior: &Record,
) -> Result<(RegistryStore, Record), Box<[pith_diag::Diag]>> {
    RegistryStore::open(directory.to_path_buf(), &pinned_key(), prior)
}

fn admitted_with_prior(directory: &Path, prior: &Record) -> TestResult<(RegistryStore, Record)> {
    opened_with_prior(directory, prior)
        .map_err(|diagnostics| boxed(format!("the index verifies: {diagnostics:?}")))
}

/// The workspace of a root served by the registry itself.
fn resolved_through(directory: &Path) -> Result<Workspace, Box<[pith_diag::Diag]>> {
    let (store, _record) =
        RegistryStore::open(directory.to_path_buf(), &pinned_key(), &Record::empty())?;
    let subject = ModuleSubject::parse("example/root")
        .unwrap_or_else(|_| unreachable!("the fixture subject parses"));
    let root = store.locate_subject(&subject).map_err(|failure| {
        Box::from([pith_diag::Diag::new(
            pith_diag::Severity::Error,
            FrontendCode::MissingManifest.stable(),
            pith_diag::Span::none(),
            failure.describe(),
        )])
    })?;
    Workspace::resolve(store, &root)
}

fn engine() -> Engine {
    let mut engine = Engine::with_state_store(
        MemoryContentStore::default(),
        MemoryEngineStateStore::default(),
    );
    engine.register_frontend();
    engine
}

fn module_of<'workspace>(
    workspace: &'workspace Workspace,
    spelling: &str,
) -> &'workspace ResolvedModule {
    workspace
        .modules()
        .find(|module| module.subject().spelling().as_ref() == spelling)
        .unwrap_or_else(|| unreachable!("the closure resolved {spelling}"))
}

/// One module's semantic identity through the whole pipeline, the shape
/// `acquisition.rs` compares stores by.
struct Semantic {
    source_ids: Vec<pith_ids::ContentId>,
    abi: pith_ids::ModuleAbiDigest,
    surface: pith_ids::ContentId,
}

fn semantics(workspace: &Workspace) -> TestResult<BTreeMap<Box<str>, Semantic>> {
    let elaborated: ElaboratedWorkspace = workspace
        .elaborate()
        .map_err(|error| format!("the workspace elaborates: {error}"))?;
    let mut engine = engine();
    let projection: FrontendProjection<'_> = workspace
        .project_onto_frontend(&mut engine)
        .map_err(|error| format!("the workspace projects: {error}"))?;
    let mut semantics = BTreeMap::new();
    for ((module, projected), elaborated_module) in projection.modules().zip(elaborated.modules()) {
        let mut source_ids = module
            .sources()
            .files()
            .iter()
            .map(|file| file.content_id())
            .collect::<Vec<_>>();
        source_ids.sort_by_key(|id| id.digest());
        semantics.insert(
            module.subject().spelling(),
            Semantic {
                source_ids,
                abi: elaborated_module.abi_digest(),
                surface: projected.interface().surface(),
            },
        );
    }
    Ok(semantics)
}

fn carries(diagnostics: &[pith_diag::Diag], code: FrontendCode) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == code.stable())
}

#[test]
fn the_same_module_bytes_locally_and_through_the_signed_index_elaborate_identically() -> TestResult
{
    let local = tempfile::tempdir()?;
    staged_project(
        local.path(),
        DEP_MANIFEST,
        "module example/root 0.1.0\n\nuse dep = example/dep from path \"dep\"\n",
    )?;
    let local_workspace = Workspace::load(&local.path().join("module.pi"))
        .map_err(|diagnostics| format!("the local project resolves: {diagnostics:?}"))?;

    let registry = serving_registry()?;
    let acquired = resolved_through(registry.path())
        .map_err(|diagnostics| format!("the registry project resolves: {diagnostics:?}"))?;

    let local_semantics = semantics(&local_workspace)?;
    let acquired_semantics = semantics(&acquired)?;
    for (subject, local) in &local_semantics {
        let through_registry = acquired_semantics
            .get(subject)
            .unwrap_or_else(|| unreachable!("{subject} resolved through the registry too"));
        assert_eq!(
            local.source_ids, through_registry.source_ids,
            "{subject}'s source identities are store-independent"
        );
        assert_eq!(
            local.abi, through_registry.abi,
            "{subject}'s ABI is store-independent"
        );
        assert_eq!(
            local.surface, through_registry.surface,
            "{subject}'s published surface is store-independent"
        );
    }
    let local_dep = module_of(&local_workspace, "example/dep");
    let acquired_dep = module_of(&acquired, "example/dep");
    assert_ne!(local_dep.origin(), acquired_dep.origin());
    assert!(
        acquired_dep.origin().contains("registry/"),
        "the origin names the registry: {}",
        acquired_dep.origin()
    );
    Ok(())
}

#[test]
fn a_line_signed_by_a_key_absent_from_the_domain_key_set_refuses() -> TestResult {
    let registry = serving_registry_with(|host, staging| {
        host.enroll(
            &root(),
            "example",
            &BTreeSet::from([publisher().public()]),
            1,
            1,
        )?;
        let dep = derive_module(&staging.join("dep"))?;
        host.publish(&dep, &successor(), ADMITTED)?;
        Ok(())
    })?;
    let diagnostics = match RegistryStore::open(
        registry.path().to_path_buf(),
        &pinned_key(),
        &Record::empty(),
    ) {
        Ok(_) => unreachable!("an unenrolled key's line is refused"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        carries(&diagnostics, FrontendCode::UnenrolledKey),
        "the refusal names the key and the set: {diagnostics:?}"
    );
    Ok(())
}

#[test]
fn a_key_set_not_signed_by_the_pinned_root_refuses_distinctly() -> TestResult {
    let registry = serving_registry_with(|host, staging| {
        host.enroll(
            &successor(),
            "example",
            &BTreeSet::from([publisher().public()]),
            1,
            1,
        )?;
        let dep = derive_module(&staging.join("dep"))?;
        host.publish(&dep, &publisher(), ADMITTED)?;
        Ok(())
    })?;
    let diagnostics = match RegistryStore::open(
        registry.path().to_path_buf(),
        &pinned_key(),
        &Record::empty(),
    ) {
        Ok(_) => unreachable!("a key set the root never signed is refused"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        carries(&diagnostics, FrontendCode::UnsignedKeySet),
        "the root-mismatch refusal is its own boundary: {diagnostics:?}"
    );
    assert!(
        !carries(&diagnostics, FrontendCode::UnenrolledKey),
        "a bad key-set signature is not a bad release signature: {diagnostics:?}"
    );
    Ok(())
}

#[test]
fn a_generation_that_moved_backwards_refuses_as_rollback() -> TestResult {
    let registry = serving_registry()?;
    // Read at generation 1, rotate to generation 2, and read what the
    // rotation served: the consumer's record now holds generation 2.
    let (_store, first) = opened_ok(registry.path())?;
    Host::new(registry.path()).enroll(
        &root(),
        "example",
        &BTreeSet::from([successor().public()]),
        1,
        2,
    )?;
    let (_store, rotated) = admitted_with_prior(registry.path(), &first)?;
    // Replay the generation-1 set the consumer has already moved past.
    Host::new(registry.path()).enroll(
        &root(),
        "example",
        &BTreeSet::from([publisher().public()]),
        1,
        1,
    )?;
    let diagnostics = match opened_with_prior(registry.path(), &rotated) {
        Ok(_) => unreachable!("a generation that moved backwards is refused"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        carries(&diagnostics, FrontendCode::KeySetRollback),
        "the rollback refusal is its own boundary, not a signature failure: {diagnostics:?}"
    );
    assert!(!carries(&diagnostics, FrontendCode::UnenrolledKey));
    Ok(())
}

#[test]
fn a_previously_admitted_line_now_altered_refuses_as_a_fork() -> TestResult {
    let registry = serving_registry()?;
    let (_store, prior) = opened_ok(registry.path())?;
    // Rewrite the admission time of the dep's line: unsigned by either
    // key, so the publisher's signature still holds and only the
    // append-only check can catch it.
    let dep_index = registry.path().join("index/example/dep");
    let text = fs::read_to_string(&dep_index)?;
    let altered = text.replace(&format!("{ADMITTED}"), &format!("{}", ADMITTED + 1));
    fs::write(&dep_index, altered)?;
    let diagnostics = match opened_with_prior(registry.path(), &prior) {
        Ok(_) => unreachable!("an altered previously admitted line is refused"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        carries(&diagnostics, FrontendCode::IndexFork),
        "the fork refusal names the line and both states: {diagnostics:?}"
    );
    assert!(
        !carries(&diagnostics, FrontendCode::UnenrolledKey),
        "the admission time is covered by no signature, so only the fork check fires: {diagnostics:?}"
    );
    Ok(())
}

#[test]
fn key_rotation_enrolls_the_successor_and_retires_the_old_key() -> TestResult {
    let registry = serving_registry()?;
    let (_store, admitted) = opened_ok(registry.path())?;
    Host::new(registry.path()).enroll(
        &root(),
        "example",
        &BTreeSet::from([successor().public()]),
        1,
        2,
    )?;
    // The lines admitted under generation 1 still read: rotation retires a
    // key for what it may sign next, and the consumer's record already
    // holds what it signed while enrolled.
    admitted_with_prior(registry.path(), &admitted)?;

    // A release signed by the retired key is new to the consumer and
    // answers to the generation-2 set, which does not carry it.
    let staging = tempfile::tempdir()?;
    staged_project(
        staging.path(),
        "module example/other 1.0.0\n",
        "module example/root 0.1.0\n",
    )?;
    let other = derive_module(&staging.path().join("dep"))?;
    Host::new(registry.path()).publish(&other, &publisher(), ADMITTED + 120)?;
    let diagnostics = match opened_with_prior(registry.path(), &admitted) {
        Ok(_) => unreachable!("the retired key's new line is refused"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        carries(&diagnostics, FrontendCode::UnenrolledKey),
        "the refusal names the retired key and the new set: {diagnostics:?}"
    );
    Ok(())
}

#[test]
fn an_entry_whose_cached_fields_disagree_with_the_pinned_revision_refuses() -> TestResult {
    let registry = serving_registry()?;
    // Replace the stored manifest at the revision the entry pins: the
    // index is untouched, so the four checks pass and the disagreement is
    // the consumer's to find.
    let dep_index = registry.path().join("index/example/dep");
    let text = fs::read_to_string(&dep_index)?;
    let revision = text
        .split(' ')
        .find(|token| token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit()))
        .unwrap_or_else(|| unreachable!("the line names its revision digest"))
        .to_string();
    let stored = registry
        .path()
        .join("sources")
        .join(&revision)
        .join("module.pi");
    fs::write(&stored, "module example/dep 2.0.0\n")?;
    let diagnostics = match resolved_through(registry.path()) {
        Ok(_) => unreachable!("a wrong cache is refused, never reconciled"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        carries(&diagnostics, FrontendCode::WrongIndexCache),
        "the refusal names the field and both values: {diagnostics:?}"
    );
    Ok(())
}

#[test]
fn fetched_bytes_whose_tree_disagrees_with_the_entry_refuse() -> TestResult {
    let registry = serving_registry()?;
    let dep_index = registry.path().join("index/example/dep");
    let text = fs::read_to_string(&dep_index)?;
    let revision = text
        .split(' ')
        .find(|token| token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit()))
        .unwrap_or_else(|| unreachable!("the line names its revision digest"))
        .to_string();
    let stored = registry
        .path()
        .join("sources")
        .join(&revision)
        .join("src/rules.pi");
    fs::write(
        &stored,
        "pure rule speak(who: Message) -> Text = { \"second\" }\n",
    )?;
    let diagnostics = match resolved_through(registry.path()) {
        Ok(_) => unreachable!("content that measures against its entry is refused"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        carries(&diagnostics, FrontendCode::ContentMismatch),
        "the refusal names both digests: {diagnostics:?}"
    );
    Ok(())
}

#[test]
fn a_withdrawn_release_is_refused_with_its_reason_named() -> TestResult {
    let registry = serving_registry()?;
    Host::new(registry.path()).withdraw(
        &ModuleSubject::parse("example/dep")
            .unwrap_or_else(|_| unreachable!("the fixture subject parses")),
        pith_hir::ManifestVersion::parse("1.2.0")?,
        "withdrawn for the fixture",
        &publisher(),
    )?;
    let diagnostics = match resolved_through(registry.path()) {
        Ok(_) => unreachable!("a withdrawn release is refused"),
        Err(diagnostics) => diagnostics,
    };
    let refusal = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == FrontendCode::WithdrawnSelection.stable())
        .unwrap_or_else(|| {
            unreachable!("the withdrawal refusal is its own boundary: {diagnostics:?}")
        });
    let message = refusal.message.0.to_string();
    assert!(
        message.contains("withdrawn for the fixture"),
        "the refusal names the reason: {message}"
    );
    Ok(())
}

#[test]
fn an_index_that_cannot_be_read_at_all_is_unreachable() -> TestResult {
    let absent = tempfile::tempdir()?.path().join("no-such-registry");
    let diagnostics = match RegistryStore::open(absent, &pinned_key(), &Record::empty()) {
        Ok(_) => unreachable!("an absent registry is refused"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        carries(&diagnostics, FrontendCode::IndexUnreachable),
        "unreachable is distinct from carrying no entry: {diagnostics:?}"
    );
    Ok(())
}

#[test]
fn two_entries_for_one_canonical_version_refuse() -> TestResult {
    let registry = serving_registry_with(|host, staging| {
        host.enroll(
            &root(),
            "example",
            &BTreeSet::from([publisher().public()]),
            1,
            1,
        )?;
        let dep = derive_module(&staging.join("dep"))?;
        host.publish(&dep, &publisher(), ADMITTED)?;
        // The same release under a spelling that compares equal: `1.2`
        // against `1.2.0`, so a person reads one version where the index
        // would carry two.
        file(&staging.join("dep/module.pi"), "module example/dep 1.2\n")?;
        let respelled = derive_module(&staging.join("dep"))?;
        host.publish(&respelled, &publisher(), ADMITTED + 30)?;
        Ok(())
    })?;
    let diagnostics = match opened(registry.path()) {
        Ok(_) => unreachable!("two entries for one canonical version are refused"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        carries(&diagnostics, FrontendCode::DuplicateVersion),
        "versions that compare equal are one version: {diagnostics:?}"
    );
    Ok(())
}
