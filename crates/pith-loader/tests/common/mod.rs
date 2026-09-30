//! Helpers shared by the loader's integration suites: fixture files, the
//! frontend engine, and the per-module semantic identity that
//! store-equivalence claims compare by.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use pith_engine::{Engine, MemoryEngineStateStore};
use pith_loader::{
    ElaboratedWorkspace, FrontendProjection, RegisterFrontend, ResolvedModule, Workspace,
};
use pith_store::MemoryContentStore;

pub(crate) type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

pub(crate) fn file(path: &Path, text: &str) -> TestResult {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
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

/// One module's semantic identity through the whole pipeline: its source
/// content identities, its elaborated ABI, and its published surface.
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
        let mut source_ids = std::iter::once(module.files().project())
            .chain(module.files().includes().files())
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
