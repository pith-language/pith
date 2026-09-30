//! The project header's grammar: clause shapes, locators, ranges, registry
//! bindings and domain routes, and refusals.

use std::sync::Arc;

use pith_diag::{SourceFile, SourceId};
use pith_hir::{InputLocator, ModuleVersion, VersionBound, VersionRange};
use pith_loader::FrontendCode;

use super::{has_code, parse_err, parse_ok, subject};

#[test]
fn a_project_declares_its_subject_version_members_and_includes() {
    let header = parse_ok(
        "\nmodule example/hello 0.1.0\n\ninputs {\n  greeting = path \"modules/greeting\"\n}\n\n\
         workspace { members: [\"modules/greeting\"] }\n\ninclude \"rules.pi\"\n",
    );
    let module = header
        .module
        .as_ref()
        .unwrap_or_else(|| unreachable!("parsed"));
    assert_eq!(module.subject, subject("example/hello"));
    assert_eq!(module.version.segments(), &[0, 1, 0]);
    assert_eq!(
        header
            .workspace
            .as_ref()
            .unwrap_or_else(|| unreachable!("parsed"))
            .members
            .iter()
            .map(|member| member.path.as_ref())
            .collect::<Vec<_>>(),
        ["modules/greeting"]
    );
    let [only] = header.inputs.inputs.as_ref() else {
        unreachable!("one input");
    };
    assert_eq!(only.name.as_ref(), "greeting");
    assert_eq!(
        only.locator.path().map(|(path, _)| path),
        Some("modules/greeting")
    );
    let [include] = header.includes.as_ref() else {
        unreachable!("one include");
    };
    assert_eq!(include.path.as_ref(), "rules.pi");
    let validated =
        pith_hir::Project::from_header(&header).unwrap_or_else(|error| unreachable!("{error}"));
    assert_eq!(validated.subject(), &subject("example/hello"));
    assert_eq!(validated.inputs().len(), 1);
}

#[test]
fn hyphenated_and_digit_segments_parse_as_one_identifier() {
    let header = parse_ok("module my-org/dep-2 1.0.0\n\ninputs {\n}\n");
    let module = header
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
        assert!(
            pith_hir::ModuleSubject::parse(spelling).is_err(),
            "{spelling} parsed"
        );
    }
    assert!(has_code(
        &parse_err("module example/He_llo 0.1.0\n\ninputs {\n}\n"),
        FrontendCode::InvalidSubject
    ));
    assert!(has_code(
        &parse_err("module hello 0.1.0\n\ninputs {\n}\n"),
        FrontendCode::InvalidSubject
    ));
}

#[test]
fn a_version_outside_dotted_integers_is_refused() {
    assert!(has_code(
        &parse_err("module a/b 1.x.0\n\ninputs {\n}\n"),
        FrontendCode::InvalidVersion
    ));
    assert!(has_code(
        &parse_err("module a/b 1..2\n\ninputs {\n}\n"),
        FrontendCode::InvalidVersion
    ));
    assert!(has_code(
        &parse_err("module a/b v1\n\ninputs {\n}\n"),
        FrontendCode::InvalidVersion
    ));
}

/// A project file holds its header and then its declarations; an included
/// file holds declarations only. Both sides of the boundary refuse what
/// crosses it.
#[test]
fn header_clauses_in_an_include_and_declarations_before_the_header_are_refused() {
    let include = Arc::new(SourceFile::new(
        SourceId::from_raw(0),
        "rules.pi",
        "module example/greeting 0.1.0\n",
    ));
    let (_, diagnostics) = pith_syntax::parse(&include);
    assert!(
        super::has_code(&diagnostics, FrontendCode::WrongDocument),
        "{diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.0.contains("`module`")),
        "the refusal names the clause: {diagnostics:?}"
    );

    let include = Arc::new(SourceFile::new(
        SourceId::from_raw(0),
        "rules.pi",
        "inputs {\n}\n",
    ));
    let (_, diagnostics) = pith_syntax::parse(&include);
    assert!(
        super::has_code(&diagnostics, FrontendCode::WrongDocument),
        "{diagnostics:?}"
    );

    let (header, diagnostics) = super::parse("nominal X = Text\n\ninputs {\n}\n");
    assert!(
        has_code(&diagnostics, FrontendCode::WrongDocument),
        "a clause after the declarations is refused: {diagnostics:?}"
    );
    assert!(header.inputs.inputs.is_empty());
}

#[test]
fn the_header_holds_its_clauses_in_order() {
    assert!(
        has_code(
            &parse_err("inputs {\n}\n\nmodule a/b 0.1.0\n"),
            FrontendCode::WrongDocument
        ),
        "the `module` clause opens the header"
    );
    assert!(
        has_code(
            &parse_err("module a/b 0.1.0\n\ninclude \"a.pi\"\n\ninputs {\n}\n"),
            FrontendCode::WrongDocument
        ),
        "an include follows the header clauses before it"
    );
}

#[test]
fn duplicate_singleton_clauses_and_duplicate_names_are_refused() {
    assert!(has_code(
        &parse_err("module a/b 0.1.0\nmodule c/d 1.0.0\n\ninputs {\n}\n"),
        FrontendCode::DuplicateClause
    ));
    assert!(has_code(
        &parse_err(
            "module a/b 0.1.0\n\ninputs {\n}\n\nworkspace { members: [] }\n\nworkspace { \
             members: [] }\n"
        ),
        FrontendCode::DuplicateClause
    ));
    assert!(has_code(
        &parse_err("module a/b 0.1.0\n\ninputs {\n  g = path \"c\"\n  g = path \"e\"\n}\n"),
        FrontendCode::DuplicateBinding
    ));
}

#[test]
fn a_project_file_without_an_inputs_block_is_refused() {
    assert!(!parse_err("module a/b 0.1.0\n").is_empty());
    assert!(!parse_err("workspace { members: [] }\n").is_empty());
}

#[test]
fn the_four_locators_parse() {
    let header = parse_ok(
        "module a/b 0.1.0\n\ninputs {\n\
         w = c/d\n\
         x = path \"../e\"\n\
         y = git \"https://example/f\" at \"abc123\" subpath \"sub\"\n\
         z = archive \"https://example/g.tar\" digest \"blake3:ff\"\n\
         }\n",
    );
    let kinds = header
        .inputs
        .inputs
        .iter()
        .map(|input| input.locator.kind())
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["registry", "path", "git", "archive"]);
    assert!(
        matches!(
            header.inputs.inputs.first().map(|input| &input.locator),
            Some(InputLocator::Registry { registry: None, .. })
        ),
        "a subject with no `from registry` takes the domain's configured route"
    );
}

/// Each written range spelling means one constructor. No caret or tilde: the
/// grammar refuses a sigil whose meaning a reader must look up.
#[test]
fn every_written_range_spelling_parses_to_its_constructor() {
    let version = |segments: &[u64]| {
        ModuleVersion::from_segments(segments.to_vec())
            .unwrap_or_else(|error| unreachable!("{error}"))
    };
    let range = |text: &str| {
        let header = parse_ok(&format!(
            "module a/b 0.1.0\n\ninputs {{\n  d = c/d {text}\n}}\n"
        ));
        header
            .inputs
            .inputs
            .first()
            .map(|input| match &input.locator {
                InputLocator::Registry { range, .. } => range.clone(),
                _ => unreachable!("a subject names the registry route"),
            })
            .unwrap_or_else(|| unreachable!("the entry parses"))
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
    assert_eq!(range(""), VersionRange::Any);
}

/// Version identity: two spellings that compare equal are one version. The
/// written segments survive for printing, so formatting never rewrites what
/// a person typed.
#[test]
fn two_spellings_of_one_version_are_one_version() {
    let parsed = |text: &str| {
        parse_ok(&format!("module a/b {text}\n\ninputs {{\n}}\n"))
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
/// parse in any project (a dependency is a root in its own checkout) but
/// carry authority only in a root's.
#[test]
fn registry_bindings_and_domain_routes_parse() {
    let header = parse_ok(
        "module a/b 0.1.0\n\ninputs {\n\
         registry main = \"https://git.example/registry\" root \"ed25519:AAAA\"\n\
         domain example from main\n\
         d = example/d >= 1.0 from registry main\n\
         }\n",
    );
    let [registry] = header.inputs.registries.as_ref() else {
        unreachable!("one registry binding");
    };
    assert_eq!(registry.name.as_ref(), "main");
    assert_eq!(registry.locator.as_ref(), "https://git.example/registry");
    assert_eq!(registry.root_key.algorithm(), "ed25519");
    assert_eq!(registry.root_key.material(), "AAAA");
    let [route] = header.inputs.domains.as_ref() else {
        unreachable!("one domain route");
    };
    assert_eq!(route.domain.as_str(), "example");
    assert_eq!(route.registry.as_ref(), "main");
    assert!(matches!(
        header.inputs.inputs.first().map(|input| &input.locator),
        Some(InputLocator::Registry {
            registry: Some(_),
            ..
        })
    ));
}

/// Each locator and range refusal the grammar places at parse, at its own
/// code.
#[test]
fn the_parse_time_locator_and_range_refusals_are_distinct() {
    assert!(
        has_code(
            &parse_err("module a/b 0.1.0\n\ninputs {\n  g = carrier \"x\"\n}\n"),
            FrontendCode::InvalidSubject
        ),
        "a locator is path, git, archive, or a subject, and `carrier` is none"
    );
    assert!(
        has_code(
            &parse_err("module a/b 0.1.0\n\ninputs {\n  g = git \"https://example\"\n}\n"),
            FrontendCode::IncompleteSource
        ),
        "a git locator with no revision names no immutable content"
    );
    assert!(
        has_code(
            &parse_err("module a/b 0.1.0\n\ninputs {\n  g = archive \"https://x.tar\"\n}\n"),
            FrontendCode::IncompleteSource
        ),
        "an archive locator with no digest has nothing to admit its bytes against"
    );
    assert!(
        has_code(
            &parse_err("module a/b 0.1.0\n\ninputs {\n  g = c/d >= 2.0, < 1.0\n}\n"),
            FrontendCode::InvalidRange
        ),
        "an empty range admits no version at all"
    );
    assert!(
        has_code(
            &parse_err(
                "module a/b 0.1.0\n\ninputs {\n  registry r = \"https://x\" root \"nocolon\"\n}\n"
            ),
            FrontendCode::InvalidRootKey
        ),
        "a root key names the algorithm that interprets it"
    );
    assert!(
        has_code(
            &parse_err("module a/b 0.1.0\n\ninputs {\n  registry r = \"https://x\"\n}\n"),
            FrontendCode::InvalidRootKey
        ),
        "a registry binding with no pinned root key vouches for nothing"
    );
    assert!(
        has_code(
            &parse_err(
                "module a/b 0.1.0\n\ninputs {\n\
                 registry r = \"https://x\" root \"ed25519:A\"\n\
                 registry r = \"https://y\" root \"ed25519:B\"\n\
                 }\n"
            ),
            FrontendCode::DuplicateRegistry
        ),
        "one name binds one registry"
    );
}

/// The later-slice forms parse to their extent and are refused, naming what
/// is missing.
#[test]
fn typed_values_with_arguments_and_grants_parse_then_refuse() {
    assert!(
        has_code(
            &parse_err("module a/b 0.1.0\n\ninputs {\n  style : (Text) -> Text = (t) -> t\n}\n"),
            FrontendCode::UnsupportedInput
        ),
        "a typed value input is parsed to its annotation and refused"
    );
    assert!(
        has_code(
            &parse_err("module a/b 0.1.0\n\ninputs {\n  d = path \"../d\" with { style = 1 }\n}\n"),
            FrontendCode::UnsupportedInput
        ),
        "`with` arguments are parsed to their extent and refused"
    );
    assert!(
        has_code(
            &parse_err(
                "module a/b 0.1.0\n\ninputs {\n  d = path \"../d\" grant { network: deny }\n}\n"
            ),
            FrontendCode::UnsupportedInput
        ),
        "a grant is parsed to its extent and refused"
    );
}

#[test]
fn the_host_clause_parses_and_names_its_adapter_and_artifact() {
    let header = parse_ok("module a/b 0.1.0\n\ninputs {\n}\n\nhost wasm-component ./host/a.wasm\n");
    let host = header
        .host
        .as_ref()
        .unwrap_or_else(|| unreachable!("parsed"));
    assert_eq!(host.adapter.as_ref(), "wasm-component");
    assert_eq!(host.artifact.as_ref(), "./host/a.wasm");
}

#[test]
fn a_registry_locator_with_a_range_parses() {
    let header = parse_ok("module a/b 0.1.0\n\ninputs {\n  g = c/d >= 1.0\n}\n");
    assert!(
        header
            .inputs
            .inputs
            .first()
            .is_some_and(|input| input.locator.resolves_versions())
    );
}

#[test]
fn empty_and_absolute_paths_are_refused() {
    assert!(has_code(
        &parse_err("module a/b 0.1.0\n\ninputs {\n  g = path \"\"\n}\n"),
        FrontendCode::InvalidPath
    ));
    assert!(has_code(
        &parse_err("module a/b 0.1.0\n\ninputs {\n  g = path \"/abs\"\n}\n"),
        FrontendCode::InvalidPath
    ));
    assert!(has_code(
        &parse_err("module a/b 0.1.0\n\ninclude \"/abs.pi\"\n"),
        FrontendCode::InvalidPath
    ));
    assert!(has_code(
        &parse_err("module a/b 0.1.0\n\ninclude \"./a.pi\"\n"),
        FrontendCode::InvalidPath
    ));
    assert!(has_code(
        &parse_err("module a/b 0.1.0\n\ninclude \"a.pi\"\ninclude \"a.pi\"\n"),
        FrontendCode::DuplicateBinding
    ));
}
