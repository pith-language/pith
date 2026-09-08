//! The manifest grammar: clause shapes, sources, ranges, registry bindings,
//! and refusals.

use std::sync::Arc;

use pith_diag::{SourceFile, SourceId};
use pith_hir::{DependencySource, ManifestVersion, ModuleSubject, VersionBound, VersionRange};
use pith_loader::FrontendCode;

use super::{has_code, parse_err, parse_ok, subject};

#[test]
fn a_manifest_declares_its_subject_version_members_and_dependencies() {
    let manifest = parse_ok(
        "\nmodule example/hello 0.1.0\n\nworkspace { members: [\"modules/greeting\"] }\n\nuse \
         greeting = example/greeting from path \"modules/greeting\"\n",
    );
    let module = manifest
        .module
        .as_ref()
        .unwrap_or_else(|| unreachable!("parsed"));
    assert_eq!(module.subject, subject("example/hello"));
    assert_eq!(module.version.segments(), &[0, 1, 0]);
    assert_eq!(
        manifest
            .workspace
            .as_ref()
            .unwrap_or_else(|| unreachable!("parsed"))
            .members
            .iter()
            .map(|member| member.path.as_ref())
            .collect::<Vec<_>>(),
        ["modules/greeting"]
    );
    let [only] = manifest.uses.as_ref() else {
        unreachable!("one use clause");
    };
    assert_eq!(only.alias.as_ref(), "greeting");
    assert_eq!(only.subject, subject("example/greeting"));
    assert_eq!(only.path().map(|(path, _)| path), Some("modules/greeting"));
    let validated = manifest
        .manifest()
        .unwrap_or_else(|error| unreachable!("{error}"));
    assert_eq!(validated.subject(), &subject("example/hello"));
    assert_eq!(validated.uses().len(), 1);
}

#[test]
fn hyphenated_and_digit_segments_parse_as_one_identifier() {
    let manifest = parse_ok("module my-org/dep-2 1.0.0\n");
    let module = manifest
        .module
        .as_ref()
        .unwrap_or_else(|| unreachable!("parsed"));
    assert_eq!(module.subject, subject("my-org/dep-2"));
}

#[test]
fn a_subject_spelling_prints_and_reparses() {
    let parsed = subject("example/hello");
    assert_eq!(parsed.spelling().as_ref(), "example/hello");
    assert_eq!(subject(&parsed.spelling()), parsed);
}

#[test]
fn a_subject_outside_the_two_segment_grammar_is_refused() {
    for spelling in [
        "hello",
        "example/hello/extra",
        "Example/hello",
        "example/Hello",
        "1x/hello",
        "",
    ] {
        assert!(ModuleSubject::parse(spelling).is_err(), "{spelling} parsed");
    }
    assert!(has_code(
        &parse_err("module example/He_llo 0.1.0\n"),
        FrontendCode::InvalidSubject
    ));
    assert!(has_code(
        &parse_err("module hello 0.1.0\n"),
        FrontendCode::InvalidSubject
    ));
}

#[test]
fn a_version_outside_dotted_integers_is_refused() {
    assert!(has_code(
        &parse_err("module a/b 1.x.0\n"),
        FrontendCode::InvalidVersion
    ));
    assert!(has_code(
        &parse_err("module a/b 1..2\n"),
        FrontendCode::InvalidVersion
    ));
    assert!(has_code(
        &parse_err("module a/b v1\n"),
        FrontendCode::InvalidVersion
    ));
}

#[test]
fn source_clauses_in_a_manifest_and_manifest_clauses_in_a_source_are_refused() {
    assert!(has_code(
        &parse_err("module a/b 0.1.0\n\nnominal X = Text\n"),
        FrontendCode::WrongDocument
    ));
    assert!(has_code(
        &parse_err("module a/b 0.1.0\n\nimport dep\n"),
        FrontendCode::WrongDocument
    ));
    let source = Arc::new(SourceFile::new(
        SourceId::from_raw(0),
        "main.pi",
        "use greeting = example/greeting from path \"modules/greeting\"\n",
    ));
    let (_, diagnostics) = pith_syntax::parse(&source);
    assert!(has_code(&diagnostics, FrontendCode::WrongDocument));
}

#[test]
fn duplicate_singleton_clauses_and_duplicate_aliases_are_refused() {
    assert!(has_code(
        &parse_err("module a/b 0.1.0\nmodule c/d 1.0.0\n"),
        FrontendCode::DuplicateManifestClause
    ));
    assert!(has_code(
        &parse_err("module a/b 0.1.0\nworkspace { members: [] }\nworkspace { members: [] }\n"),
        FrontendCode::DuplicateManifestClause
    ));
    assert!(has_code(
        &parse_err("module a/b 0.1.0\nuse g = c/d from path \"c\"\nuse g = e/f from path \"e\"\n"),
        FrontendCode::DuplicateBinding
    ));
}

#[test]
fn a_manifest_without_a_module_clause_is_refused() {
    assert!(!parse_err("workspace { members: [] }\n").is_empty());
}

#[test]
fn the_four_dependency_sources_parse() {
    let manifest = parse_ok(
        "module a/b 0.1.0\n\
         use w = c/d\n\
         use x = c/e from path \"../e\"\n\
         use y = c/f from git \"https://example/f\" revision \"abc123\" subpath \"sub\"\n\
         use z = c/g from archive \"https://example/g.tar\" digest \"blake3:ff\"\n",
    );
    let kinds = manifest
        .uses
        .iter()
        .map(|use_| use_.source.kind())
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["registry", "path", "git", "archive"]);
    assert!(
        matches!(
            manifest.uses.first().map(|use_| &use_.source),
            Some(DependencySource::Registry { registry: None, .. })
        ),
        "a clause with no `from` takes the domain's configured route"
    );
}

/// Each written range spelling means one constructor. No caret or tilde: the
/// grammar refuses a sigil whose meaning a reader must look up.
#[test]
fn every_written_range_spelling_parses_to_its_constructor() {
    let version = |segments: &[u64]| {
        ManifestVersion::from_segments(segments.to_vec())
            .unwrap_or_else(|error| unreachable!("{error}"))
    };
    let range = |text: &str| {
        let manifest = parse_ok(&format!("module a/b 0.1.0\nuse d = c/d {text}\n"));
        manifest
            .uses
            .first()
            .map(|use_| use_.range.clone())
            .unwrap_or_else(|| unreachable!("the clause parses"))
    };
    assert_eq!(range("any"), VersionRange::Any);
    assert_eq!(range("1.2"), VersionRange::Exactly(version(&[1, 2])));
    assert_eq!(
        range(">= 1.2"),
        VersionRange::AtLeast(VersionBound {
            version: version(&[1, 2]),
            inclusive: true
        })
    );
    assert_eq!(
        range("> 1.2"),
        VersionRange::AtLeast(VersionBound {
            version: version(&[1, 2]),
            inclusive: false
        })
    );
    assert_eq!(
        range("<= 2.0"),
        VersionRange::AtMost(VersionBound {
            version: version(&[2]),
            inclusive: true
        })
    );
    assert_eq!(
        range("< 2.0"),
        VersionRange::AtMost(VersionBound {
            version: version(&[2]),
            inclusive: false
        })
    );
    assert_eq!(
        range(">= 1.2, < 2.0"),
        VersionRange::Between {
            lower: VersionBound {
                version: version(&[1, 2]),
                inclusive: true
            },
            upper: VersionBound {
                version: version(&[2]),
                inclusive: false
            }
        }
    );
    // A clause with no range admits every version the registry carries.
    assert_eq!(range(""), VersionRange::Any);
}

/// Version identity: two spellings that compare equal are one version. The
/// written segments survive for printing, so formatting never rewrites what
/// a person typed.
#[test]
fn two_spellings_of_one_version_are_one_version() {
    let parsed = |text: &str| {
        parse_ok(&format!("module a/b {text}\n"))
            .module
            .map(|module| module.version)
            .unwrap_or_else(|| unreachable!("the module clause parses"))
    };
    let short = parsed("1.2");
    let long = parsed("1.2.0.0");
    assert_eq!(
        short, long,
        "`1.2` and `1.2.0.0` are two spellings of one version"
    );
    assert_eq!(short.canonical(), long.canonical());
    assert_eq!(short.canonical_spelling(), long.canonical_spelling());
    assert_eq!(
        long.to_string(),
        "1.2.0.0",
        "the written spelling is retained"
    );
    assert_ne!(short, parsed("1.2.1"));
}

/// Registry bindings and domain routes are consumer configuration: they
/// parse in any manifest (a dependency is a root in its own checkout) but
/// carry authority only in a root's.
#[test]
fn registry_bindings_and_domain_routes_parse() {
    let manifest = parse_ok(
        "module a/b 0.1.0\n\
         registry main = \"https://git.example/registry\" root \"ed25519:AAAA\"\n\
         domain example from main\n\
         use d = example/d >= 1.0 from registry main\n",
    );
    let [registry] = manifest.registries.as_ref() else {
        unreachable!("one registry binding");
    };
    assert_eq!(registry.name.as_ref(), "main");
    assert_eq!(registry.locator.as_ref(), "https://git.example/registry");
    assert_eq!(registry.root_key.algorithm(), "ed25519");
    assert_eq!(registry.root_key.material(), "AAAA");
    let [route] = manifest.domains.as_ref() else {
        unreachable!("one domain route");
    };
    assert_eq!(route.domain.as_str(), "example");
    assert_eq!(route.registry.as_ref(), "main");
    assert!(matches!(
        manifest.uses.first().map(|use_| &use_.source),
        Some(DependencySource::Registry {
            registry: Some(_),
            ..
        })
    ));
}

/// Each source and range refusal the grammar places at parse, at its own code.
#[test]
fn the_parse_time_source_and_range_refusals_are_distinct() {
    assert!(
        has_code(
            &parse_err("module a/b 0.1.0\nuse g = c/d from carrier \"x\"\n"),
            FrontendCode::UnsupportedSource
        ),
        "a source kind outside the four is refused by name"
    );
    assert!(
        has_code(
            &parse_err("module a/b 0.1.0\nuse g = c/d from git \"https://example\"\n"),
            FrontendCode::IncompleteSource
        ),
        "a git route with no revision names no immutable content"
    );
    assert!(
        has_code(
            &parse_err("module a/b 0.1.0\nuse g = c/d from archive \"https://x.tar\"\n"),
            FrontendCode::IncompleteSource
        ),
        "an archive route with no digest has nothing to admit its bytes against"
    );
    assert!(
        has_code(
            &parse_err("module a/b 0.1.0\nuse g = c/d 1.0 from path \"d\"\n"),
            FrontendCode::RangeWithoutResolution
        ),
        "a path names one module, so a range selects nothing"
    );
    assert!(
        has_code(
            &parse_err("module a/b 0.1.0\nuse g = c/d >= 2.0, < 1.0\n"),
            FrontendCode::InvalidRange
        ),
        "an empty range admits no version at all"
    );
    assert!(
        has_code(
            &parse_err("module a/b 0.1.0\nregistry r = \"https://x\" root \"nocolon\"\n"),
            FrontendCode::InvalidRootKey
        ),
        "a root key names the algorithm that interprets it"
    );
    assert!(
        has_code(
            &parse_err("module a/b 0.1.0\nregistry r = \"https://x\"\n"),
            FrontendCode::InvalidRootKey
        ),
        "a registry binding with no pinned root key vouches for nothing"
    );
    assert!(
        has_code(
            &parse_err(
                "module a/b 0.1.0\n\
                 registry r = \"https://x\" root \"ed25519:A\"\n\
                 registry r = \"https://y\" root \"ed25519:B\"\n"
            ),
            FrontendCode::DuplicateRegistry
        ),
        "one name binds one registry"
    );
}

/// A registry-routed clause with a range and no path parses.
#[test]
fn unacquirable_sources_parse_and_refuse_at_load() {
    let manifest = parse_ok("module a/b 0.1.0\nuse g = c/d >= 1.0\n");
    assert!(
        manifest
            .uses
            .first()
            .is_some_and(|use_| use_.path().is_none())
    );
}

#[test]
fn empty_and_absolute_dependency_paths_are_refused() {
    assert!(has_code(
        &parse_err("module a/b 0.1.0\nuse g = c/d from path \"\"\n"),
        FrontendCode::InvalidPath
    ));
    assert!(has_code(
        &parse_err("module a/b 0.1.0\nuse g = c/d from path \"/abs\"\n"),
        FrontendCode::InvalidPath
    ));
}
