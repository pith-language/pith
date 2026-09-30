//! The project file's grammar: the header clauses, the inputs block and
//! its locators, workspace membership, includes, wrong-document refusals,
//! and the canonical spelling `pith fmt` writes. [`grammar`] pins the
//! refusals, [`formatting`] the canonical spelling.

mod formatting;
mod grammar;

use pith_diag::{Diag, SourceId};
use pith_hir::{ModuleSubject, ProjectHeader};
use pith_loader::{FrontendCode, ProjectSource, parse_project_file};

fn parse(text: &str) -> (ProjectHeader, Vec<Diag>) {
    let file = parse_project_file(&ProjectSource::new(
        SourceId::from_raw(0),
        "pith.pi",
        text.trim_start_matches('\n'),
    ));
    (file.header().clone(), file.diagnostics().to_vec())
}

fn parse_ok(text: &str) -> ProjectHeader {
    let (header, diagnostics) = parse(text);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    header
}

fn parse_err(text: &str) -> Vec<Diag> {
    let (_, diagnostics) = parse(text);
    assert!(!diagnostics.is_empty(), "the project refuses this text");
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
