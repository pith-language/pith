use std::sync::Arc;

use indexmap::IndexMap;
use pith_diag::{ByteOffset, Diag, Severity, SourceFile, Span};

use crate::FrontendCode;
use crate::body::{SurfaceExpr, SurfaceExprArena, SurfaceExprId};

mod remap;

use crate::surface::{
    ParsedSurface, SurfaceAbout, SurfaceComment, SurfaceDeclaration, SurfaceEntry, SurfaceField,
    SurfaceImport, SurfaceLocal, SurfaceParam, SurfaceRule, SurfaceTypeArena, SurfaceTypeId,
    SurfaceTypeNode,
};
use remap::{
    next_span_base, remap_body, remap_expr, remap_id, remap_node, remap_request, remap_rule_body,
    remap_value, shift_all, shifted,
};

pub struct MergedModule {
    pub surface: ParsedSurface,
    pub files: ModuleFiles,
}

pub struct ModuleFiles {
    files: Box<[Arc<SourceFile>]>,
    bases: Box<[u32]>,
}

impl ModuleFiles {
    pub fn one(source: &Arc<SourceFile>) -> Self {
        Self {
            files: [source.clone()].into(),
            bases: [0].into(),
        }
    }

    pub fn sources(&self) -> &[Arc<SourceFile>] {
        &self.files
    }

    /// The file a merged span landed in, with the span rebased to that
    /// file's own offsets: the attribution every position sidecar and
    /// diagnostic needs when one module holds several files.
    pub fn file_of(&self, span: Span) -> (&Arc<SourceFile>, Span) {
        let position = self.position_of(span);
        let Some(source) = self.files.get(position) else {
            unreachable!("a module always holds at least one file");
        };
        let base = self.bases.get(position).copied().unwrap_or(0);
        (
            source,
            Span::new(
                ByteOffset(span.start.0.saturating_sub(base)),
                ByteOffset(span.end.0.saturating_sub(base)),
            ),
        )
    }

    pub fn source_of(&self, span: Span) -> &Arc<SourceFile> {
        self.file_of(span).0
    }

    pub fn error(&self, code: FrontendCode, span: Span, message: impl Into<String>) -> Diag {
        let (source, local) = self.file_of(span);
        error(code, local, message, source)
    }

    fn position_of(&self, span: Span) -> usize {
        self.bases
            .partition_point(|base| *base <= span.start.0)
            .checked_sub(1)
            .unwrap_or_else(|| unreachable!("the first file begins at offset zero"))
    }
}

pub fn merge_module_files(files: &[(Arc<SourceFile>, ParsedSurface)]) -> MergedModule {
    let mut types: SurfaceTypeArena<SurfaceTypeNode> = SurfaceTypeArena::new();
    let mut exprs: SurfaceExprArena<SurfaceExpr> = SurfaceExprArena::new();
    let mut fields: Vec<SurfaceField> = Vec::new();
    let mut imports = Vec::new();
    let mut declarations = Vec::new();
    let mut rules = Vec::new();
    let mut locals = Vec::new();
    let mut entries = Vec::new();
    let mut about = Vec::new();
    let mut comments = Vec::new();
    let mut bases = Vec::with_capacity(files.len());
    let mut span_base = 0_u32;
    let mut field_base = 0_u32;
    let header = files
        .first()
        .map(|(_, surface)| surface.header.clone())
        .unwrap_or(crate::project::DocumentHeader::Include);

    for (source, surface) in files {
        bases.push(span_base);
        let mut remapped: IndexMap<SurfaceTypeId, SurfaceTypeId> = IndexMap::new();
        for (id, node) in surface.types.iter() {
            let remapped_id = types.push(remap_node(node, &remapped, span_base, field_base));
            remapped.insert(id, remapped_id);
        }
        let mut remapped_exprs: IndexMap<SurfaceExprId, SurfaceExprId> = IndexMap::new();
        for (id, node) in surface.exprs.iter() {
            let remapped_id = exprs.push(remap_expr(node, &remapped_exprs, span_base));
            remapped_exprs.insert(id, remapped_id);
        }
        for field in &surface.fields {
            fields.push(SurfaceField {
                name: field.name.clone(),
                payload: remap_id(field.payload, &remapped),
                span: shifted(field.span, span_base),
            });
        }
        imports.extend(surface.imports.iter().map(|import| SurfaceImport {
            module: import.module.clone(),
            span: shifted(import.span, span_base),
            documentation: shift_all(&import.documentation, span_base),
        }));
        declarations.extend(
            surface
                .declarations
                .iter()
                .map(|declaration| SurfaceDeclaration {
                    name: declaration.name.clone(),
                    name_span: shifted(declaration.name_span, span_base),
                    body: remap_body(&declaration.body, &remapped, span_base),
                    documentation: shift_all(&declaration.documentation, span_base),
                }),
        );
        rules.extend(surface.rules.iter().map(|rule| {
            SurfaceRule {
                label: rule.label.clone(),
                label_span: shifted(rule.label_span, span_base),
                params: rule
                    .params
                    .iter()
                    .map(|param| SurfaceParam {
                        name: param
                            .name
                            .as_ref()
                            .map(|(name, span)| (name.clone(), shifted(*span, span_base))),
                        payload: remap_id(param.payload, &remapped),
                    })
                    .collect(),
                output: remap_id(rule.output, &remapped),
                category: rule.category,
                body: remap_rule_body(&rule.body, &remapped_exprs, &remapped, span_base),
                span: shifted(rule.span, span_base),
                documentation: shift_all(&rule.documentation, span_base),
            }
        }));
        locals.extend(surface.locals.iter().map(|local| SurfaceLocal {
            name: local.name.clone(),
            name_span: shifted(local.name_span, span_base),
            annotation: remap_id(local.annotation, &remapped),
            value: remap_value(&local.value, &remapped_exprs, &remapped, span_base),
            span: shifted(local.span, span_base),
            documentation: shift_all(&local.documentation, span_base),
        }));
        entries.extend(surface.entries.iter().map(|entry| SurfaceEntry {
            name: entry.name.clone(),
            name_span: shifted(entry.name_span, span_base),
            output: remap_id(entry.output, &remapped),
            request: remap_request(&entry.request, &remapped_exprs, &remapped, span_base),
            span: shifted(entry.span, span_base),
            documentation: shift_all(&entry.documentation, span_base),
        }));
        about.extend(surface.about.iter().map(|block| SurfaceAbout {
            fields: block.fields.clone(),
            span: shifted(block.span, span_base),
            documentation: shift_all(&block.documentation, span_base),
        }));
        comments.extend(surface.comments.iter().map(|comment| SurfaceComment {
            span: shifted(comment.span, span_base),
            trailing: comment.trailing,
        }));
        span_base = next_span_base(span_base, source.source_text().len());
        field_base = field_base
            .checked_add(u32::try_from(surface.fields.len()).unwrap_or_else(|_| {
                unreachable!("a surface cannot hold more than u32::MAX fields")
            }))
            .unwrap_or_else(|| unreachable!("the merged field arena exceeds u32::MAX entries"));
    }

    MergedModule {
        surface: ParsedSurface {
            header,
            types,
            exprs,
            fields,
            imports: imports.into(),
            declarations: declarations.into(),
            rules: rules.into(),
            locals: locals.into(),
            entries: entries.into(),
            about: about.into(),
            comments: comments.into(),
        },
        files: ModuleFiles {
            files: files.iter().map(|(source, _)| source.clone()).collect(),
            bases: bases.into(),
        },
    }
}

fn error(
    code: FrontendCode,
    span: Span,
    message: impl Into<String>,
    source: &Arc<SourceFile>,
) -> Diag {
    Diag::new(Severity::Error, code.stable(), span, message.into()).with_source(source.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pith_diag::SourceId;

    #[test]
    fn an_end_of_file_span_stays_with_the_file_before_the_boundary() {
        let first = Arc::new(SourceFile::new(SourceId::from_raw(1), "first.pi", "text"));
        let second = Arc::new(SourceFile::new(SourceId::from_raw(2), "second.pi", "more"));
        let files = ModuleFiles {
            files: [first.clone(), second].into(),
            bases: [0, next_span_base(0, first.source_text().len())].into(),
        };

        let diagnostic = files.error(
            FrontendCode::UnexpectedToken,
            Span::point(ByteOffset(4)),
            "expected a declaration",
        );
        let Some(source) = diagnostic.source else {
            unreachable!("module diagnostics carry their source");
        };
        assert_eq!(source.id, first.id);
        assert_eq!(diagnostic.span, Span::point(ByteOffset(4)));
    }
}
