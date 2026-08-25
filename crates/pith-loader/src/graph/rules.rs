use std::collections::BTreeMap;
use std::sync::Arc;

use pith_core::Value;
use pith_diag::{Diag, DiagnosticSink, Severity, SourceFile, SourceId, Span};
use pith_elaborator::{elaborate, scope_imports};
use pith_engine::{PureRule, PureRuleFrame, PureStep, Resumption};
use pith_hir::{ParsedSurface, merge_module_files};
use pith_ids::{ContentId, ModuleAbiDigest};

use super::artifact::InterfaceSurface;
use super::values::{
    FrontendImport, FrontendSource, SourceEntry, ValueDiagnostic, ValueIncompleteRule,
    ValueRuleBinding, bodies_value, index_value, module_interface_value, read_import_env,
    read_source,
};
use crate::{FrontendCode, ImportEnv};

#[derive(Clone, Copy)]
pub(crate) enum Projection {
    Interface,
    Bodies,
    Index,
}

pub(crate) struct FrontendRule {
    projection: Projection,
}

impl FrontendRule {
    pub(crate) const fn new(projection: Projection) -> Self {
        Self { projection }
    }
}

impl PureRule for FrontendRule {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        let [source_value, import_value] = inputs else {
            unreachable!("the engine validated the request against the rule's interface");
        };
        Box::new(FrontendFrame {
            projection: self.projection,
            source: read_source(source_value),
            imports: read_import_env(import_value).entries,
            texts: Vec::new(),
            surfaces: Vec::new(),
            diagnostics: Vec::new(),
            content_diagnostics: Vec::new(),
            parsed: Vec::new(),
        })
    }
}

struct FrontendFrame {
    projection: Projection,
    source: FrontendSource,
    imports: Box<[FrontendImport]>,
    texts: Vec<Result<Box<str>, ContentId>>,
    surfaces: Vec<InterfaceSurface>,
    diagnostics: Vec<Diag>,
    content_diagnostics: Vec<ValueDiagnostic>,
    parsed: Vec<(Arc<SourceFile>, ParsedSurface)>,
}

impl PureRuleFrame for FrontendFrame {
    fn step(&mut self, input: Option<Resumption>) -> pith_diag::PithResult<PureStep> {
        if let Some(Resumption::One(Value::Bytes(bytes))) = input {
            if self.texts.len() < self.source.files.len() {
                let position = self.texts.len();
                let content = self.content_at(position);
                self.texts.push(match std::str::from_utf8(&bytes) {
                    Ok(text) => Ok(text.into()),
                    Err(_) => Err(content),
                });
            } else if self.surfaces.len() < self.imports.len() {
                match InterfaceSurface::decode(&bytes) {
                    Ok(surface) => {
                        let Some(import) = self.imports.get(self.surfaces.len()) else {
                            unreachable!("the surface count is below the import count");
                        };
                        validate_surface(import, &surface)?;
                        self.surfaces.push(surface);
                    }
                    Err(error) => {
                        return Err(frontend_failure(format!(
                            "an imported interface surface does not decode: {error}"
                        )));
                    }
                }
            }
        }
        if self.texts.len() < self.source.files.len() {
            return Ok(PureStep::NeedBlob(self.content_at(self.texts.len())));
        }
        let Some(entry) = self.imports.get(self.surfaces.len()) else {
            return Ok(PureStep::Complete(self.finish()));
        };
        Ok(PureStep::NeedBlob(entry.surface))
    }
}

impl FrontendFrame {
    fn content_at(&self, position: usize) -> ContentId {
        let Some(file) = self.source.files.get(position) else {
            unreachable!("the file position is checked before requesting content");
        };
        file.content
    }

    fn finish(&mut self) -> Value {
        self.parse_files();
        let merged = merge_module_files(&self.parsed);
        let mut environment = ImportEnv::new();
        for (import, surface) in self.imports.iter().zip(&self.surfaces) {
            environment.insert_surface(&import.binding, surface);
        }
        let scoped = scope_imports(
            &merged.surface,
            &environment.inner,
            &merged.files,
            &mut self.diagnostics,
        );
        let no_definitions = BTreeMap::new();
        let elaborated = elaborate(
            &self.source.module,
            &merged.surface,
            &scoped,
            &no_definitions,
            &merged.files,
            &mut self.diagnostics,
        );
        let ordered_imports = scoped
            .iter()
            .map(|(name, imported)| (Box::from(name), imported.abi_digest()))
            .collect::<Vec<(Box<str>, ModuleAbiDigest)>>();
        let surface = InterfaceSurface::of_parts(
            &self.source.module,
            ordered_imports.into(),
            elaborated.table.clone(),
            elaborated
                .rules
                .iter()
                .filter(|rule| !rule.local)
                .map(|rule| (rule.category, rule.interface.clone())),
        );
        let mut diagnostics = self.content_diagnostics.clone();
        diagnostics.extend(value_diagnostics(&self.diagnostics, &self.source.files));
        match self.projection {
            Projection::Interface => module_interface_value(
                &self.source.module,
                surface.abi_digest(),
                &surface,
                &diagnostics,
            ),
            Projection::Bodies => {
                let rules = elaborated
                    .rules
                    .iter()
                    .map(|rule| ValueRuleBinding {
                        module: self.source.module.clone(),
                        name: rule.label.clone(),
                        category: rule.category,
                        interface: rule.interface.clone(),
                    })
                    .collect::<Vec<_>>();
                let incomplete_rules = elaborated
                    .incomplete_rules
                    .iter()
                    .map(|rule| ValueIncompleteRule {
                        module: self.source.module.clone(),
                        name: rule.label.clone(),
                        diagnostics: value_diagnostics(&rule.diagnostics, &self.source.files)
                            .into(),
                    })
                    .collect::<Vec<_>>();
                bodies_value(&rules, &incomplete_rules, &diagnostics)
            }
            Projection::Index => {
                let mut entries = elaborated
                    .rules
                    .iter()
                    .filter(|rule| !rule.local)
                    .map(|rule| (rule.category, rule.interface.clone(), rule.label.clone()))
                    .collect::<Vec<_>>();
                entries.sort_by(|left, right| {
                    (left.0, left.1.encode_canonical(), &left.2).cmp(&(
                        right.0,
                        right.1.encode_canonical(),
                        &right.2,
                    ))
                });
                let entries = entries
                    .into_iter()
                    .map(|(category, interface, name)| {
                        (category, interface, self.source.module.clone(), name)
                    })
                    .collect::<Vec<_>>();
                index_value(&entries, &diagnostics)
            }
        }
    }

    fn parse_files(&mut self) {
        self.parsed = self
            .texts
            .iter()
            .enumerate()
            .filter_map(|(position, text)| {
                let entry = self.source.files.get(position)?;
                match text {
                    Ok(text) => {
                        let source_file = Arc::new(SourceFile::new(
                            SourceId::from_raw(u32::try_from(position).unwrap_or_else(|_| {
                                unreachable!("a module cannot hold more than u32::MAX files")
                            })),
                            entry.path.clone(),
                            text.clone(),
                        ));
                        let (surface, mut diagnostics) = pith_syntax::parse(&source_file);
                        self.diagnostics.append(&mut diagnostics);
                        Some((source_file, surface))
                    }
                    Err(content) => {
                        self.content_diagnostics.push(ValueDiagnostic {
                            code: FrontendCode::SourceNotUtf8.stable().0,
                            message: format!("the file at `{}` is not UTF-8", entry.path).into(),
                            source: *content,
                            start: 0,
                            end: 0,
                        });
                        None
                    }
                }
            })
            .collect();
    }
}

fn value_diagnostics(diagnostics: &[Diag], files: &[SourceEntry]) -> Vec<ValueDiagnostic> {
    diagnostics
        .iter()
        .filter_map(|diagnostic| {
            let source = diagnostic.source.as_ref()?;
            let entry = files.get(usize::try_from(source.id.to_raw()).ok()?)?;
            Some(ValueDiagnostic {
                code: diagnostic.code.0,
                message: diagnostic.message.0.clone(),
                source: entry.content,
                start: diagnostic.span.start.0,
                end: diagnostic.span.end.0,
            })
        })
        .collect()
}

fn validate_surface(
    import: &FrontendImport,
    surface: &InterfaceSurface,
) -> pith_diag::PithResult<()> {
    if surface.module != import.module {
        return Err(frontend_failure(format!(
            "import `{}` names module `{}`, but its surface declares `{}`",
            import.binding, import.module, surface.module
        )));
    }
    let actual_abi = surface.abi_digest();
    if actual_abi != import.abi {
        return Err(frontend_failure(format!(
            "import `{}` has an interface surface that does not match its ABI digest",
            import.binding
        )));
    }
    if surface.content_id() != import.surface {
        return Err(frontend_failure(format!(
            "import `{}` has an interface surface that does not match its content identity",
            import.binding
        )));
    }
    Ok(())
}

fn frontend_failure(message: String) -> DiagnosticSink {
    let mut sink = DiagnosticSink::new();
    sink.push(Diag::new(
        Severity::Error,
        FrontendCode::MalformedSurface.stable(),
        Span::none(),
        message,
    ));
    sink
}
