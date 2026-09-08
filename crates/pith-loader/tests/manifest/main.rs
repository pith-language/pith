//! The manifest document's grammar: subjects, versions, workspaces, the
//! dependency sources and their ranges, registry bindings and domain routes,
//! wrong-document refusals, and the canonical spelling `pith fmt` writes.
//! [`grammar`] pins the refusals, [`formatting`] the canonical spelling.

mod formatting;
mod grammar;

use pith_diag::{Diag, SourceId};
use pith_hir::{ModuleSubject, ParsedManifest};
use pith_loader::{FrontendCode, ManifestSource, parse_manifest};

fn parse(text: &str) -> (ParsedManifest, Vec<Diag>) {
    let file = parse_manifest(&ManifestSource::new(
        SourceId::from_raw(0),
        "module.pi",
        text.trim_start_matches('\n'),
    ));
    (file.manifest().clone(), file.diagnostics().to_vec())
}

fn parse_ok(text: &str) -> ParsedManifest {
    let (manifest, diagnostics) = parse(text);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    manifest
}

fn parse_err(text: &str) -> Vec<Diag> {
    let (_, diagnostics) = parse(text);
    assert!(!diagnostics.is_empty(), "the manifest refuses this text");
    diagnostics
}

fn has_code(diagnostics: &[Diag], code: FrontendCode) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == code.stable())
}

fn subject(spelling: &str) -> ModuleSubject {
    ModuleSubject::parse(spelling).unwrap_or_else(|error| unreachable!("{error}"))
}
