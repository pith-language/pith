use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::files;
use crate::report::{Diagnostic, Report};

const SCHEMA_V1: &str = "design-doc/v1";

pub(crate) fn run(root: &Path) -> Report {
    let (mut paths, walk_diagnostics) = files::with_extension(root, Path::new("docs"), "md");
    if root.join("README.md").is_file() {
        paths.push(PathBuf::from("README.md"));
        paths.sort();
    }

    let mut report = Report::new(
        "docs",
        format!("validated {} Markdown documents", paths.len()),
    );
    report.extend(walk_diagnostics);

    let mut documents = Vec::new();
    for path in paths {
        let source = match std::fs::read_to_string(root.join(&path)) {
            Ok(source) => source,
            Err(error) => {
                report.push(Diagnostic::file(
                    &path,
                    format!("could not read document: {error}"),
                ));
                continue;
            }
        };

        let (document, diagnostics) = parse_document(&path, &source);
        report.extend(diagnostics);
        documents.extend(document);
    }

    report.extend(validate_repository(&documents));
    report.sort_diagnostics();
    report
}

#[derive(Debug)]
struct Document {
    path: PathBuf,
    metadata: FrontmatterV1,
}

#[derive(Deserialize)]
struct SchemaEnvelope {
    schema: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrontmatterV1 {
    #[serde(rename = "schema")]
    _schema: SchemaV1,
    id: String,
    title: String,
    summary: String,
    kind: Kind,
    #[serde(rename = "status")]
    _status: Status,
    created: String,
    updated: String,
    tags: Vec<String>,
    relations: Relations,
    evidence: Option<Evidence>,
}

#[derive(Debug, Deserialize)]
enum SchemaV1 {
    #[serde(rename = "design-doc/v1")]
    V1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Decision,
    Design,
    Foundation,
    Index,
    Planning,
    Requirements,
    Research,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Decision => "decision",
            Self::Design => "design",
            Self::Foundation => "foundation",
            Self::Index => "index",
            Self::Planning => "planning",
            Self::Requirements => "requirements",
            Self::Research => "research",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Status {
    Accepted,
    Active,
    Draft,
    Proposed,
    Researching,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Evidence {
    Preliminary,
    Reviewed,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Relations {
    informed_by: Vec<String>,
    depends_on: Vec<String>,
    #[serde(default)]
    supersedes: Vec<String>,
    #[serde(default)]
    amends: Vec<String>,
}

impl Relations {
    fn named(&self) -> [(&'static str, &[String]); 4] {
        [
            ("informed_by", self.informed_by.as_slice()),
            ("depends_on", self.depends_on.as_slice()),
            ("supersedes", self.supersedes.as_slice()),
            ("amends", self.amends.as_slice()),
        ]
    }
}

fn parse_document(path: &Path, source: &str) -> (Option<Document>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let Some(frontmatter) = extract_frontmatter(path, source, &mut diagnostics) else {
        return (None, diagnostics);
    };

    let envelope: SchemaEnvelope = match serde_yaml_ng::from_str(&frontmatter) {
        Ok(envelope) => envelope,
        Err(error) => {
            diagnostics.push(yaml_diagnostic(path, &error));
            return (None, diagnostics);
        }
    };

    if envelope.schema != SCHEMA_V1 {
        diagnostics.push(Diagnostic::file(
            path,
            format!(
                "unsupported frontmatter schema `{}`; expected `{SCHEMA_V1}`",
                envelope.schema
            ),
        ));
        return (None, diagnostics);
    }

    let metadata: FrontmatterV1 = match serde_yaml_ng::from_str(&frontmatter) {
        Ok(metadata) => metadata,
        Err(error) => {
            diagnostics.push(yaml_diagnostic(path, &error));
            return (None, diagnostics);
        }
    };

    validate_metadata(path, &metadata, &mut diagnostics);
    (
        Some(Document {
            path: path.to_path_buf(),
            metadata,
        }),
        diagnostics,
    )
}

fn extract_frontmatter(
    path: &Path,
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    let mut lines = source.lines();
    if lines.next() != Some("---") {
        diagnostics.push(Diagnostic::line(
            path,
            1,
            "document must start with a `---` frontmatter delimiter",
        ));
        return None;
    }

    let mut yaml_lines = Vec::new();
    for line in lines {
        if line == "---" {
            return Some(yaml_lines.join("\n"));
        }
        yaml_lines.push(line);
    }

    diagnostics.push(Diagnostic::line(
        path,
        1,
        "frontmatter has no closing `---` delimiter",
    ));
    None
}

fn yaml_diagnostic(path: &Path, error: &serde_yaml_ng::Error) -> Diagnostic {
    let rendered = error.to_string();
    match error.location() {
        Some(location) => {
            let location_suffix =
                format!(" at line {} column {}", location.line(), location.column());
            let detail = rendered.strip_suffix(&location_suffix).unwrap_or(&rendered);
            let message = format!(
                "invalid `{SCHEMA_V1}` frontmatter at column {}: {detail}",
                location.column()
            );
            let document_line = location.line().checked_add(1).unwrap_or(location.line());
            Diagnostic::line(path, document_line, message)
        }
        None => Diagnostic::file(
            path,
            format!("invalid `{SCHEMA_V1}` frontmatter: {rendered}"),
        ),
    }
}

fn validate_metadata(path: &Path, metadata: &FrontmatterV1, diagnostics: &mut Vec<Diagnostic>) {
    validate_nonempty(path, "id", &metadata.id, diagnostics);
    validate_nonempty(path, "title", &metadata.title, diagnostics);
    validate_nonempty(path, "summary", &metadata.summary, diagnostics);

    if !is_kebab_case(&metadata.id) {
        diagnostics.push(Diagnostic::file(
            path,
            format!("id `{}` must be lowercase kebab-case", metadata.id),
        ));
    }

    validate_date(path, "created", &metadata.created, diagnostics);
    validate_date(path, "updated", &metadata.updated, diagnostics);
    if is_date(&metadata.created)
        && is_date(&metadata.updated)
        && metadata.created > metadata.updated
    {
        diagnostics.push(Diagnostic::file(
            path,
            format!(
                "created date `{}` is later than updated date `{}`",
                metadata.created, metadata.updated
            ),
        ));
    }

    validate_named_list(path, "tags", &metadata.tags, true, diagnostics);
    for (relation, targets) in metadata.relations.named() {
        validate_named_list(
            path,
            &format!("relations.{relation}"),
            targets,
            false,
            diagnostics,
        );
        for target in targets {
            if target == &metadata.id {
                diagnostics.push(Diagnostic::file(
                    path,
                    format!("relations.{relation} contains a self-reference to `{target}`"),
                ));
            }
        }
    }

    match (metadata.kind, metadata.evidence.as_ref()) {
        (Kind::Research, None) => diagnostics.push(Diagnostic::file(
            path,
            "research documents must declare `evidence`",
        )),
        (Kind::Research, Some(_)) | (_, None) => {}
        (_, Some(_)) => diagnostics.push(Diagnostic::file(
            path,
            "only research documents may declare `evidence`",
        )),
    }

    if let Some(expected) = expected_kind(path)
        && metadata.kind != expected
    {
        diagnostics.push(Diagnostic::file(
            path,
            format!(
                "kind `{}` does not match the document location; expected `{}`",
                metadata.kind.as_str(),
                expected.as_str()
            ),
        ));
    }
}

fn validate_nonempty(path: &Path, field: &str, value: &str, diagnostics: &mut Vec<Diagnostic>) {
    if value.trim().is_empty() {
        diagnostics.push(Diagnostic::file(
            path,
            format!("`{field}` must not be empty"),
        ));
    }
}

fn validate_named_list(
    path: &Path,
    field: &str,
    values: &[String],
    require_nonempty: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if require_nonempty && values.is_empty() {
        diagnostics.push(Diagnostic::file(
            path,
            format!("`{field}` must contain at least one value"),
        ));
    }

    let mut seen = BTreeSet::new();
    for value in values {
        if !is_kebab_case(value) {
            diagnostics.push(Diagnostic::file(
                path,
                format!("`{field}` value `{value}` must be lowercase kebab-case"),
            ));
        }
        if !seen.insert(value) {
            diagnostics.push(Diagnostic::file(
                path,
                format!("`{field}` contains duplicate value `{value}`"),
            ));
        }
    }
}

fn validate_date(path: &Path, field: &str, value: &str, diagnostics: &mut Vec<Diagnostic>) {
    if !is_date(value) {
        diagnostics.push(Diagnostic::file(
            path,
            format!("`{field}` value `{value}` must be a valid YYYY-MM-DD date"),
        ));
    }
}

#[allow(
    clippy::arithmetic_side_effects,
    reason = "calendar divisors are constants and parsed years are bounded integers"
)]
fn is_date(value: &str) -> bool {
    let mut parts = value.split('-');
    let (Some(year), Some(month), Some(day), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    if year.len() != 4 || month.len() != 2 || day.len() != 2 {
        return false;
    }
    if !year.chars().all(|character| character.is_ascii_digit())
        || !month.chars().all(|character| character.is_ascii_digit())
        || !day.chars().all(|character| character.is_ascii_digit())
    {
        return false;
    }

    let (Ok(year), Ok(month), Ok(day)) =
        (year.parse::<u16>(), month.parse::<u8>(), day.parse::<u8>())
    else {
        return false;
    };
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days_in_month).contains(&day)
}

fn is_kebab_case(value: &str) -> bool {
    let mut previous_was_separator = true;
    for character in value.chars() {
        match character {
            'a'..='z' | '0'..='9' => previous_was_separator = false,
            '-' if !previous_was_separator => previous_was_separator = true,
            _ => return false,
        }
    }
    !previous_was_separator
}

fn expected_kind(path: &Path) -> Option<Kind> {
    if path == Path::new("README.md") || path == Path::new("docs/index.md") {
        return Some(Kind::Index);
    }

    let mut components = path.strip_prefix("docs").ok()?.components();
    match components.next()?.as_os_str().to_str()? {
        "decisions" => Some(Kind::Decision),
        "design" => Some(Kind::Design),
        "foundation" => Some(Kind::Foundation),
        "planning" => Some(Kind::Planning),
        "requirements" => Some(Kind::Requirements),
        "research" => Some(Kind::Research),
        _ => None,
    }
}

fn validate_repository(documents: &[Document]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut ids: BTreeMap<&str, &Path> = BTreeMap::new();

    for document in documents {
        match ids.entry(&document.metadata.id) {
            Entry::Vacant(entry) => {
                entry.insert(&document.path);
            }
            Entry::Occupied(entry) => diagnostics.push(Diagnostic::file(
                &document.path,
                format!(
                    "duplicate document id `{}`; first declared in {}",
                    document.metadata.id,
                    entry.get().display()
                ),
            )),
        }
    }

    for document in documents {
        for (relation, targets) in document.metadata.relations.named() {
            for target in targets {
                if !ids.contains_key(target.as_str()) {
                    diagnostics.push(Diagnostic::file(
                        &document.path,
                        format!("relations.{relation} target `{target}` does not exist"),
                    ));
                }
            }
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::{parse_document, validate_repository};
    use std::path::{Path, PathBuf};

    const VALID_RESEARCH: &str = r"---
schema: design-doc/v1
id: research-example
title: example
summary: an example document
kind: research
status: researching
evidence: preliminary
created: 2026-08-17
updated: 2026-08-17
tags:
  - example
relations:
  informed_by: []
  depends_on: []
  supersedes: []
---

# example
";

    #[test]
    fn accepts_valid_frontmatter() {
        let (document, diagnostics) =
            parse_document(Path::new("docs/research/example.md"), VALID_RESEARCH);

        assert!(document.is_some());
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    }

    #[test]
    fn rejects_unknown_fields() {
        let source = VALID_RESEARCH.replace("title: example", "owner: example");
        let (document, diagnostics) =
            parse_document(Path::new("docs/research/example.md"), &source);

        assert!(document.is_none());
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.to_string().contains("unknown field `owner`")),
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn rejects_invalid_dates() {
        let source = VALID_RESEARCH.replace("2026-08-17", "2026-02-30");
        let (_document, diagnostics) =
            parse_document(Path::new("docs/research/example.md"), &source);

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.to_string().contains("valid YYYY-MM-DD date")),
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn rejects_unresolved_relations() {
        let source = VALID_RESEARCH.replace("depends_on: []", "depends_on:\n    - missing-doc");
        let (document, parse_diagnostics) =
            parse_document(Path::new("docs/research/example.md"), &source);
        let documents: Vec<_> = document.into_iter().collect();
        let repository_diagnostics = validate_repository(&documents);

        assert!(parse_diagnostics.is_empty(), "{parse_diagnostics:#?}");
        assert!(
            repository_diagnostics.iter().any(|diagnostic| diagnostic
                .to_string()
                .contains("target `missing-doc` does not exist")),
            "{repository_diagnostics:#?}"
        );
    }

    #[test]
    fn rejects_duplicate_ids() {
        let (first, first_diagnostics) =
            parse_document(Path::new("docs/research/first.md"), VALID_RESEARCH);
        let (second, second_diagnostics) =
            parse_document(Path::new("docs/research/second.md"), VALID_RESEARCH);
        let documents: Vec<_> = first.into_iter().chain(second).collect();
        let repository_diagnostics = validate_repository(&documents);

        assert!(first_diagnostics.is_empty(), "{first_diagnostics:#?}");
        assert!(second_diagnostics.is_empty(), "{second_diagnostics:#?}");
        assert!(
            repository_diagnostics
                .iter()
                .any(|diagnostic| diagnostic.to_string().contains("duplicate document id")),
            "{repository_diagnostics:#?}"
        );
    }

    #[test]
    fn rejects_kind_that_does_not_match_location() {
        let (document, diagnostics) =
            parse_document(Path::new("docs/design/example.md"), VALID_RESEARCH);

        assert!(document.is_some());
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.to_string().contains("expected `design`")),
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn diagnostic_paths_remain_relative() {
        let path = PathBuf::from("docs/research/example.md");
        let source = VALID_RESEARCH.replace("schema: design-doc/v1", "schema: design-doc/v2");
        let (_document, diagnostics) = parse_document(&path, &source);

        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| !diagnostic.to_string().starts_with('/')),
            "{diagnostics:#?}"
        );
    }
}
