//! The manifest's canonical spelling: what `pith fmt` writes, and its round
//! trips.

use pith_diag::SourceId;
use pith_loader::{ManifestSource, format_manifest};

use super::parse_ok;

#[test]
fn formatting_a_manifest_is_canonical_and_idempotent() {
    let source = ManifestSource::new(
        SourceId::from_raw(0),
        "module.pi",
        "\nmodule   example/hello   0.1.0\n\nuse  g=example/greeting from path \"../greeting\"\n",
    );
    let once =
        format_manifest(&source).unwrap_or_else(|diagnostics| unreachable!("{diagnostics:?}"));
    assert_eq!(
        once,
        "module example/hello 0.1.0\n\nuse g = example/greeting from path \"../greeting\"\n"
    );
    let printed = ManifestSource::new(SourceId::from_raw(1), "module.pi", once.clone());
    let twice =
        format_manifest(&printed).unwrap_or_else(|diagnostics| unreachable!("{diagnostics:?}"));
    assert_eq!(twice, once, "formatting formatted text moved it");
}

/// The printer's contract over every clause: printed text re-parses to the
/// same manifest, printing twice moves nothing, and a version keeps the
/// spelling it was written in.
#[test]
fn formatting_covers_every_clause_and_round_trips() {
    let written = "\n\
        registry  main=\"https://git.example/registry\"  root  \"ed25519:AAAA\"\n\
        module   example/hello   0.1.0\n\
        domain   example   from   main\n\
        use  a=example/one\n\
        use  b=example/two >=1.2,<2.0\n\
        use  c=example/three from registry main\n\
        use  d=example/four from path \"../four\"\n\
        use  e=example/five from git \"https://example/five\" revision \"abc\" subpath \"s\"\n\
        use  f=example/six from archive \"https://example/six.tar\" digest \"blake3:ff\"\n";
    let source = ManifestSource::new(SourceId::from_raw(0), "module.pi", written);
    let once =
        format_manifest(&source).unwrap_or_else(|diagnostics| unreachable!("{diagnostics:?}"));
    assert_eq!(
        once,
        "registry main = \"https://git.example/registry\" root \"ed25519:AAAA\"\n\
         \n\
         module example/hello 0.1.0\n\
         \n\
         domain example from main\n\
         \n\
         use a = example/one\n\
         \n\
         use b = example/two >= 1.2, < 2.0\n\
         \n\
         use c = example/three from registry main\n\
         \n\
         use d = example/four from path \"../four\"\n\
         \n\
         use e = example/five from git \"https://example/five\" revision \"abc\" subpath \"s\"\n\
         \n\
         use f = example/six from archive \"https://example/six.tar\" digest \"blake3:ff\"\n"
    );
    let printed = ManifestSource::new(SourceId::from_raw(1), "module.pi", once.clone());
    let twice =
        format_manifest(&printed).unwrap_or_else(|diagnostics| unreachable!("{diagnostics:?}"));
    assert_eq!(twice, once, "formatting formatted text moved it");

    // The re-parse holds the same clauses, which an equal string alone does
    // not prove.
    let reparsed = parse_ok(&once);
    assert_eq!(reparsed.registries.len(), 1);
    assert_eq!(reparsed.domains.len(), 1);
    assert_eq!(reparsed.uses.len(), 6);
}

/// A trailing-zero version is one version with two spellings, and the
/// formatter writes back the one it was given rather than canonicalizing on
/// a person's behalf.
#[test]
fn formatting_retains_a_versions_written_spelling() {
    let source = ManifestSource::new(SourceId::from_raw(0), "module.pi", "module a/b 1.2.0\n");
    let printed =
        format_manifest(&source).unwrap_or_else(|diagnostics| unreachable!("{diagnostics:?}"));
    assert_eq!(printed, "module a/b 1.2.0\n");
}

#[test]
fn formatting_refuses_a_manifest_that_does_not_parse() {
    let source = ManifestSource::new(SourceId::from_raw(0), "module.pi", "module a/b\n");
    assert!(format_manifest(&source).is_err());
}
