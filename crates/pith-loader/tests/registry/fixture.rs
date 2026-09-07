//! The shared fixture: fixed keys, a staged two-module project, a
//! directory host serving it, and the opened-store helpers with their
//! diagnostics intact for refusals asserted by code.

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

pub(crate) type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

pub(crate) const ROOT_SEED: [u8; 32] = [1; 32];
pub(crate) const PUBLISHER_SEED: [u8; 32] = [3; 32];
pub(crate) const SUCCESSOR_SEED: [u8; 32] = [5; 32];
pub(crate) const ADMITTED: u64 = 1_757_000_000;

pub(crate) const DEP_MANIFEST: &str = "module example/dep 1.2.0\n";
pub(crate) const DEP_TYPES: &str = "nominal Message = Text\n";
pub(crate) const DEP_RULES: &str = "pure rule speak(who: Message) -> Text = { \"first\" }\n";
pub(crate) const ROOT_MAIN: &str = "import dep\n\nentry hello : Text = ask (Message(\"hello\"))\n";

pub(crate) fn root() -> Signing {
    Signing::from_seed(ROOT_SEED)
}

pub(crate) fn publisher() -> Signing {
    Signing::from_seed(PUBLISHER_SEED)
}

pub(crate) fn successor() -> Signing {
    Signing::from_seed(SUCCESSOR_SEED)
}

pub(crate) fn pinned_key() -> RootKey {
    RootKey::parse(&root().public().spelling())
        .unwrap_or_else(|_| unreachable!("the fixture's root key spelling parses"))
}

pub(crate) fn file(path: &Path, text: &str) -> TestResult {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

/// The two modules every fixture publishes: a dependency and a root that
/// imports it.
pub(crate) fn staged_project(
    staging: &Path,
    dep_manifest: &str,
    root_manifest: &str,
) -> TestResult {
    file(&staging.join("module.pi"), root_manifest)?;
    file(&staging.join("src/main.pi"), ROOT_MAIN)?;
    file(&staging.join("dep/module.pi"), dep_manifest)?;
    file(&staging.join("dep/src/types.pi"), DEP_TYPES)?;
    file(&staging.join("dep/src/rules.pi"), DEP_RULES)?;
    Ok(())
}

/// The routed root manifest a consumer writes: the registry named, its
/// root key pinned, the domain bound, the dependency ranged.
pub(crate) fn routed_root() -> String {
    format!(
        "module example/root 0.1.0\n\nregistry fixture = \"dir\" root \"{}\"\ndomain example \
         from fixture\nuse dep = example/dep >= 1.2\n",
        root().public().spelling()
    )
}

/// [`derive`] with the test's error type: a derivation refusal renders as
/// its diagnostics.
pub(crate) fn derive_module(directory: &Path) -> TestResult<pith_loader::registry::Draft> {
    derive(directory).map_err(|diagnostics| boxed(format!("the module derives: {diagnostics:?}")))
}

pub(crate) fn boxed(message: String) -> Box<dyn std::error::Error> {
    message.into()
}

/// A registry serving the two published modules under the enrolled
/// publisher key.
pub(crate) fn serving_registry() -> TestResult<tempfile::TempDir> {
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
pub(crate) fn serving_registry_with(
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
pub(crate) fn opened(directory: &Path) -> Result<(RegistryStore, Record), Box<[pith_diag::Diag]>> {
    RegistryStore::open(directory.to_path_buf(), &pinned_key(), &Record::empty())
}

pub(crate) fn opened_ok(directory: &Path) -> TestResult<(RegistryStore, Record)> {
    opened(directory).map_err(|diagnostics| boxed(format!("the index verifies: {diagnostics:?}")))
}

pub(crate) fn opened_with_prior(
    directory: &Path,
    prior: &Record,
) -> Result<(RegistryStore, Record), Box<[pith_diag::Diag]>> {
    RegistryStore::open(directory.to_path_buf(), &pinned_key(), prior)
}

pub(crate) fn admitted_with_prior(
    directory: &Path,
    prior: &Record,
) -> TestResult<(RegistryStore, Record)> {
    opened_with_prior(directory, prior)
        .map_err(|diagnostics| boxed(format!("the index verifies: {diagnostics:?}")))
}

/// The workspace of a root served by the registry itself.
pub(crate) fn resolved_through(directory: &Path) -> Result<Workspace, Box<[pith_diag::Diag]>> {
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

pub(crate) fn engine() -> Engine {
    let mut engine = Engine::with_state_store(
        MemoryContentStore::default(),
        MemoryEngineStateStore::default(),
    );
    engine.register_frontend();
    engine
}

pub(crate) fn module_of<'workspace>(
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
pub(crate) struct Semantic {
    pub(crate) source_ids: Vec<pith_ids::ContentId>,
    pub(crate) abi: pith_ids::ModuleAbiDigest,
    pub(crate) surface: pith_ids::ContentId,
}

pub(crate) fn semantics(workspace: &Workspace) -> TestResult<BTreeMap<Box<str>, Semantic>> {
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

pub(crate) fn carries(diagnostics: &[pith_diag::Diag], code: FrontendCode) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == code.stable())
}
