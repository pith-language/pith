//! The acquisition pipeline's refusals: a wrong cache, a wrong tree, a
//! withdrawn release, and an unreachable index.

use std::fs;

use crate::fixture::{
    TestResult, carries, pinned_key, publisher, resolved_through, serving_registry,
};
use pith_hir::{FrontendCode, ModuleSubject};
use pith_loader::registry::{Host, Record, RegistryStore};

#[test]
fn an_entry_whose_cached_fields_disagree_with_the_pinned_revision_refuses() -> TestResult {
    let registry = serving_registry()?;
    // Replace the stored manifest at the revision the entry pins: the
    // index is untouched, so the checks pass and the disagreement is
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
        .join("pith.pi");
    fs::write(&stored, "module example/dep 2.0.0\n\ninputs {\n}\n")?;
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
        .join("rules.pi");
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
        pith_hir::ModuleVersion::parse("1.2.0")?,
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
