//! The project file's canonical spelling: what `pith fmt` writes, and its
//! round trips.

use pith_diag::SourceId;
use pith_loader::{ProjectSource, format_project_file};

use super::parse_ok;

#[test]
fn formatting_a_project_is_canonical_and_idempotent() {
    let source = ProjectSource::new(
        SourceId::from_raw(0),
        "pith.pi",
        "\nmodule   example/hello   0.1.0\n\ninputs  {\n  g=path \"../greeting\"\n}\n",
    );
    let once = format_project_file(&source).unwrap_or_else(|d| unreachable!("{d:?}"));
    assert_eq!(
        once,
        "module example/hello 0.1.0\n\ninputs {\n  g = path \"../greeting\",\n}\n"
    );
    let printed = ProjectSource::new(SourceId::from_raw(1), "pith.pi", once.clone());
    let twice = format_project_file(&printed).unwrap_or_else(|d| unreachable!("{d:?}"));
    assert_eq!(twice, once, "formatting formatted text moved it");
}

/// The printer's contract over every clause: printed text re-parses to the
/// same header, printing twice moves nothing, and a version keeps the
/// spelling it was written in.
#[test]
fn formatting_covers_every_clause_and_round_trips() {
    let written = "\n\
        module   example/hello   0.1.0\n\
        inputs  {\n\
        registry  main=\"https://git.example/registry\"  root  \"ed25519:AAAA\"\n\
        domain   example   from   main\n\
        a=example/one\n\
        b=example/two >=1.2,<2.0\n\
        c=example/three from registry main\n\
        d=path \"../four\"\n\
        e=git \"https://example/five\" at \"abc\" subpath \"s\"\n\
        f=archive \"https://example/six.tar\" digest \"blake3:ff\"\n\
        }\n\
        host wasm-component ./host/render.wasm\n\
        workspace { members: [ \"one\" ] }\n\
        include  \"rules.pi\"\n\
        nominal Payload = Text\n";
    let source = ProjectSource::new(SourceId::from_raw(0), "pith.pi", written);
    let once = format_project_file(&source).unwrap_or_else(|d| unreachable!("{d:?}"));
    assert_eq!(
        once,
        "module example/hello 0.1.0\n\
         \n\
         inputs {\n\
         \x20 registry main = \"https://git.example/registry\" root \"ed25519:AAAA\",\n\
         \x20 domain example from main,\n\
         \x20 a = example/one,\n\
         \x20 b = example/two >= 1.2, < 2.0,\n\
         \x20 c = example/three from registry main,\n\
         \x20 d = path \"../four\",\n\
         \x20 e = git \"https://example/five\" at \"abc\" subpath \"s\",\n\
         \x20 f = archive \"https://example/six.tar\" digest \"blake3:ff\",\n\
         }\n\
         \n\
         host wasm-component ./host/render.wasm\n\
         \n\
         workspace {\n\
         \x20 members: [\"one\"],\n\
         }\n\
         \n\
         include \"rules.pi\"\n\
         \n\
         nominal Payload = Text\n"
    );
    let printed = ProjectSource::new(SourceId::from_raw(1), "pith.pi", once.clone());
    let twice = format_project_file(&printed).unwrap_or_else(|d| unreachable!("{d:?}"));
    assert_eq!(twice, once, "formatting formatted text moved it");

    // The re-parse holds the same clauses, which an equal string alone does
    // not prove.
    let reparsed = parse_ok(&once);
    assert_eq!(reparsed.inputs.registries.len(), 1);
    assert_eq!(reparsed.inputs.domains.len(), 1);
    assert_eq!(reparsed.inputs.inputs.len(), 6);
    assert_eq!(reparsed.includes.len(), 1);
}

/// A trailing-zero version is one version with two spellings, and the
/// formatter writes back the one it was given rather than canonicalizing on
/// a person's behalf.
#[test]
fn formatting_retains_a_versions_written_spelling() {
    let source = ProjectSource::new(
        SourceId::from_raw(0),
        "pith.pi",
        "module a/b 1.2.0\n\ninputs {\n}\n",
    );
    let printed = format_project_file(&source).unwrap_or_else(|d| unreachable!("{d:?}"));
    assert_eq!(printed, "module a/b 1.2.0\n\ninputs {\n}\n");
}

#[test]
fn formatting_refuses_a_project_that_does_not_parse() {
    let source = ProjectSource::new(SourceId::from_raw(0), "pith.pi", "module a/b\n");
    assert!(format_project_file(&source).is_err());
}
