mod expression;
mod helpers;
mod items;
mod locator;
mod project;
mod request;
mod types;

pub(crate) use helpers::{expr_span, int_from};

use std::sync::Arc;

use pith_core::Value;
use pith_diag::{ByteOffset, Diag, SourceFile, Span};
use pith_hir::{
    DocumentHeader, FrontendCode, ParsedSurface, RuleCategory, SurfaceAbout, SurfaceArm,
    SurfaceBatchMember, SurfaceClause, SurfaceComment, SurfaceDeclaration, SurfaceEntry,
    SurfaceExpr, SurfaceExprArena, SurfaceExprId, SurfaceField, SurfaceImport, SurfaceLocal,
    SurfaceOperator, SurfaceRequest, SurfaceRule, SurfaceTypeArena, SurfaceTypeId, SurfaceTypeNode,
    SurfaceValue, SurfaceValueField,
};

use crate::lex::{Document, Token, TokenKind, error, lex, lex_from};

/// The declarations of an included file: no header, every clause of the
/// declaration grammar.
pub fn parse(source: &Arc<SourceFile>) -> (ParsedSurface, Vec<Diag>) {
    let (tokens, mut diagnostics) = lex(source);
    let mut parser = Parser {
        tokens: &tokens,
        position: 0,
        source,
        items_of: ItemsOf::Include,
        types: SurfaceTypeArena::new(),
        exprs: SurfaceExprArena::new(),
        fields: Vec::new(),
        diagnostics: Vec::new(),
        comments: Vec::new(),
    };
    let mut items = parser.items();
    diagnostics.append(&mut parser.diagnostics);
    items.header = DocumentHeader::Include;
    (items.surface(), diagnostics)
}

/// The project document a project file spells: its header clauses, then the
/// same declaration grammar an include holds.
///
/// The header is lexed in its own region, where subjects and hyphenated
/// names are single identifiers and `<=`/`>=` single tokens; the
/// declarations are lexed exactly as any other file's, so the same body
/// text parses the same way wherever it sits.
pub fn parse_project(source: &Arc<SourceFile>) -> (ParsedSurface, Vec<Diag>) {
    let (tokens, mut diagnostics) = crate::lex::lex_in(source, Document::Project);
    let mut header_parser = Parser {
        tokens: &tokens,
        position: 0,
        source,
        items_of: ItemsOf::ProjectFile,
        types: SurfaceTypeArena::new(),
        exprs: SurfaceExprArena::new(),
        fields: Vec::new(),
        diagnostics: Vec::new(),
        comments: Vec::new(),
    };
    let (header, boundary) = header_parser.project_header();
    let header_comments = header_parser.comments;
    diagnostics.append(&mut header_parser.diagnostics);
    diagnostics
        .retain(|diagnostic| usize::try_from(diagnostic.span.start.0).unwrap_or(0) <= boundary);

    let (tokens, mut lex_diagnostics) = lex_from(source, Document::Include, boundary);
    diagnostics.append(&mut lex_diagnostics);
    let mut parser = Parser {
        tokens: &tokens,
        position: 0,
        source,
        items_of: ItemsOf::ProjectFile,
        types: SurfaceTypeArena::new(),
        exprs: SurfaceExprArena::new(),
        fields: Vec::new(),
        diagnostics: Vec::new(),
        comments: Vec::new(),
    };
    let mut items = parser.items();
    diagnostics.append(&mut parser.diagnostics);
    items.header = DocumentHeader::Project(Box::new(header));
    items.comments.splice(0..0, header_comments);
    items
        .comments
        .sort_by_key(|comment: &SurfaceComment| comment.span.start.0);
    (items.surface(), diagnostics)
}

/// Whose declarations an item parser is reading: a project file's, where a
/// header clause after the header is misplaced, or an include's, where no
/// header clause may appear at all.
#[derive(Clone, Copy)]
enum ItemsOf {
    ProjectFile,
    Include,
}

/// The collections an item parser fills, assembled into a
/// [`ParsedSurface`] once its header is known.
struct Items {
    header: DocumentHeader,
    types: SurfaceTypeArena<SurfaceTypeNode>,
    exprs: SurfaceExprArena<SurfaceExpr>,
    fields: Vec<SurfaceField>,
    imports: Vec<SurfaceImport>,
    declarations: Vec<SurfaceDeclaration>,
    rules: Vec<SurfaceRule>,
    locals: Vec<SurfaceLocal>,
    entries: Vec<SurfaceEntry>,
    about: Vec<SurfaceAbout>,
    comments: Vec<SurfaceComment>,
}

impl Items {
    fn surface(self) -> ParsedSurface {
        let Self {
            header,
            types,
            exprs,
            fields,
            imports,
            declarations,
            rules,
            locals,
            entries,
            about,
            comments,
        } = self;
        ParsedSurface {
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
        }
    }
}

struct Named {
    text: Box<str>,
    span: Span,
}

struct Parser<'source> {
    tokens: &'source [Token],
    position: usize,
    source: &'source Arc<SourceFile>,
    items_of: ItemsOf,
    types: SurfaceTypeArena<SurfaceTypeNode>,
    exprs: SurfaceExprArena<SurfaceExpr>,
    fields: Vec<SurfaceField>,
    diagnostics: Vec<Diag>,
    /// Top-level comments in the region being parsed. The header production
    /// fills this for its clauses' region; the item parser for its own.
    comments: Vec<SurfaceComment>,
}

impl Parser<'_> {
    fn items(&mut self) -> Items {
        let mut imports = Vec::new();
        let mut declarations = Vec::new();
        let mut rules = Vec::new();
        let mut locals = Vec::new();
        let mut entries = Vec::new();
        let mut about = Vec::new();
        let mut documentation = Vec::new();
        while self.peek().kind != TokenKind::End {
            let start = self.position;
            if self.peek().kind == TokenKind::LineComment {
                let trailing = self.comment_is_trailing();
                let span = self.take().span;
                self.comments.push(SurfaceComment { span, trailing });
                if !trailing {
                    documentation.push(span);
                }
                continue;
            }
            let outcome = match (self.peek().kind, self.lexeme(self.peek())) {
                (TokenKind::Ident, "import") => {
                    self.import(&documentation).map(|item| imports.push(item))
                }
                (TokenKind::Ident, "nominal" | "sum" | "type") => self
                    .declaration(&documentation)
                    .map(|item| declarations.push(item)),
                (TokenKind::Ident, "pure") => self
                    .rule(&documentation, RuleCategory::Pure)
                    .map(|item| rules.push(item)),
                (TokenKind::Ident, "action") => self
                    .rule(&documentation, RuleCategory::Action)
                    .map(|item| rules.push(item)),
                (TokenKind::Ident, "let") => self
                    .module_local(&documentation)
                    .map(|item| locals.push(item)),
                (TokenKind::Ident, "entry") => {
                    self.entry(&documentation).map(|item| entries.push(item))
                }
                (TokenKind::Ident, "about") => self
                    .about_block(&documentation)
                    .map(|item| about.push(item)),
                _ => {
                    let token = self.take();
                    let (code, message) = if token.kind == TokenKind::Ident
                        && crate::lex::HEADER_CLAUSES.contains(&self.lexeme(&token))
                    {
                        (
                            FrontendCode::WrongDocument,
                            match self.items_of {
                                ItemsOf::ProjectFile => format!(
                                    "`{}` opens a header clause, and the header precedes the \
                                     declarations",
                                    self.lexeme(&token)
                                ),
                                ItemsOf::Include => format!(
                                    "`{}` opens a header clause, and an included file holds \
                                     declarations only",
                                    self.lexeme(&token)
                                ),
                            },
                        )
                    } else {
                        (
                            FrontendCode::UnexpectedToken,
                            format!(
                                "expected `import`, `nominal`, `sum`, `type`, `pure rule`, \
                                 `action rule`, `let`, `entry`, or `about`, found {}",
                                self.describe(&token)
                            ),
                        )
                    };
                    self.diagnostics
                        .push(error(code, token.span, message, self.source));
                    Some(())
                }
            };
            if outcome.is_none() {
                self.skip_to_item();
            }
            documentation.clear();
            if self.position == start {
                self.take();
            }
        }
        Items {
            header: DocumentHeader::Include,
            types: std::mem::take(&mut self.types),
            exprs: std::mem::take(&mut self.exprs),
            fields: std::mem::take(&mut self.fields),
            imports,
            declarations,
            rules,
            locals,
            entries,
            about,
            comments: std::mem::take(&mut self.comments),
        }
    }

    fn comment_is_trailing(&self) -> bool {
        let Some(previous) = self
            .position
            .checked_sub(1)
            .and_then(|at| self.tokens.get(at))
        else {
            return false;
        };
        let start = usize::try_from(previous.span.end.0).unwrap_or(0);
        let end = usize::try_from(self.peek().span.start.0).unwrap_or(start);
        self.source
            .source_text()
            .get(start..end)
            .is_some_and(|between| !between.contains('\n'))
    }
}
