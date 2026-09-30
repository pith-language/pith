use std::collections::BTreeMap;

use pith_ids::ModuleAbiDigest;
use pith_loader::{FrontendCode, Workspace};
use proptest::prelude::*;
use proptest::test_runner::TestCaseResult;

use super::model::Case;
use super::store::{Location, Reads};

pub fn accepted(case: &Case, workspace: &Workspace, reads: &Reads) -> TestCaseResult {
    let dependency = workspace
        .modules()
        .find(|module| module.subject().to_string() == "example/dep")
        .ok_or_else(|| TestCaseError::fail("selected dependency is absent"))?;
    let version = dependency.project().version().canonical_spelling();
    prop_assert_eq!(version.as_ref(), case.content.version.to_string());
    prop_assert!(
        dependency
            .files()
            .project()
            .text()
            .contains(case.content.source().as_str()),
        "the selected dependency's declarations are its project file's"
    );
    let canonical = Location::Source(case.canonical.clone());
    prop_assert_eq!(
        reads
            .projects
            .iter()
            .filter(|location| *location == &canonical)
            .count(),
        1
    );
    for (index, branch) in case.branches.iter().enumerate() {
        let consumer = Location::Branch(index, branch.depth);
        prop_assert_eq!(
            reads
                .located
                .iter()
                .filter(|(base, _)| base == &consumer)
                .count(),
            1
        );
    }
    Ok(())
}

pub fn semantics(
    workspace: &Workspace,
) -> Result<BTreeMap<String, ModuleAbiDigest>, TestCaseError> {
    let elaborated = workspace
        .elaborate()
        .map_err(|error| TestCaseError::fail(error.to_string()))?;
    Ok(elaborated
        .modules()
        .map(|module| (module.module().to_string(), module.abi_digest()))
        .collect())
}

pub fn refused(
    result: Result<Workspace, Box<[pith_diag::Diag]>>,
    code: FrontendCode,
) -> TestCaseResult {
    let diagnostics = result
        .err()
        .ok_or_else(|| TestCaseError::fail("invalid graph was accepted"))?;
    let matching: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == code.stable())
        .collect();
    prop_assert!(!matching.is_empty(), "wrong refusal: {diagnostics:?}");
    for diagnostic in matching {
        let source = diagnostic
            .source
            .as_ref()
            .ok_or_else(|| TestCaseError::fail("refusal lost its source"))?;
        let span = diagnostic.span;
        let text = source
            .source_text()
            .get(span.start.0 as usize..span.end.0 as usize)
            .ok_or_else(|| TestCaseError::fail("refusal span is outside its source"))?;
        prop_assert!(
            ["path", "git", "archive", "example/"]
                .iter()
                .any(|opener| text.starts_with(opener)),
            "refusal points at a locator, found {text:?}"
        );
    }
    Ok(())
}
