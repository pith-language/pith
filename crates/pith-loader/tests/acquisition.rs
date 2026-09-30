//! Acquisition is one path: the store a route names feeds the same
//! resolution, admission, and elaboration the local filesystem does. The
//! fixture store stands in for the registry client; equal module bytes
//! through both stores must produce equal semantic artifacts, provenance
//! aside.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use common::{TestResult, file, module_of, semantics};
use pith_hir::FrontendCode;
use pith_loader::{
    AcquireFailure, AcquiredProject, AcquiredSource, PROJECT_NAME, ProjectStore, Route, Workspace,
};

const ROOT_PROJECT: &str = "module example/root 0.1.0\n\ninputs {\n  dep = path \"dep\"\n}\n\nimport dep\n\nentry      hello : Text = ask (Message(\"hello\"))\n";
const DEP_PROJECT: &str =
    "module example/dep 1.2.0\n\ninputs {\n}\n\ninclude \"types.pi\"\ninclude \"rules.pi\"\n";
const DEP_TYPES: &str = "nominal Message = Text\n";
const DEP_RULES: &str = "pure rule speak(who: Message) -> Text = { \"first\" }\n";
/// The on-disk fixture: a root and one path dependency.
fn local_project(directory: &Path) -> TestResult {
    file(&directory.join(PROJECT_NAME), ROOT_PROJECT)?;
    file(&directory.join("dep").join(PROJECT_NAME), DEP_PROJECT)?;
    file(&directory.join("dep/types.pi"), DEP_TYPES)?;
    file(&directory.join("dep/rules.pi"), DEP_RULES)?;
    Ok(())
}

/// The root project with its dependency routed at a registry: registry
/// locator, and a range the release satisfies.
const REGISTRY_ROOT: &str = "module example/root 0.1.0\n\ninputs {\n  registry fixture = \"memory:fixture\" root \"ed25519:A\"\n  domain example from fixture\n  dep = example/dep >= 1.2\n}\n\nimport dep\n\nentry hello : Text = ask (Message(\"hello\"))\n";

/// A store serving modules by subject, the registry's shape. Manifest
/// labels carry the store's identity, which is the diagnostic identity a
/// refused or unparsable module reports.
#[derive(Clone)]
struct RegistryFixture {
    modules: BTreeMap<Box<str>, FixtureModule>,
}

#[derive(Clone)]
struct FixtureModule {
    project: Box<str>,
    includes: Vec<(Box<str>, Box<str>)>,
}

/// A fixture location: the subject it serves. Two routes to one subject
/// produce one location, so a shared module loads once.
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
                        project: REGISTRY_ROOT.into(),
                        includes: Vec::new(),
                    },
                ),
                (
                    "example/dep".into(),
                    FixtureModule {
                        project: DEP_PROJECT.into(),
                        includes: vec![
                            ("types.pi".into(), DEP_TYPES.into()),
                            ("rules.pi".into(), DEP_RULES.into()),
                        ],
                    },
                ),
            ]),
        }
    }

    fn with_dep_version(version: &str) -> Self {
        let mut store = Self::with_root_bytes();
        if let Some(dep) = store.modules.get_mut("example/dep") {
            dep.project = format!(
                "module example/dep {version}\n\ninputs {{\n}}\n\ninclude \"types.pi\"\ninclude \
                 \"rules.pi\"\n"
            )
            .into();
        }
        store
    }

    fn with_unparsable_dep() -> Self {
        let mut store = Self::with_root_bytes();
        if let Some(dep) = store.modules.get_mut("example/dep") {
            dep.project = "module example/dep 1.2.0\nmodule example/other 1.0.0\n".into();
        }
        store
    }

    fn root(&self) -> FixtureLocation {
        FixtureLocation("example/root".into())
    }
}

impl ProjectStore for RegistryFixture {
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

    fn project(&mut self, location: &Self::Location) -> Result<AcquiredProject, AcquireFailure> {
        let module =
            self.modules
                .get(location.0.as_ref())
                .ok_or_else(|| AcquireFailure::Unreadable {
                    message: format!("the index carries no release at {location}").into(),
                })?;
        Ok(AcquiredProject {
            label: format!("{location}/{PROJECT_NAME}").into(),
            text: module.project.clone(),
        })
    }

    fn include(
        &mut self,
        location: &Self::Location,
        path: &str,
    ) -> Result<AcquiredSource, AcquireFailure> {
        let module =
            self.modules
                .get(location.0.as_ref())
                .ok_or_else(|| AcquireFailure::Unreadable {
                    message: format!("the index carries no release at {location}").into(),
                })?;
        module
            .includes
            .iter()
            .find(|(include, _)| include.as_ref() == path)
            .map(|(path, text)| AcquiredSource {
                path: path.clone(),
                text: text.clone(),
            })
            .ok_or_else(|| AcquireFailure::Unreadable {
                message: format!("the release at {location} names no include {path}").into(),
            })
    }
}

#[test]
fn the_same_module_bytes_through_two_stores_produce_equal_semantic_artifacts() -> TestResult {
    let root = tempfile::tempdir()?;
    local_project(root.path())?;
    let local = Workspace::load(&root.path().join(PROJECT_NAME))
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
        // The two roots spell their locators differently, so their project
        // files differ as content; the dependency's bytes are the same
        // through both stores, and every module's semantics are.
        if subject.as_ref() == "example/dep" {
            assert_eq!(
                local.source_ids, through_registry.source_ids,
                "{subject}'s file identities are store-independent"
            );
        }
        assert_eq!(
            local.abi, through_registry.abi,
            "{subject}'s ABI is store-independent"
        );
        assert_eq!(
            local.surface, through_registry.surface,
            "{subject}'s published surface is store-independent"
        );
    }

    // Provenance differs and is not a semantic key: each store's location
    // becomes the module's origin.
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
    assert!(
        module_of(&workspace, "example/dep")
            .files()
            .includes()
            .files()
            .len()
            == 2
    );

    // A release outside the written range is refused at acquisition, by
    // the same admission either source kind passes, naming both sides.
    let mut refused = RegistryFixture::with_dep_version("2.0.0");
    refused.modules.insert(
        "example/root".into(),
        FixtureModule {
            project: REGISTRY_ROOT.replace(">= 1.2", "< 2").into(),
            includes: Vec::new(),
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
fn a_registry_served_project_carries_the_stores_diagnostic_identity() -> TestResult {
    let store = RegistryFixture::with_unparsable_dep();
    let root = store.root();
    let diagnostics = match Workspace::resolve(store, &root) {
        Ok(_) => unreachable!("a manifest that does not parse refuses"),
        Err(diagnostics) => diagnostics,
    };
    let unparsable = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == FrontendCode::DuplicateClause.stable())
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
