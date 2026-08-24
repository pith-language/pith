use core::range::Range;
use std::sync::Arc;

use indexmap::IndexMap;
use pith_diag::{ByteOffset, Diag, SourceFile, Span};

use crate::FrontendCode;
use crate::lex::error;
use crate::surface::{
    ParsedSurface, SurfaceBody, SurfaceConstructor, SurfaceDeclaration, SurfaceField,
    SurfaceImport, SurfaceRule, SurfaceTypeArena, SurfaceTypeId, SurfaceTypeNode,
};

pub(crate) struct MergedModule {
    pub surface: ParsedSurface,
    pub files: ModuleFiles,
}

pub(crate) struct ModuleFiles {
    files: Box<[Arc<SourceFile>]>,
    bases: Box<[u32]>,
}

impl ModuleFiles {
    pub(crate) fn one(source: &Arc<SourceFile>) -> Self {
        Self {
            files: [source.clone()].into(),
            bases: [0].into(),
        }
    }

    pub(crate) fn source_of(&self, span: Span) -> &Arc<SourceFile> {
        let Some(source) = self.files.get(self.position_of(span)) else {
            unreachable!("a module always holds at least one file");
        };
        source
    }

    pub(crate) fn error(&self, code: FrontendCode, span: Span, message: impl Into<String>) -> Diag {
        let position = self.position_of(span);
        let Some(source) = self.files.get(position) else {
            unreachable!("a module always holds at least one file");
        };
        let base = self.bases.get(position).copied().unwrap_or(0);
        error(
            code,
            Span::new(
                ByteOffset(span.start.0.saturating_sub(base)),
                ByteOffset(span.end.0.saturating_sub(base)),
            ),
            message,
            source,
        )
    }

    fn position_of(&self, span: Span) -> usize {
        self.bases
            .partition_point(|base| *base <= span.start.0)
            .checked_sub(1)
            .unwrap_or_else(|| unreachable!("the first file begins at offset zero"))
    }
}

pub(crate) fn merge_module_files(files: &[(Arc<SourceFile>, ParsedSurface)]) -> MergedModule {
    let mut types: SurfaceTypeArena<SurfaceTypeNode> = SurfaceTypeArena::new();
    let mut fields: Vec<SurfaceField> = Vec::new();
    let mut imports = Vec::new();
    let mut declarations = Vec::new();
    let mut rules = Vec::new();
    let mut bases = Vec::with_capacity(files.len());
    let mut span_base = 0_u32;
    let mut field_base = 0_u32;

    for (source, surface) in files {
        bases.push(span_base);
        let mut remapped: IndexMap<SurfaceTypeId, SurfaceTypeId> = IndexMap::new();
        for (id, node) in surface.types.iter() {
            let remapped_id = types.push(remap_node(node, &remapped, span_base, field_base));
            remapped.insert(id, remapped_id);
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
                inputs: rule
                    .inputs
                    .iter()
                    .map(|input| remap_id(*input, &remapped))
                    .collect(),
                output: remap_id(rule.output, &remapped),
                category: rule.category,
                span: shifted(rule.span, span_base),
                documentation: shift_all(&rule.documentation, span_base),
            }
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
            types,
            fields,
            imports: imports.into(),
            declarations: declarations.into(),
            rules: rules.into(),
        },
        files: ModuleFiles {
            files: files.iter().map(|(source, _)| source.clone()).collect(),
            bases: bases.into(),
        },
    }
}

fn remap_id(id: SurfaceTypeId, remapped: &IndexMap<SurfaceTypeId, SurfaceTypeId>) -> SurfaceTypeId {
    remapped
        .get(&id)
        .copied()
        .unwrap_or_else(|| unreachable!("surface children are allocated before their parents"))
}

fn remap_node(
    node: &SurfaceTypeNode,
    remapped: &IndexMap<SurfaceTypeId, SurfaceTypeId>,
    span_base: u32,
    field_base: u32,
) -> SurfaceTypeNode {
    match node {
        SurfaceTypeNode::Unit
        | SurfaceTypeNode::Bool
        | SurfaceTypeNode::Int
        | SurfaceTypeNode::Text
        | SurfaceTypeNode::Bytes
        | SurfaceTypeNode::Blob => node.clone(),
        SurfaceTypeNode::List(element) => SurfaceTypeNode::List(remap_id(*element, remapped)),
        SurfaceTypeNode::Record { fields } => SurfaceTypeNode::Record {
            fields: Range {
                start: shifted_offset(fields.start, field_base),
                end: shifted_offset(fields.end, field_base),
            },
        },
        SurfaceTypeNode::Reference { module, name, span } => SurfaceTypeNode::Reference {
            module: module.clone(),
            name: name.clone(),
            span: shifted(*span, span_base),
        },
    }
}

fn remap_body(
    body: &SurfaceBody,
    remapped: &IndexMap<SurfaceTypeId, SurfaceTypeId>,
    span_base: u32,
) -> SurfaceBody {
    match body {
        SurfaceBody::Nominal(representation) => {
            SurfaceBody::Nominal(remap_id(*representation, remapped))
        }
        SurfaceBody::Alias(target) => SurfaceBody::Alias(remap_id(*target, remapped)),
        SurfaceBody::Sum(constructors) => SurfaceBody::Sum(
            constructors
                .iter()
                .map(|constructor| SurfaceConstructor {
                    name: constructor.name.clone(),
                    payload: constructor
                        .payload
                        .map(|payload| remap_id(payload, remapped)),
                    span: shifted(constructor.span, span_base),
                })
                .collect(),
        ),
    }
}

fn shifted(span: Span, base: u32) -> Span {
    Span::new(
        ByteOffset(shifted_offset(span.start.0, base)),
        ByteOffset(shifted_offset(span.end.0, base)),
    )
}

fn shifted_offset(offset: u32, base: u32) -> u32 {
    offset
        .checked_add(base)
        .unwrap_or_else(|| unreachable!("the merged module span exceeds u32::MAX bytes"))
}

fn next_span_base(current_base: u32, file_length: usize) -> u32 {
    let file_length = u32::try_from(file_length)
        .unwrap_or_else(|_| unreachable!("a source file cannot exceed u32::MAX bytes"));
    // Keep an EOF point span in its own file instead of selecting the next file.
    current_base
        .checked_add(file_length)
        .and_then(|next| next.checked_add(1))
        .unwrap_or_else(|| unreachable!("the merged module span exceeds u32::MAX bytes"))
}

fn shift_all(spans: &[Span], base: u32) -> Box<[Span]> {
    spans.iter().map(|span| shifted(*span, base)).collect()
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
