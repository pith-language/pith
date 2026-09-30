//! The per-file remapping a merge performs: every id, field range, and
//! span rebased onto the merged module's arena and span space, children
//! before parents so a remapped id always exists when its reader wants it.

use core::range::Range;
use indexmap::IndexMap;
use pith_diag::{ByteOffset, Span};

use crate::body::{
    SurfaceBatchMember, SurfaceClause, SurfaceExpr, SurfaceExprId, SurfaceRequest,
    SurfaceStatement, SurfaceValue, SurfaceWrittenBody,
};
use crate::surface::{
    SurfaceBody, SurfaceConstructor, SurfaceRuleBody, SurfaceTypeId, SurfaceTypeNode,
};

pub(super) fn remap_expr(
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

pub(super) fn remap_request(
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

pub(super) fn remap_batch_member(
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

pub(super) fn remap_value(
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

pub(super) fn remap_rule_body(
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

pub(super) fn remap_written(
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

pub(super) fn remap_binder(
    binder: &crate::body::SurfaceBinder,
    span_base: u32,
) -> crate::body::SurfaceBinder {
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

pub(super) fn remap_id<B: pith_arena::Brand>(
    id: pith_arena::Id<B>,
    remapped: &IndexMap<pith_arena::Id<B>, pith_arena::Id<B>>,
) -> pith_arena::Id<B> {
    remapped
        .get(&id)
        .copied()
        .unwrap_or_else(|| unreachable!("surface children are allocated before their parents"))
}

pub(super) fn remap_node(
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

pub(super) fn remap_body(
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

pub(super) fn shifted(span: Span, base: u32) -> Span {
    Span::new(
        ByteOffset(shifted_offset(span.start.0, base)),
        ByteOffset(shifted_offset(span.end.0, base)),
    )
}

pub(super) fn shifted_offset(offset: u32, base: u32) -> u32 {
    offset
        .checked_add(base)
        .unwrap_or_else(|| unreachable!("the merged module span exceeds u32::MAX bytes"))
}

pub(super) fn next_span_base(current_base: u32, file_length: usize) -> u32 {
    let file_length = u32::try_from(file_length)
        .unwrap_or_else(|_| unreachable!("a source file cannot exceed u32::MAX bytes"));
    // Keep an EOF point span in its own file instead of selecting the next file.
    current_base
        .checked_add(file_length)
        .and_then(|next| next.checked_add(1))
        .unwrap_or_else(|| unreachable!("the merged module span exceeds u32::MAX bytes"))
}

pub(super) fn shift_all(spans: &[Span], base: u32) -> Box<[Span]> {
    spans.iter().map(|span| shifted(*span, base)).collect()
}
