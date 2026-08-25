use core::range::Range;

use pith_arena::define_arena;
use pith_diag::Span;

use crate::RuleCategory;

define_arena!(
    SurfaceTypeId,
    SurfaceTypeArena,
    SurfaceTypeBrand,
    "An id for one type node in a parsed module surface's arena."
);

pub struct ParsedSurface {
    pub types: SurfaceTypeArena<SurfaceTypeNode>,
    pub fields: Vec<SurfaceField>,
    pub imports: Box<[SurfaceImport]>,
    pub declarations: Box<[SurfaceDeclaration]>,
    pub rules: Box<[SurfaceRule]>,
}

pub struct SurfaceField {
    pub name: Box<str>,
    pub payload: SurfaceTypeId,
    pub span: Span,
}

pub struct SurfaceImport {
    pub module: Box<str>,
    pub span: Span,
}

pub enum SurfaceBody {
    Nominal(SurfaceTypeId),
    Sum(Box<[SurfaceConstructor]>),
    Alias(SurfaceTypeId),
}

pub struct SurfaceDeclaration {
    pub name: Box<str>,
    pub name_span: Span,
    pub body: SurfaceBody,
    pub documentation: Box<[Span]>,
}

pub struct SurfaceConstructor {
    pub name: Box<str>,
    pub payload: Option<SurfaceTypeId>,
    pub span: Span,
}

#[derive(Clone)]
pub enum SurfaceTypeNode {
    Unit,
    Bool,
    Int,
    Text,
    Bytes,
    Blob,
    List(SurfaceTypeId),
    Record {
        fields: Range<u32>,
    },
    Reference {
        module: Option<Box<str>>,
        name: Box<str>,
        span: Span,
    },
}

pub struct SurfaceRule {
    pub label: Box<str>,
    pub label_span: Span,
    pub inputs: Box<[SurfaceTypeId]>,
    pub output: SurfaceTypeId,
    pub category: RuleCategory,
    pub span: Span,
    pub documentation: Box<[Span]>,
}
