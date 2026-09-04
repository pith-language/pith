use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use pith_diag::{Diag, Severity, SourceId};
use pith_loader::{
    DefinitionKind, DefinitionLocation, LoadedModule, ModuleSource, elaborate_module, format_module,
};
use pith_output::dto::{
    AboutValueRepr, AboutView, CheckReport, DeclarationView, DiagnosticRepr, EntryView, FmtReport,
    FmtStatus, ImportView, ModuleView, RuleCategoryRepr, RuleView, SeverityRepr, TierRepr,
};

use crate::error::QueryError;
use crate::program::prepared_imports;

/// Whether `format` writes the canonical spelling back or only verifies it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FormatMode {
    Write,
    Check,
}

/// Format the module at `path`: write the canonical spelling of its parsed
/// surface back, or under [`FormatMode::Check`] name what a write would
/// change without touching the file.
///
/// The module has to parse — there is no canonical spelling of a module that
/// does not — but it does not have to elaborate, which is the property `fmt`
/// shares with `check`.
///
/// # Errors
/// [`QueryError`] when the file cannot be read, is not named as a module,
/// does not parse, or cannot be written.
pub fn format(path: &Path, mode: FormatMode) -> Result<FmtReport, QueryError> {
    let source = read_module(path)?;
    let canonical = format_module(&source).map_err(|diagnostics| {
        QueryError::user(format!(
            "`{}` does not parse, so it has no canonical spelling",
            path.display()
        ))
        .with_diagnostics(diagnostics)
    })?;
    let status = match (mode, canonical.as_str() == source.text.as_ref()) {
        (_, true) => FmtStatus::Unchanged,
        (FormatMode::Check, false) => FmtStatus::WouldFormat,
        (FormatMode::Write, false) => {
            replace_file(path, canonical.as_bytes()).map_err(|error| {
                QueryError::user(format!("cannot write `{}`: {error}", path.display()))
            })?;
            FmtStatus::Formatted
        }
    };
    Ok(FmtReport {
        module: source.module.clone(),
        path: path.display().to_string().into(),
        status,
    })
}

/// Elaborate the module at `path` and report what elaboration said, whether or
/// not it succeeded.
///
/// # Errors
/// [`QueryError`] when the file cannot be read or is not named as a module.
pub fn check(path: &Path) -> Result<CheckReport, QueryError> {
    let source = read_module(path)?;
    let (imports, parsed) = prepared_imports(path, &source)?;
    let (diagnostics, abi_digest) = match elaborate_module(parsed, &imports) {
        Ok(loaded) => (
            loaded.diagnostics().to_vec(),
            Some(loaded.abi_digest().digest().to_string().into()),
        ),
        Err(diagnostics) => (diagnostics.into_vec(), None),
    };
    let errors = count(&diagnostics, Severity::Error);
    let warnings = count(&diagnostics, Severity::Warning);
    Ok(CheckReport {
        module: source.module.clone(),
        path: path.display().to_string().into(),
        abi_digest,
        diagnostics: diagnostics.iter().map(diagnostic).collect(),
        errors,
        warnings,
    })
}

/// What the module at `path` declares: its types, its rules, the interface
/// each rule provides, and which tier answers it.
///
/// # Errors
/// [`QueryError`] when the file cannot be read, is not named as a module, or
/// does not elaborate. Unlike `check`, this one needs a module that loaded:
/// there is nothing to explore about a module with no declarations.
pub fn explore(path: &Path) -> Result<ModuleView, QueryError> {
    let source = read_module(path)?;
    let (imports, parsed) = prepared_imports(path, &source)?;
    let loaded = elaborate_module(parsed, &imports).map_err(|diagnostics| {
        QueryError::user(format!(
            "`{}` does not elaborate, so there is nothing to explore",
            path.display()
        ))
        .with_diagnostics(diagnostics)
    })?;
    Ok(module_view(path, &loaded))
}

fn module_view(path: &Path, loaded: &LoadedModule) -> ModuleView {
    let definitions = definition_index(loaded);
    let entry_definitions = entry_definition_index(loaded);
    ModuleView {
        module: loaded.module().into(),
        path: path.display().to_string().into(),
        abi_digest: loaded.abi_digest().digest().to_string().into(),
        imports: loaded
            .imports()
            .iter()
            .map(|(module, abi)| ImportView {
                module: module.clone(),
                abi_digest: abi.digest().to_string().into(),
            })
            .collect(),
        declarations: loaded
            .table()
            .iter()
            .map(|declaration| {
                let mut view = DeclarationView::from(declaration);
                view.documentation = documentation(&definitions, &view.name).into();
                view
            })
            .collect(),
        rules: rule_views(loaded, &definitions),
        entries: loaded
            .entries()
            .iter()
            .map(|entry| EntryView {
                name: entry.name().into(),
                coordinate: format!("{}::{}", loaded.module(), entry.rule_label()).into(),
                tier: TierRepr::Represented,
                interface: entry.interface().into(),
                documentation: documentation(&entry_definitions, entry.name()).into(),
            })
            .collect(),
        about: loaded
            .about()
            .iter()
            .map(|about| AboutView {
                fields: about
                    .fields
                    .iter()
                    .map(|(name, value)| {
                        let value = match value {
                            pith_hir::SurfaceAboutValue::Text(text) => {
                                AboutValueRepr::Text { text: text.clone() }
                            }
                            pith_hir::SurfaceAboutValue::List(elements) => AboutValueRepr::List {
                                elements: elements.clone(),
                            },
                        };
                        (name.clone(), value)
                    })
                    .collect(),
                documentation: span_documentation(loaded, &about.documentation).into(),
            })
            .collect(),
    }
}

fn rule_views(
    loaded: &LoadedModule,
    definitions: &BTreeMap<&str, &DefinitionLocation>,
) -> Box<[RuleView]> {
    let pure = loaded.pure_rules().iter().map(|rule| {
        (
            RuleCategoryRepr::Pure,
            rule.coordinate(),
            rule.interface(),
            rule.is_represented(),
        )
    });
    let action = loaded.action_rules().iter().map(|rule| {
        (
            RuleCategoryRepr::Action,
            rule.coordinate(),
            rule.interface(),
            rule.is_represented(),
        )
    });
    pure.chain(action)
        .map(|(category, coordinate, interface, represented)| RuleView {
            label: coordinate.name.clone(),
            category,
            tier: if represented {
                TierRepr::Represented
            } else {
                TierRepr::Host
            },
            interface: interface.into(),
            documentation: documentation(definitions, &coordinate.name).into(),
        })
        .collect()
}

fn definition_index(loaded: &LoadedModule) -> BTreeMap<&str, &DefinitionLocation> {
    let mut definitions = BTreeMap::new();
    for definition in loaded.positions().definitions().iter() {
        definitions
            .entry(definition.coordinate().name.as_ref())
            .or_insert(definition);
    }
    definitions
}

fn entry_definition_index(loaded: &LoadedModule) -> BTreeMap<&str, &DefinitionLocation> {
    loaded
        .positions()
        .definitions()
        .iter()
        .filter(|definition| matches!(definition.kind(), DefinitionKind::Entry))
        .map(|definition| (definition.coordinate().name.as_ref(), definition))
        .collect()
}

fn documentation(definitions: &BTreeMap<&str, &DefinitionLocation>, name: &str) -> String {
    definitions
        .get(name)
        .map_or_else(String::new, |definition| definition.documentation())
}

fn span_documentation(loaded: &LoadedModule, spans: &[pith_diag::Span]) -> String {
    let source = loaded.source().source_text();
    spans
        .iter()
        .filter_map(|span| {
            let start = usize::try_from(span.start.0).ok()?;
            let end = usize::try_from(span.end.0).ok()?;
            source.get(start..end)
        })
        .map(|line| line.strip_prefix("--").unwrap_or(line).trim())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn read_module(path: &Path) -> Result<ModuleSource, QueryError> {
    let module = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| {
            QueryError::user(format!(
                "`{}` does not name a module: a module's identity is its file stem",
                path.display()
            ))
        })?;
    let text = fs::read_to_string(path)
        .map_err(|error| QueryError::user(format!("cannot read `{}`: {error}", path.display())))?;
    let label = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(module);
    Ok(ModuleSource::new(
        module,
        SourceId::from_raw(0),
        label,
        text,
    ))
}

fn replace_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

    let permissions = fs::metadata(path)?.permissions();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let (temporary_path, mut temporary) = loop {
        let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".pith-format-{}-{sequence}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => break (candidate, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    };

    let publish = (|| {
        temporary.set_permissions(permissions)?;
        temporary.write_all(bytes)?;
        temporary.sync_all()?;
        drop(temporary);
        fs::rename(&temporary_path, path)?;
        File::open(parent)?.sync_all()
    })();
    if publish.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    publish
}

fn count(diagnostics: &[Diag], severity: Severity) -> u64 {
    let matching = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == severity)
        .count();
    u64::try_from(matching).unwrap_or(u64::MAX)
}

fn diagnostic(diagnostic: &Diag) -> DiagnosticRepr {
    let position = diagnostic
        .source
        .as_ref()
        .map(|source| (source.label.clone(), source.line_col(diagnostic.span.start)));
    let (label, line, column) = match position {
        Some((label, (line, column))) => (
            Some(label),
            Some(u64::try_from(line).unwrap_or(u64::MAX)),
            Some(u64::try_from(column).unwrap_or(u64::MAX)),
        ),
        None => (None, None, None),
    };
    DiagnosticRepr {
        severity: severity(diagnostic.severity),
        code: diagnostic.code.0,
        label,
        line,
        column,
        message: diagnostic.message.0.clone(),
    }
}

fn severity(severity: Severity) -> SeverityRepr {
    match severity {
        Severity::Error => SeverityRepr::Error,
        Severity::Warning => SeverityRepr::Warning,
        Severity::Info => SeverityRepr::Info,
        Severity::Note => SeverityRepr::Note,
    }
}

#[cfg(test)]
mod tests {
    use pith_output::dto::TierRepr;

    use super::explore;

    #[test]
    fn explore_preserves_documentation_while_indexing_definitions()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("documented.pi");
        std::fs::write(
            &path,
            "-- user-facing text\n\
             nominal Message = Text\n\
             -- renders a message\n\
             pure rule render(Message) -> Text = host\n",
        )?;

        let view = explore(&path)?;
        assert_eq!(view.declarations.len(), 1);
        assert_eq!(
            view.declarations
                .first()
                .map(|declaration| declaration.documentation.as_ref()),
            Some("user-facing text")
        );
        assert_eq!(view.rules.len(), 1);
        assert_eq!(
            view.rules.first().map(|rule| rule.documentation.as_ref()),
            Some("renders a message")
        );
        Ok(())
    }

    #[test]
    fn explore_resolves_file_relative_imports() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let dependency = root.path().join("dep");
        let consumer = root.path().join("consumer");
        std::fs::create_dir_all(&dependency)?;
        std::fs::create_dir_all(&consumer)?;
        std::fs::write(dependency.join("dep.pi"), "nominal Message = Text\n")?;
        let path = consumer.join("consumer.pi");
        std::fs::write(&path, "import dep\nnominal Wrapped = dep.Message\n")?;

        let view = explore(&path)?;
        assert_eq!(view.imports.len(), 1);
        assert_eq!(
            view.imports.first().map(|import| import.module.as_ref()),
            Some("dep")
        );
        Ok(())
    }

    #[test]
    fn explore_derives_the_rule_tier() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("represented.pi");
        std::fs::write(&path, "pure rule identity(value: Int) -> Int = { value }\n")?;

        let view = explore(&path)?;
        assert!(
            view.rules
                .first()
                .is_some_and(|rule| matches!(rule.tier, TierRepr::Represented))
        );
        Ok(())
    }
}
