//! Acquisition is one path: the store a route names feeds the same
//! resolution, admission, elaboration, and frontend projection that the
//! local filesystem does.
//!
//! The fixture store here stands where the registry client arrives in a
//! later slice: it serves modules by subject from memory, which is the
//! registry's shape — a registry serves by subject because a `use` clause
//! selects one, and the route alone names nothing. Loading the same module
//! bytes through the local store and through this one must produce equal
//! semantic artifacts, with only the provenance differing, and a
//! registry-routed clause's written range reaches the admission decision
//! through the same path a local source's subject agreement does.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use common::{TestResult, file, module_of, semantics};
use pith_hir::FrontendCode;
use pith_loader::{
    AcquireFailure, AcquiredManifest, AcquiredSource, ModuleStore, Route, Workspace,
};

const ROOT_MANIFEST: &str =
    "module example/root 0.1.0\n\nuse dep = example/dep from path \"dep\"\n";
const DEP_MANIFEST: &str = "module example/dep 1.2.0\n";
const DEP_TYPES: &str = "nominal Message = Text\n";
const DEP_RULES: &str = "pure rule speak(who: Message) -> Text = { \"first\" }\n";
const ROOT_MAIN: &str = "import dep\n\nentry hello : Text = ask (Message(\"hello\"))\n";

/// The on-disk fixture: a root and one path dependency.
fn local_project(directory: &Path) -> TestResult {
    file(&directory.join("module.pi"), ROOT_MANIFEST)?;
    file(&directory.join("src/main.pi"), ROOT_MAIN)?;
    file(&directory.join("dep/module.pi"), DEP_MANIFEST)?;
    file(&directory.join("dep/src/types.pi"), DEP_TYPES)?;
    file(&directory.join("dep/src/rules.pi"), DEP_RULES)?;
    Ok(())
}

/// The root manifest with its dependency routed at a registry: the same
/// clause with the default source, and a range the release satisfies.
const REGISTRY_ROOT: &str = "module example/root 0.1.0\nregistry fixture = \"memory:fixture\" root \"ed25519:A\"\ndomain example from fixture\nuse dep = example/dep >= 1.2\n";

/// A store serving modules by subject, the registry's shape. The manifest
/// label carries the store's identity, which is the diagnostic file
/// identity a refused or unparsable module carries.
#[derive(Clone)]
struct RegistryFixture {
    modules: BTreeMap<Box<str>, FixtureModule>,
}

#[derive(Clone)]
struct FixtureModule {
    manifest: Box<str>,
    sources: Vec<(Box<str>, Box<str>)>,
}

/// A fixture location: the subject it serves. Two routes to one subject
/// produce one location, which is what lets resolution load the diamond
/// once without knowing what a registry is.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FixtureLocation(Box<str>);

impl std::fmt::Display for FixtureLocation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "registry-fixture/{}", self.0)
    }
}

impl RegistryFixture {
    /// The fixture carrying the same module bytes the local project holds,
    /// with the root routed at the registry.
    fn with_root_bytes() -> Self {
        Self {
            modules: BTreeMap::from([
                (
                    "example/root".into(),
                    FixtureModule {
                        manifest: REGISTRY_ROOT.into(),
                        sources: vec![("src/main.pi".into(), ROOT_MAIN.into())],
                    },
                ),
                (
                    "example/dep".into(),
                    FixtureModule {
                        manifest: DEP_MANIFEST.into(),
                        sources: vec![
                            ("src/types.pi".into(), DEP_TYPES.into()),
                            ("src/rules.pi".into(), DEP_RULES.into()),
                        ],
                    },
                ),
            ]),
        }
    }

    fn with_dep_version(version: &str) -> Self {
        let mut store = Self::with_root_bytes();
        if let Some(dep) = store.modules.get_mut("example/dep") {
            dep.manifest = format!("module example/dep {version}\n").into();
        }
        store
    }

    fn with_unparsable_dep() -> Self {
        let mut store = Self::with_root_bytes();
        if let Some(dep) = store.modules.get_mut("example/dep") {
            dep.manifest = "module example/dep 1.2.0\nmodule example/other 1.0.0\n".into();
        }
        store
    }

    fn root(&self) -> FixtureLocation {
        FixtureLocation("example/root".into())
    }
}

impl ModuleStore for RegistryFixture {
    type Location = FixtureLocation;

    fn locate(
        &self,
        _base: &Self::Location,
        route: &Route<'_>,
    ) -> Result<Self::Location, AcquireFailure> {
        let Route::Registry(registry) = route else {
            return Err(AcquireFailure::Unsupported { kind: route.kind() });
        };
        assert_eq!(registry.locator(), "memory:fixture");
        assert_eq!(registry.root_key().material(), "A");
        let subject = registry.subject();
        if self.modules.contains_key(subject.spelling().as_ref()) {
            Ok(FixtureLocation(subject.spelling()))
        } else {
            Err(AcquireFailure::Unreadable {
                message: format!("the index carries no release of {subject}").into(),
            })
        }
    }

    fn manifest(&mut self, location: &Self::Location) -> Result<AcquiredManifest, AcquireFailure> {
        let module =
            self.modules
                .get(location.0.as_ref())
                .ok_or_else(|| AcquireFailure::Unreadable {
                    message: format!("the index carries no release at {location}").into(),
                })?;
        Ok(AcquiredManifest {
            label: format!("{location}/module.pi").into(),
            text: module.manifest.clone(),
        })
    }

    fn sources(
        &mut self,
        location: &Self::Location,
    ) -> Result<Vec<AcquiredSource>, AcquireFailure> {
        let module =
            self.modules
                .get(location.0.as_ref())
                .ok_or_else(|| AcquireFailure::Unreadable {
                    message: format!("the index carries no release at {location}").into(),
                })?;
        Ok(module
            .sources
            .iter()
            .map(|(path, text)| AcquiredSource {
                path: path.clone(),
                text: text.clone(),
            })
            .collect())
    }
}

#[test]
fn the_same_module_bytes_through_two_stores_produce_equal_semantic_artifacts() -> TestResult {
    let root = tempfile::tempdir()?;
    local_project(root.path())?;
    let local = Workspace::load(&root.path().join("module.pi"))
        .map_err(|diagnostics| format!("the local project resolves: {diagnostics:?}"))?;

    let store = RegistryFixture::with_root_bytes();
    let root = store.root();
    let acquired = Workspace::resolve(store, &root)
        .map_err(|diagnostics| format!("the registry project resolves: {diagnostics:?}"))?;

    let local_semantics = semantics(&local)?;
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

    // Provenance is the one thing that moved, and it is not a semantic
    // key: the same module carries each store's location as its origin.
    let local_dep = module_of(&local, "example/dep");
    let acquired_dep = module_of(&acquired, "example/dep");
    assert_ne!(local_dep.origin(), acquired_dep.origin());
    assert!(acquired_dep.origin().contains("registry-fixture"));
    Ok(())
}

#[test]
fn a_registry_routed_range_reaches_the_admission_decision() -> TestResult {
    let admitted = RegistryFixture::with_dep_version("1.2.0");
    let admitted_root = admitted.root();
    let workspace = Workspace::resolve(admitted, &admitted_root).map_err(|diagnostics| {
        format!("a release inside the written range admits: {diagnostics:?}")
    })?;
    assert!(module_of(&workspace, "example/dep").sources().files().len() == 2);

    // A release outside the written range is refused at acquisition, by
    // the same admission either source kind passes, naming both sides.
    let mut refused = RegistryFixture::with_dep_version("2.0.0");
    refused.modules.insert(
        "example/root".into(),
        FixtureModule {
            manifest: REGISTRY_ROOT.replace(">= 1.2", "< 2").into(),
            sources: vec![("src/main.pi".into(), ROOT_MAIN.into())],
        },
    );
    let refused_root = refused.root();
    let diagnostics = match Workspace::resolve(refused, &refused_root) {
        Ok(_) => unreachable!("a release outside the written range is refused"),
        Err(diagnostics) => diagnostics,
    };
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == FrontendCode::UnexpectedSubject.stable()),
        "the refusal lands at acquisition: {diagnostics:?}"
    );
    Ok(())
}

#[test]
fn a_registry_served_manifest_carries_the_stores_diagnostic_identity() -> TestResult {
    let store = RegistryFixture::with_unparsable_dep();
    let root = store.root();
    let diagnostics = match Workspace::resolve(store, &root) {
        Ok(_) => unreachable!("a manifest that does not parse refuses"),
        Err(diagnostics) => diagnostics,
    };
    let unparsable = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == FrontendCode::DuplicateManifestClause.stable())
        .unwrap_or_else(|| {
            unreachable!("the clause-level parse refusal is reported: {diagnostics:?}")
        });
    let label = unparsable
        .source
        .as_ref()
        .map(|source| source.label.as_ref().to_string())
        .unwrap_or_default();
    assert!(
        label.contains("registry-fixture/example/dep"),
        "the diagnostic names the file the store served, not a path: {label}"
    );
    Ok(())
}
