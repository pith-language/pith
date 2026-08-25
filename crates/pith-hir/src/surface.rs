use core::range::Range;

use pith_arena::define_arena;
use pith_diag::Span;

use crate::RuleCategory;
use crate::body::{SurfaceRequest, SurfaceValue, SurfaceWrittenBody};

define_arena!(
    SurfaceTypeId,
    SurfaceTypeArena,
    SurfaceTypeBrand,
    "An id for one type node in a parsed module surface's arena."
);

pub struct ParsedSurface {
    pub types: SurfaceTypeArena<SurfaceTypeNode>,
    pub exprs: crate::body::SurfaceExprArena<crate::body::SurfaceExpr>,
    pub fields: Vec<SurfaceField>,
    pub imports: Box<[SurfaceImport]>,
    pub declarations: Box<[SurfaceDeclaration]>,
    pub rules: Box<[SurfaceRule]>,
    pub locals: Box<[SurfaceLocal]>,
    pub entries: Box<[SurfaceEntry]>,
    pub about: Box<[SurfaceAbout]>,
    /// Every top-level line comment, including comments not documenting a
    /// semantic item.
    pub comments: Box<[SurfaceComment]>,
}

#[derive(Clone, Copy, Debug)]
pub struct SurfaceComment {
    pub span: Span,
    /// The comment starts on the same source line as the preceding token.
    pub trailing: bool,
}

pub struct SurfaceField {
    pub name: Box<str>,
    pub payload: SurfaceTypeId,
    pub span: Span,
}

pub struct SurfaceImport {
    pub module: Box<str>,
    pub span: Span,
    pub documentation: Box<[Span]>,
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
    pub params: Box<[SurfaceParam]>,
    pub output: SurfaceTypeId,
    pub category: RuleCategory,
    pub body: SurfaceRuleBody,
    pub span: Span,
    pub documentation: Box<[Span]>,
}

/// One input of a rule signature. The name documents the position and binds
/// it in a written body; the interface reads only the type, so the name can
/// neither drift into a digest nor move a revision.
pub struct SurfaceParam {
    pub name: Option<(Box<str>, Span)>,
    pub payload: SurfaceTypeId,
}

/// What follows the `=` in a rule declaration.
pub enum SurfaceRuleBody {
    Host,
    Written(Box<SurfaceWrittenBody>),
}

/// A module-private definition: annotated, earlier-in-file-only,
/// non-recursive, and elaborating to a request against its own annotation —
/// a first-order call, not an inlined expansion.
pub struct SurfaceLocal {
    pub name: Box<str>,
    pub name_span: Span,
    pub annotation: SurfaceTypeId,
    pub value: SurfaceValue,
    pub span: Span,
    pub documentation: Box<[Span]>,
}

/// A name bound to a request, invocable only from the root module. The
/// request must keep the computation pure; the caller performs the effect.
pub struct SurfaceEntry {
    pub name: Box<str>,
    pub name_span: Span,
    pub output: SurfaceTypeId,
    pub request: SurfaceRequest,
    pub span: Span,
    pub documentation: Box<[Span]>,
}

/// Module metadata with documentation spans kept outside semantic identity.
#[derive(Clone)]
pub struct SurfaceAbout {
    pub fields: Box<[(Box<str>, SurfaceAboutValue)]>,
    pub span: Span,
    pub documentation: Box<[Span]>,
}

#[derive(Clone, Debug)]
pub enum SurfaceAboutValue {
    Text(Box<str>),
    List(Box<[Box<str>]>),
}
