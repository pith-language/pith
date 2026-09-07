//! The round trip: the same module bytes from a local directory and
//! through the signed index elaborate identically, with only the rendered
//! origin moving.

use crate::fixture::{
    DEP_MANIFEST, TestResult, module_of, resolved_through, semantics, serving_registry,
    staged_project,
};
use pith_loader::Workspace;

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
