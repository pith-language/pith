//! Formatter tests for digest preservation, idempotence, and write behavior.

use std::path::{Path, PathBuf};

use pith_diag::SourceId;
use pith_loader::{ImportEnv, LoadedModule, ModuleSource, format_module, load_module};
use pith_output::dto::FmtStatus;
use pith_query::{FormatMode, format};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// The corpus this file formats. One corpus module is deliberately absent:
/// the independent-evidence domain, which no other crate may name.
const CORPUS: &[(&str, &str)] = &[
    ("xylem", "../../crates/xylem/xylem.pi"),
    ("stele", "../../crates/stele/stele.pi"),
    ("phloem", "../../crates/phloem/phloem.pi"),
];

/// The written-body tier: every request construct, the control forms, and an
/// entry, which the corpus's host-tier modules cannot exercise. Spellings
/// the notation suite already elaborates.
const NOTATION: &str = "sum Shape = circle(Int) | square

nominal Object = Blob

pure rule \"make-shape\"(n: Int) -> Shape = {
  if n == 0 { square() } else { circle(n * 2 - 1) }
}

pure rule area(s: Shape) -> Int = {
  match s {
    circle(radius) { radius * 3 }
    square { 14 }
  }
}

pure rule described(s: Shape) -> Text = {
  match s {
    circle(radius) { concat(describe(radius), \" sides\") }
    square { \"four sides\" }
  }
}

pure rule \"sum-of\"(xs: List<Int>) -> Int = {
  fold xs from 0 { (element, accumulator) -> element + accumulator }
}

pure rule one(n: Int) -> Text = {
  let doubled = ask Int (n)
  describe(doubled)
}

pure rule batch(flag: Bool) -> Text = {
  let (doubled, zero) = ask all (ask Int (7), ask Bool (flag))
  if zero { describe(doubled) } else { \"nonzero\" }
}

pure rule \"fan-out\"(ns: List<Int>) -> List<Int> = {
  ask all Int [ for n in ns { } (n) ]
}

pure rule materialized(o: Object) -> Text = {
  let raw = bytes of unwrap(o)
  decode(raw)
}

entry size : Text = ask (\"four sides\")
";

fn workspace_file(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn source(module: &str, label: &str, text: impl Into<Box<str>>) -> ModuleSource {
    ModuleSource::new(module, SourceId::from_raw(0), label, text)
}

fn load(module: &str, label: &str, text: &str, imports: &ImportEnv) -> TestResult<LoadedModule> {
    load_module(&source(module, label, text), imports)
        .map_err(|diagnostics| format!("`{module}` does not elaborate: {diagnostics:?}").into())
}

/// What a module's digests are: the ABI digest and each represented rule's
/// body digest, paired with the rule's label so a move is named.
fn digests(module: &str, label: &str, text: &str, imports: &ImportEnv) -> TestResult<Vec<String>> {
    let loaded = load(module, label, text, imports)?;
    let mut digests = vec![loaded.abi_digest().digest().to_string()];
    for rule in loaded.pure_rules() {
        if let Some(body) = rule.represented_digest() {
            digests.push(format!("{}: {}", rule.coordinate().name, body.digest()));
        }
    }
    Ok(digests)
}

/// Every corpus module over the imports it names, which is the environment
/// its own elaboration runs in.
fn corpus_env() -> TestResult<ImportEnv> {
    let mut imports = ImportEnv::new();
    for (module, relative) in CORPUS {
        let text = std::fs::read_to_string(workspace_file(relative))?;
        let loaded = load(module, relative, &text, &imports)?;
        imports.insert_loaded(&loaded);
    }
    Ok(imports)
}

/// The claim over the corpus: formatting moves no ABI digest and no body
/// digest, in either direction — a formatted module and the module it came
/// from are the same module to every reader below the frontend.
#[test]
fn formatting_moves_no_corpus_digest() -> TestResult {
    let imports = corpus_env()?;
    for (module, relative) in CORPUS {
        let text = std::fs::read_to_string(workspace_file(relative))?;
        let formatted =
            format_module(&source(module, relative, text.clone())).map_err(|diagnostics| {
                format!("the corpus module `{module}` does not parse: {diagnostics:?}")
            })?;

        let before = digests(module, relative, &text, &imports)?;
        let after = digests(module, relative, &formatted, &imports)?;
        assert_eq!(before, after, "formatting `{module}` moved a digest");
    }
    Ok(())
}

/// The claim over the notation: written bodies, the request constructs, and
/// an entry keep their digests through formatting too.
#[test]
fn formatting_moves_no_body_digest() -> TestResult {
    let formatted = format_module(&source("notation", "notation.pi", NOTATION))
        .map_err(|diagnostics| format!("the notation does not parse: {diagnostics:?}"))?;
    let imports = ImportEnv::new();

    let before = digests("notation", "notation.pi", NOTATION, &imports)?;
    let after = digests("notation", "notation.pi", &formatted, &imports)?;
    assert_eq!(before, after, "formatting the notation moved a digest");
    Ok(())
}

/// `fmt(fmt(x))` equals `fmt(x)`, over the corpus, the notation, and one
/// deliberately non-canonical module, through the query the `pith fmt`
/// command takes. The tree keeps its corpus canonical, so a corpus module's
/// first format can already be a no-op; the messy one cannot, and it keeps
/// the write path exercised. Formatting is parse-only, so no corpus module
/// needs its imports here.
#[test]
fn formatting_is_idempotent() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut modules: Vec<(String, String)> = CORPUS
        .iter()
        .map(|(module, relative)| {
            (
                format!("{module}.pi"),
                std::fs::read_to_string(workspace_file(relative)).unwrap_or_default(),
            )
        })
        .collect();
    modules.push(("messy.pi".to_owned(), "nominal   X   =   Text\n".to_owned()));
    modules.push(("notation.pi".to_owned(), NOTATION.to_owned()));

    let mut wrote = false;
    for (name, text) in modules {
        let path = directory.path().join(&name);
        std::fs::write(&path, text.as_bytes())?;
        let once = format(&path, FormatMode::Write)?;
        let twice = format(&path, FormatMode::Write)?;
        let checked = format(&path, FormatMode::Check)?;
        wrote |= once.status == FmtStatus::Formatted;
        assert_eq!(
            twice.status,
            FmtStatus::Unchanged,
            "a second format of `{name}` changed something"
        );
        assert_eq!(
            checked.status,
            FmtStatus::Unchanged,
            "a checked format of canonical `{name}` changed something"
        );
    }
    assert!(wrote, "no module exercised the write path");
    Ok(())
}

/// `--check` names what a write would change, and changes nothing. A module
/// that does not parse is a refusal, because it has no canonical spelling.
#[test]
fn check_names_a_change_without_making_it() -> TestResult {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("checked.pi");
    std::fs::write(&path, b"nominal   X   =   Text\n")?;

    let checked = format(&path, FormatMode::Check)?;
    assert_eq!(checked.status, FmtStatus::WouldFormat);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap_or_default(),
        "nominal   X   =   Text\n",
        "--check wrote to the file it was only naming"
    );

    let unparsable = directory.path().join("broken.pi");
    std::fs::write(&unparsable, b"nominal X =\n")?;
    assert!(format(&unparsable, FormatMode::Check).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn formatting_preserves_file_permissions() -> TestResult {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir()?;
    let path = directory.path().join("permissions.pi");
    std::fs::write(&path, b"nominal   X = Text\n")?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))?;

    let report = format(&path, FormatMode::Write)?;

    assert_eq!(report.status, FmtStatus::Formatted);
    assert_eq!(
        std::fs::metadata(&path)?.permissions().mode() & 0o777,
        0o640
    );
    Ok(())
}
