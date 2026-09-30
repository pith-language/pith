//! The parser's shared machinery: naming a token, expecting one, error
//! recovery between items, and the token cursor itself.

use pith_core::Int;
use pith_diag::{ByteOffset, Span};
use pith_hir::{FrontendCode, SurfaceExpr};

use super::{Named, Parser};
use crate::lex::{Token, TokenKind, error};

impl Parser<'_> {
    pub(super) fn at_named_position(&mut self) -> Option<Named> {
        let nameable = matches!(self.peek().kind, TokenKind::Str)
            || (self.peek().kind == TokenKind::Ident && !is_keyword(self.lexeme(self.peek())));
        let followed_by_colon = self
            .tokens
            .get(self.position.saturating_add(1))
            .is_some_and(|next| next.kind == TokenKind::Colon);
        (nameable && followed_by_colon).then(|| self.take_named())
    }

    pub(super) fn take_named(&mut self) -> Named {
        let token = self.take();
        Named {
            text: match token.kind {
                TokenKind::Str => token.text.unwrap_or_else(|| Box::from("")),
                _ => self.lexeme(&token).into(),
            },
            span: token.span,
        }
    }
    pub(super) fn name(&mut self, expected: &str) -> Option<Named> {
        match self.peek().kind {
            TokenKind::Str => Some(self.take_named()),
            TokenKind::Ident if !is_keyword(self.lexeme(self.peek())) => Some(self.take_named()),
            _ => {
                let token = self.take();
                self.diagnostics.push(error(
                    FrontendCode::UnexpectedToken,
                    token.span,
                    format!("expected {expected}, found {}", self.describe(&token)),
                    self.source,
                ));
                None
            }
        }
    }

    pub(super) fn expect(&mut self, kind: TokenKind, expected: &str) -> Option<()> {
        if self.peek().kind == kind {
            self.take();
            return Some(());
        }
        self.expected(expected)
    }

    pub(super) fn expect_ident(&mut self, spelling: &str, expected: &str) -> Option<()> {
        if self.peek().kind == TokenKind::Ident && self.lexeme(self.peek()) == spelling {
            self.take();
            return Some(());
        }
        self.expected(expected)
    }

    pub(super) fn expected(&mut self, expected: &str) -> Option<()> {
        let token = self.take();
        self.diagnostics.push(error(
            FrontendCode::UnexpectedToken,
            token.span,
            format!("expected {expected}, found {}", self.describe(&token)),
            self.source,
        ));
        None
    }

    pub(super) fn skip_to_item(&mut self) {
        while self.peek().kind != TokenKind::End {
            if self.peek().kind == TokenKind::LineComment || self.is_item_start() {
                return;
            }
            self.take();
        }
    }

    pub(super) fn skip_to_statement(&mut self) {
        while self.peek().kind != TokenKind::End {
            let at_statement =
                self.peek().kind == TokenKind::Ident && self.lexeme(self.peek()) == "let";
            if self.peek().kind == TokenKind::RBrace || at_statement {
                return;
            }
            self.take();
        }
    }

    pub(super) fn is_item_start(&self) -> bool {
        self.peek().kind == TokenKind::Ident
            && matches!(
                self.lexeme(self.peek()),
                "import"
                    | "nominal"
                    | "sum"
                    | "type"
                    | "pure"
                    | "action"
                    | "let"
                    | "entry"
                    | "about"
            )
    }

    /// The span of the token just taken, for a production reporting on
    /// everything it consumed.
    pub(super) fn previous_span(&self) -> Span {
        self.tokens
            .get(self.position.saturating_sub(1))
            .map_or_else(|| Span::point(ByteOffset(0)), |token| token.span)
    }

    pub(super) fn previous_end(&self) -> u32 {
        self.tokens
            .get(self.position.saturating_sub(1))
            .map_or(0, |token| token.span.end.0)
    }

    pub(super) fn peek(&self) -> &Token {
        self.tokens
            .get(self.position)
            .unwrap_or_else(|| unreachable!("the lexer terminates the token stream"))
    }

    pub(super) fn take(&mut self) -> Token {
        let token = self.peek().clone();
        if token.kind != TokenKind::End {
            self.position = self.position.saturating_add(1);
        }
        token
    }

    pub(super) fn lexeme(&self, token: &Token) -> &str {
        let start = usize::try_from(token.span.start.0).unwrap_or(0);
        let end = usize::try_from(token.span.end.0).unwrap_or(0);
        self.source.source_text().get(start..end).unwrap_or("")
    }

    pub(super) fn describe(&self, token: &Token) -> String {
        match token.kind {
            TokenKind::End => "the end of the file".to_owned(),
            TokenKind::Ident | TokenKind::Str => format!(
                "`{}`",
                token.text.as_deref().unwrap_or_else(|| self.lexeme(token))
            ),
            TokenKind::Int => format!("`{}`", self.lexeme(token)),
            TokenKind::LineComment => "a comment".to_owned(),
            _ => format!("`{}`", punctuation(token.kind)),
        }
    }
}

pub(crate) fn expr_span(expr: &SurfaceExpr) -> Span {
    match expr {
        SurfaceExpr::Literal { span, .. }
        | SurfaceExpr::Name { span, .. }
        | SurfaceExpr::Field { span, .. }
        | SurfaceExpr::Record { span, .. }
        | SurfaceExpr::List { span, .. }
        | SurfaceExpr::Construct { span, .. }
        | SurfaceExpr::Unwrap { span, .. }
        | SurfaceExpr::If { span, .. }
        | SurfaceExpr::Match { span, .. }
        | SurfaceExpr::Fold { span, .. }
        | SurfaceExpr::Binary { span, .. } => *span,
    }
}

pub(crate) fn int_from(digits: &str) -> Int {
    let ten = Int::from(10_u32);
    digits.bytes().fold(Int::zero(), |accumulator, digit| {
        accumulator
            .multiplied(&ten)
            .added(&Int::from(u32::from(digit.saturating_sub(b'0'))))
    })
}

pub(crate) fn is_keyword(spelling: &str) -> bool {
    crate::lex::KEYWORDS.contains(&spelling)
}

pub(crate) fn punctuation(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Arrow => "->",
        TokenKind::Colon => ":",
        TokenKind::Comma => ",",
        TokenKind::Dot => ".",
        TokenKind::Eq => "=",
        TokenKind::EqEq => "==",
        TokenKind::NotEq => "!=",
        TokenKind::Minus => "-",
        TokenKind::Plus => "+",
        TokenKind::Star => "*",
        TokenKind::Pipe => "|",
        TokenKind::Slash => "/",
        TokenKind::Lt => "<",
        TokenKind::Gt => ">",
        TokenKind::LtEq => "<=",
        TokenKind::GtEq => ">=",
        TokenKind::LParen => "(",
        TokenKind::RParen => ")",
        TokenKind::LBrace => "{",
        TokenKind::RBrace => "}",
        TokenKind::LBracket => "[",
        TokenKind::RBracket => "]",
        TokenKind::Ident
        | TokenKind::Str
        | TokenKind::Int
        | TokenKind::LineComment
        | TokenKind::End => "",
    }
}
