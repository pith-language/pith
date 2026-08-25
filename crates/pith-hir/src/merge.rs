use core::range::Range;
use std::sync::Arc;

use indexmap::IndexMap;
use pith_diag::{ByteOffset, Diag, Severity, SourceFile, Span};

use crate::FrontendCode;
use crate::body::{
    SurfaceBatchMember, SurfaceClause, SurfaceExpr, SurfaceExprArena, SurfaceExprId,
    SurfaceRequest, SurfaceStatement, SurfaceValue, SurfaceWrittenBody,
};
use crate::surface::{
    ParsedSurface, SurfaceAbout, SurfaceBody, SurfaceComment, SurfaceConstructor,
    SurfaceDeclaration, SurfaceEntry, SurfaceField, SurfaceImport, SurfaceLocal, SurfaceParam,
    SurfaceRule, SurfaceRuleBody, SurfaceTypeArena, SurfaceTypeId, SurfaceTypeNode,
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

    pub fn source_of(&self, span: Span) -> &Arc<SourceFile> {
        let Some(source) = self.files.get(self.position_of(span)) else {
            unreachable!("a module always holds at least one file");
        };
        source
    }

    pub fn error(&self, code: FrontendCode, span: Span, message: impl Into<String>) -> Diag {
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

fn remap_expr(
    node: &SurfaceExpr,
    remapped: &IndexMap<SurfaceExprId, SurfaceExprId>,
    span_base: u32,
) -> SurfaceExpr {
    let expr = |id: SurfaceExprId| remap_id(id, remapped);
    match node {
        SurfaceExpr::Literal { value, span } => SurfaceExpr::Literal {
            value: value.clone(),
            span: shifted(*span, span_base),
        },
        SurfaceExpr::Name { name, span } => SurfaceExpr::Name {
            name: name.clone(),
            span: shifted(*span, span_base),
        },
        SurfaceExpr::Field { record, name, span } => SurfaceExpr::Field {
            record: expr(*record),
            name: name.clone(),
            span: shifted(*span, span_base),
        },
        SurfaceExpr::Record { fields, span } => SurfaceExpr::Record {
            fields: fields
                .iter()
                .map(|field| crate::body::SurfaceValueField {
                    name: field.name.clone(),
                    value: expr(field.value),
                    span: shifted(field.span, span_base),
                })
                .collect(),
            span: shifted(*span, span_base),
        },
        SurfaceExpr::List { items, span } => SurfaceExpr::List {
            items: items.iter().map(|item| expr(*item)).collect(),
            span: shifted(*span, span_base),
        },
        SurfaceExpr::Construct {
            name,
            arguments,
            span,
        } => SurfaceExpr::Construct {
            name: name.clone(),
            arguments: arguments.iter().map(|argument| expr(*argument)).collect(),
            span: shifted(*span, span_base),
        },
        SurfaceExpr::Unwrap { value, span } => SurfaceExpr::Unwrap {
            value: expr(*value),
            span: shifted(*span, span_base),
        },
        SurfaceExpr::If {
            condition,
            then,
            otherwise,
            span,
        } => SurfaceExpr::If {
            condition: expr(*condition),
            then: expr(*then),
            otherwise: expr(*otherwise),
            span: shifted(*span, span_base),
        },
        SurfaceExpr::Match {
            scrutinee,
            arms,
            span,
        } => SurfaceExpr::Match {
            scrutinee: expr(*scrutinee),
            arms: arms
                .iter()
                .map(|arm| crate::body::SurfaceArm {
                    constructor: arm.constructor.clone(),
                    binder: arm.binder.clone(),
                    body: expr(arm.body),
                    span: shifted(arm.span, span_base),
                })
                .collect(),
            span: shifted(*span, span_base),
        },
        SurfaceExpr::Fold {
            source,
            init,
            element,
            accumulator,
            step,
            span,
        } => SurfaceExpr::Fold {
            source: expr(*source),
            init: expr(*init),
            element: element.clone(),
            accumulator: accumulator.clone(),
            step: expr(*step),
            span: shifted(*span, span_base),
        },
        SurfaceExpr::Binary {
            operator,
            left,
            right,
            span,
        } => SurfaceExpr::Binary {
            operator: *operator,
            left: expr(*left),
            right: expr(*right),
            span: shifted(*span, span_base),
        },
    }
}

fn remap_request(
    request: &SurfaceRequest,
    remapped: &IndexMap<SurfaceExprId, SurfaceExprId>,
    remapped_types: &IndexMap<SurfaceTypeId, SurfaceTypeId>,
    span_base: u32,
) -> SurfaceRequest {
    let expr = |id: SurfaceExprId| remap_id(id, remapped);
    let remap_head = |head: Option<SurfaceTypeId>| head.map(|id| remap_id(id, remapped_types));
    match request {
        SurfaceRequest::Ask {
            head,
            arguments,
            span,
        } => SurfaceRequest::Ask {
            head: remap_head(*head),
            arguments: arguments.iter().map(|argument| expr(*argument)).collect(),
            span: shifted(*span, span_base),
        },
        SurfaceRequest::Run {
            head,
            arguments,
            span,
        } => SurfaceRequest::Run {
            head: remap_head(*head),
            arguments: arguments.iter().map(|argument| expr(*argument)).collect(),
            span: shifted(*span, span_base),
        },
        SurfaceRequest::AskAll { requests, span } => SurfaceRequest::AskAll {
            requests: requests
                .iter()
                .map(|member| remap_batch_member(member, remapped, remapped_types, span_base))
                .collect(),
            span: shifted(*span, span_base),
        },
        SurfaceRequest::AskEach {
            head,
            binder,
            source,
            clauses,
            arguments,
            span,
        } => SurfaceRequest::AskEach {
            head: remap_head(*head),
            binder: binder.clone(),
            source: expr(*source),
            clauses: clauses
                .iter()
                .map(|clause| match clause {
                    SurfaceClause::Let { name, value, span } => SurfaceClause::Let {
                        name: name.clone(),
                        value: expr(*value),
                        span: shifted(*span, span_base),
                    },
                    SurfaceClause::Filter { condition, span } => SurfaceClause::Filter {
                        condition: expr(*condition),
                        span: shifted(*span, span_base),
                    },
                })
                .collect(),
            arguments: arguments.iter().map(|argument| expr(*argument)).collect(),
            span: shifted(*span, span_base),
        },
        SurfaceRequest::BytesOf { content, span } => SurfaceRequest::BytesOf {
            content: expr(*content),
            span: shifted(*span, span_base),
        },
    }
}

fn remap_batch_member(
    member: &SurfaceBatchMember,
    remapped: &IndexMap<SurfaceExprId, SurfaceExprId>,
    remapped_types: &IndexMap<SurfaceTypeId, SurfaceTypeId>,
    span_base: u32,
) -> SurfaceBatchMember {
    let expr = |id: SurfaceExprId| remap_id(id, remapped);
    SurfaceBatchMember {
        head: member.head.map(|id| remap_id(id, remapped_types)),
        arguments: member
            .arguments
            .iter()
            .map(|argument| expr(*argument))
            .collect(),
        span: shifted(member.span, span_base),
    }
}

fn remap_value(
    value: &SurfaceValue,
    remapped: &IndexMap<SurfaceExprId, SurfaceExprId>,
    remapped_types: &IndexMap<SurfaceTypeId, SurfaceTypeId>,
    span_base: u32,
) -> SurfaceValue {
    match value {
        SurfaceValue::Expression(id) => SurfaceValue::Expression(remap_id(*id, remapped)),
        SurfaceValue::Request(request) => {
            SurfaceValue::Request(remap_request(request, remapped, remapped_types, span_base))
        }
    }
}

fn remap_rule_body(
    body: &SurfaceRuleBody,
    remapped_exprs: &IndexMap<SurfaceExprId, SurfaceExprId>,
    remapped_types: &IndexMap<SurfaceTypeId, SurfaceTypeId>,
    span_base: u32,
) -> SurfaceRuleBody {
    match body {
        SurfaceRuleBody::Host => SurfaceRuleBody::Host,
        SurfaceRuleBody::Written(written) => SurfaceRuleBody::Written(Box::new(remap_written(
            written,
            remapped_exprs,
            remapped_types,
            span_base,
        ))),
    }
}

fn remap_written(
    written: &SurfaceWrittenBody,
    remapped_exprs: &IndexMap<SurfaceExprId, SurfaceExprId>,
    remapped_types: &IndexMap<SurfaceTypeId, SurfaceTypeId>,
    span_base: u32,
) -> SurfaceWrittenBody {
    SurfaceWrittenBody {
        statements: written
            .statements
            .iter()
            .map(|statement| SurfaceStatement {
                binder: remap_binder(&statement.binder, span_base),
                annotation: statement.annotation.map(|id| remap_id(id, remapped_types)),
                value: remap_value(&statement.value, remapped_exprs, remapped_types, span_base),
                span: shifted(statement.span, span_base),
            })
            .collect(),
        tail: written
            .tail
            .as_ref()
            .map(|tail| remap_value(tail, remapped_exprs, remapped_types, span_base)),
        span: shifted(written.span, span_base),
    }
}

fn remap_binder(binder: &crate::body::SurfaceBinder, span_base: u32) -> crate::body::SurfaceBinder {
    use crate::body::SurfaceBinder;
    match binder {
        SurfaceBinder::Name { name, span } => SurfaceBinder::Name {
            name: name.clone(),
            span: shifted(*span, span_base),
        },
        SurfaceBinder::Group { names, span } => SurfaceBinder::Group {
            names: names
                .iter()
                .map(|name| remap_binder(name, span_base))
                .collect(),
            span: shifted(*span, span_base),
        },
    }
}

fn remap_id<B: pith_arena::Brand>(
    id: pith_arena::Id<B>,
    remapped: &IndexMap<pith_arena::Id<B>, pith_arena::Id<B>>,
) -> pith_arena::Id<B> {
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
