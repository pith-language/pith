//! The manifest document's grammar: subjects, versions, workspaces, the four
//! dependency sources and their ranges, registry bindings and domain routes,
//! the wrong-document refusals in both directions, and the canonical
//! spelling `pith fmt` writes for a manifest.
//!
//! Every spelling 0069 and 0070 write out appears here as a fixture, because
//! both records fix a grammar before the code that consumes it exists, and a
//! record's example that does not parse is a record nobody can implement
//! against. The grammar and its refusals are [`grammar`]; the canonical
//! spelling is [`formatting`]. Both share the parse helpers here.

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
