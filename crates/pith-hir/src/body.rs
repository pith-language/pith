use pith_arena::define_arena;
use pith_core::Value;
use pith_diag::Span;

use crate::surface::SurfaceTypeId;

define_arena!(
    SurfaceExprId,
    SurfaceExprArena,
    SurfaceExprBrand,
    "An id for one expression node in a parsed module body's arena."
);

/// One pure expression between yields. Requests are a separate type and can
/// therefore appear only in checking positions.
#[derive(Clone, Debug)]
pub enum SurfaceExpr {
    Literal {
        value: Value,
        span: Span,
    },
    Name {
        name: Box<str>,
        span: Span,
    },
    Field {
        record: SurfaceExprId,
        name: Box<str>,
        span: Span,
    },
    Record {
        fields: Box<[SurfaceValueField]>,
        span: Span,
    },
    List {
        items: Box<[SurfaceExprId]>,
        span: Span,
    },
    /// A declared name applied to arguments: a nominal wrapping its
    /// representation, a sum constructor, or a builtin. Which one it is
    /// decided by name resolution, not by the spelling.
    Construct {
        name: Box<str>,
        arguments: Box<[SurfaceExprId]>,
        span: Span,
    },
    Unwrap {
        value: SurfaceExprId,
        span: Span,
    },
    If {
        condition: SurfaceExprId,
        then: SurfaceExprId,
        otherwise: SurfaceExprId,
        span: Span,
    },
    Match {
        scrutinee: SurfaceExprId,
        arms: Box<[SurfaceArm]>,
        span: Span,
    },
    Fold {
        source: SurfaceExprId,
        init: SurfaceExprId,
        element: Box<str>,
        accumulator: Box<str>,
        step: SurfaceExprId,
        span: Span,
    },
    Binary {
        operator: SurfaceOperator,
        left: SurfaceExprId,
        right: SurfaceExprId,
        span: Span,
    },
}

#[derive(Clone, Debug)]
pub struct SurfaceValueField {
    pub name: Box<str>,
    pub value: SurfaceExprId,
    pub span: Span,
}

/// One arm of a [`SurfaceExpr::Match`]: the constructor it covers, the binder
/// its payload is bound to when it carries one, and the arm's expression.
#[derive(Clone, Debug)]
pub struct SurfaceArm {
    pub constructor: Box<str>,
    pub binder: Option<Box<str>>,
    pub body: SurfaceExprId,
    pub span: Span,
}

/// The operators represented directly by the body IR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceOperator {
    Equal,
    NotEqual,
    IntAdd,
    IntSubtract,
    IntMultiply,
}

/// One request yielded from a checking position.
#[derive(Clone, Debug)]
pub enum SurfaceRequest {
    Ask {
        head: Option<SurfaceTypeId>,
        arguments: Box<[SurfaceExprId]>,
        span: Span,
    },
    Run {
        head: Option<SurfaceTypeId>,
        arguments: Box<[SurfaceExprId]>,
        span: Span,
    },
    /// A heterogeneous batch of pure request forms.
    AskAll {
        requests: Box<[SurfaceBatchMember]>,
        span: Span,
    },
    /// The homogeneous comprehension: one request per element of `source`,
    /// with the declared independence of a batch.
    AskEach {
        head: Option<SurfaceTypeId>,
        binder: Box<str>,
        source: SurfaceExprId,
        clauses: Box<[SurfaceClause]>,
        arguments: Box<[SurfaceExprId]>,
        span: Span,
    },
    BytesOf {
        content: SurfaceExprId,
        span: Span,
    },
}

/// A pure rule request inside a heterogeneous batch. Other request forms are
/// excluded because the body IR batches rule requests only.
#[derive(Clone, Debug)]
pub struct SurfaceBatchMember {
    pub head: Option<SurfaceTypeId>,
    pub arguments: Box<[SurfaceExprId]>,
    pub span: Span,
}

impl SurfaceRequest {
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Ask { span, .. }
            | Self::Run { span, .. }
            | Self::AskAll { span, .. }
            | Self::AskEach { span, .. }
            | Self::BytesOf { span, .. } => *span,
        }
    }
}

/// One clause of a comprehension: a derived binding visible to the request
/// the comprehension builds, or a filter over the elements reached so far.
#[derive(Clone, Debug)]
pub enum SurfaceClause {
    Let {
        name: Box<str>,
        value: SurfaceExprId,
        span: Span,
    },
    Filter {
        condition: SurfaceExprId,
        span: Span,
    },
}

/// What a checking position holds: an expression, or the request it yields
/// to. The separation is what makes the checking-position rule a property of
/// the grammar rather than a convention the parser remembers to check.
pub enum SurfaceValue {
    Expression(SurfaceExprId),
    Request(SurfaceRequest),
}

/// A written rule body: `let` statements, then an optional tail whose value
/// is the body's value.
pub struct SurfaceWrittenBody {
    pub statements: Box<[SurfaceStatement]>,
    pub tail: Option<SurfaceValue>,
    pub span: Span,
}

pub struct SurfaceStatement {
    pub binder: SurfaceBinder,
    pub annotation: Option<SurfaceTypeId>,
    pub value: SurfaceValue,
    pub span: Span,
}

/// What a `let` binds: one name, or the parenthesized binder group a
/// heterogeneous `ask all` resumes under.
pub enum SurfaceBinder {
    Name {
        name: Box<str>,
        span: Span,
    },
    Group {
        names: Box<[SurfaceBinder]>,
        span: Span,
    },
}

impl SurfaceBinder {
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Name { span, .. } | Self::Group { span, .. } => *span,
        }
    }
}
