use core::range::Range;
use std::sync::Arc;

use pith_diag::{Diag, SourceFile, Span};
use pith_hir::{
    FrontendCode, ParsedSurface, RuleCategory, SurfaceBody, SurfaceConstructor, SurfaceDeclaration,
    SurfaceField, SurfaceImport, SurfaceRule, SurfaceTypeArena, SurfaceTypeId, SurfaceTypeNode,
};

use crate::lex::{Token, TokenKind, error, lex};

pub fn parse(source: &Arc<SourceFile>) -> (ParsedSurface, Vec<Diag>) {
    let (tokens, mut diagnostics) = lex(source);
    let mut parser = Parser {
        tokens: &tokens,
        position: 0,
        source,
        types: SurfaceTypeArena::new(),
        fields: Vec::new(),
        diagnostics: Vec::new(),
    };
    let surface = parser.module();
    diagnostics.append(&mut parser.diagnostics);
    (surface, diagnostics)
}

struct Named {
    text: Box<str>,
    span: Span,
}

struct Parser<'source> {
    tokens: &'source [Token],
    position: usize,
    source: &'source Arc<SourceFile>,
    types: SurfaceTypeArena<SurfaceTypeNode>,
    fields: Vec<SurfaceField>,
    diagnostics: Vec<Diag>,
}

impl Parser<'_> {
    fn module(&mut self) -> ParsedSurface {
        let mut imports = Vec::new();
        let mut declarations = Vec::new();
        let mut rules = Vec::new();
        let mut documentation = Vec::new();
        while self.peek().kind != TokenKind::End {
            let start = self.position;
            if self.peek().kind == TokenKind::LineComment {
                documentation.push(self.take().span);
                continue;
            }
            let outcome = match (self.peek().kind, self.lexeme(self.peek())) {
                (TokenKind::Ident, "import") => self.import().map(|item| imports.push(item)),
                (TokenKind::Ident, "nominal" | "sum" | "type") => self
                    .declaration(&documentation)
                    .map(|item| declarations.push(item)),
                (TokenKind::Ident, "pure") => self
                    .rule(&documentation, RuleCategory::Pure)
                    .map(|item| rules.push(item)),
                (TokenKind::Ident, "action") => self
                    .rule(&documentation, RuleCategory::Action)
                    .map(|item| rules.push(item)),
                _ => {
                    let token = self.take();
                    self.diagnostics.push(error(
                        FrontendCode::UnexpectedToken,
                        token.span,
                        format!(
                            "expected `import`, `nominal`, `sum`, `type`, `pure rule`, or \
                             `action rule`, found {}",
                            self.describe(&token)
                        ),
                        self.source,
                    ));
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
        ParsedSurface {
            types: std::mem::take(&mut self.types),
            fields: std::mem::take(&mut self.fields),
            imports: imports.into(),
            declarations: declarations.into(),
            rules: rules.into(),
        }
    }

    fn import(&mut self) -> Option<SurfaceImport> {
        let keyword = self.take();
        let module = self.name("an imported module's name")?;
        Some(SurfaceImport {
            module: module.text,
            span: keyword.span,
        })
    }

    fn declaration(&mut self, documentation: &[Span]) -> Option<SurfaceDeclaration> {
        let keyword = self.take();
        let kind = self.lexeme(&keyword).to_owned();
        let name = self.name("a declaration's name")?;
        self.expect(TokenKind::Eq, "`=`")?;
        let body = match kind.as_str() {
            "nominal" => SurfaceBody::Nominal(self.type_expression()?),
            "sum" => SurfaceBody::Sum(self.constructors()?),
            "type" => SurfaceBody::Alias(self.type_expression()?),
            _ => return None,
        };
        Some(SurfaceDeclaration {
            name: name.text,
            name_span: name.span,
            body,
            documentation: documentation.into(),
        })
    }

    fn constructors(&mut self) -> Option<Box<[SurfaceConstructor]>> {
        if self.peek().kind == TokenKind::Pipe {
            self.take();
        }
        let mut constructors = Vec::new();
        loop {
            let name = self.name("a constructor's name")?;
            let payload = if self.peek().kind == TokenKind::LParen {
                self.take();
                let payload = self.type_expression()?;
                self.expect(TokenKind::RParen, "`)` after the payload")?;
                Some(payload)
            } else {
                None
            };
            constructors.push(SurfaceConstructor {
                name: name.text,
                payload,
                span: name.span,
            });
            if self.peek().kind != TokenKind::Pipe {
                break;
            }
            self.take();
        }
        Some(constructors.into())
    }

    fn rule(&mut self, documentation: &[Span], category: RuleCategory) -> Option<SurfaceRule> {
        let category_token = self.take();
        self.expect_ident("rule", "`rule` after the effect category")?;
        let label = self.name("a rule's label")?;
        self.expect(TokenKind::LParen, "`(` after the rule's label")?;
        let mut inputs = Vec::new();
        if self.peek().kind != TokenKind::RParen {
            loop {
                inputs.push(self.type_expression()?);
                if self.peek().kind != TokenKind::Comma {
                    break;
                }
                self.take();
            }
        }
        self.expect(TokenKind::RParen, "`)` after the inputs")?;
        self.expect(TokenKind::Arrow, "`->` before the output type")?;
        let output = self.type_expression()?;
        self.expect(TokenKind::Eq, "`=` before the body tier")?;
        self.expect_ident("host", "`host` as the body tier")?;
        Some(SurfaceRule {
            label: label.text,
            label_span: label.span,
            inputs: inputs.into(),
            output,
            category,
            span: category_token.span,
            documentation: documentation.into(),
        })
    }

    fn type_expression(&mut self) -> Option<SurfaceTypeId> {
        match (self.peek().kind, self.lexeme(self.peek())) {
            (TokenKind::Ident, "Unit") => Some(self.scalar(SurfaceTypeNode::Unit)),
            (TokenKind::Ident, "Bool") => Some(self.scalar(SurfaceTypeNode::Bool)),
            (TokenKind::Ident, "Int") => Some(self.scalar(SurfaceTypeNode::Int)),
            (TokenKind::Ident, "Text") => Some(self.scalar(SurfaceTypeNode::Text)),
            (TokenKind::Ident, "Bytes") => Some(self.scalar(SurfaceTypeNode::Bytes)),
            (TokenKind::Ident, "Blob") => Some(self.scalar(SurfaceTypeNode::Blob)),
            (TokenKind::Ident, "List") => {
                self.take();
                self.expect(TokenKind::Lt, "`<` after `List`")?;
                let element = self.type_expression()?;
                self.expect(TokenKind::Gt, "`>` closing the list")?;
                Some(self.types.push(SurfaceTypeNode::List(element)))
            }
            (TokenKind::Ident | TokenKind::Str, _) => self.reference(),
            (TokenKind::LBrace, _) => self.record(),
            _ => {
                let token = self.take();
                self.diagnostics.push(error(
                    FrontendCode::UnexpectedToken,
                    token.span,
                    format!("expected a type, found {}", self.describe(&token)),
                    self.source,
                ));
                None
            }
        }
    }

    fn scalar(&mut self, node: SurfaceTypeNode) -> SurfaceTypeId {
        self.take();
        self.types.push(node)
    }

    fn record(&mut self) -> Option<SurfaceTypeId> {
        self.take();
        let mut fields = Vec::new();
        if self.peek().kind != TokenKind::RBrace {
            loop {
                let name = self.name("a record field's name")?;
                self.expect(TokenKind::Colon, "`:` after the field's name")?;
                let payload = self.type_expression()?;
                fields.push(SurfaceField {
                    name: name.text,
                    payload,
                    span: name.span,
                });
                if self.peek().kind != TokenKind::Comma {
                    break;
                }
                self.take();
            }
        }
        self.expect(TokenKind::RBrace, "`}` closing the record")?;
        let fields_from = u32::try_from(self.fields.len()).unwrap_or(u32::MAX);
        self.fields.extend(fields);
        let fields_to = u32::try_from(self.fields.len()).unwrap_or(u32::MAX);
        Some(self.types.push(SurfaceTypeNode::Record {
            fields: Range {
                start: fields_from,
                end: fields_to,
            },
        }))
    }

    fn reference(&mut self) -> Option<SurfaceTypeId> {
        let first = self.name("a type name")?;
        if self.peek().kind == TokenKind::Dot {
            self.take();
            let name = self.name("the declared name after the dot")?;
            let span = Span::new(first.span.start, name.span.end);
            return Some(self.types.push(SurfaceTypeNode::Reference {
                module: Some(first.text),
                name: name.text,
                span,
            }));
        }
        Some(self.types.push(SurfaceTypeNode::Reference {
            module: None,
            name: first.text,
            span: first.span,
        }))
    }

    fn name(&mut self, expected: &str) -> Option<Named> {
        match self.peek().kind {
            TokenKind::Str => {
                let token = self.take();
                Some(Named {
                    text: token.text.unwrap_or_else(|| Box::from("")),
                    span: token.span,
                })
            }
            TokenKind::Ident if !is_keyword(self.lexeme(self.peek())) => {
                let token = self.take();
                Some(Named {
                    text: self.lexeme(&token).into(),
                    span: token.span,
                })
            }
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

    fn expect(&mut self, kind: TokenKind, expected: &str) -> Option<()> {
        if self.peek().kind == kind {
            self.take();
            return Some(());
        }
        self.expected(expected)
    }

    fn expect_ident(&mut self, spelling: &str, expected: &str) -> Option<()> {
        if self.peek().kind == TokenKind::Ident && self.lexeme(self.peek()) == spelling {
            self.take();
            return Some(());
        }
        self.expected(expected)
    }

    fn expected(&mut self, expected: &str) -> Option<()> {
        let token = self.take();
        self.diagnostics.push(error(
            FrontendCode::UnexpectedToken,
            token.span,
            format!("expected {expected}, found {}", self.describe(&token)),
            self.source,
        ));
        None
    }

    fn skip_to_item(&mut self) {
        while self.peek().kind != TokenKind::End {
            if self.peek().kind == TokenKind::LineComment || self.is_item_start() {
                return;
            }
            self.take();
        }
    }

    fn is_item_start(&self) -> bool {
        self.peek().kind == TokenKind::Ident
            && matches!(
                self.lexeme(self.peek()),
                "import" | "nominal" | "sum" | "type" | "pure" | "action"
            )
    }

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.position)
            .unwrap_or_else(|| unreachable!("the lexer terminates the token stream"))
    }

    fn take(&mut self) -> Token {
        let token = self.peek().clone();
        if token.kind != TokenKind::End {
            self.position = self.position.saturating_add(1);
        }
        token
    }

    fn lexeme(&self, token: &Token) -> &str {
        let start = usize::try_from(token.span.start.0).unwrap_or(0);
        let end = usize::try_from(token.span.end.0).unwrap_or(0);
        self.source.source_text().get(start..end).unwrap_or("")
    }

    fn describe(&self, token: &Token) -> String {
        match token.kind {
            TokenKind::End => "the end of the file".to_owned(),
            TokenKind::Ident | TokenKind::Str => format!(
                "`{}`",
                token.text.as_deref().unwrap_or_else(|| self.lexeme(token))
            ),
            TokenKind::LineComment => "a comment".to_owned(),
            _ => format!("`{}`", punctuation(token.kind)),
        }
    }
}

fn is_keyword(spelling: &str) -> bool {
    crate::lex::KEYWORDS.contains(&spelling)
}

fn punctuation(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Arrow => "->",
        TokenKind::Colon => ":",
        TokenKind::Comma => ",",
        TokenKind::Dot => ".",
        TokenKind::Eq => "=",
        TokenKind::Minus => "-",
        TokenKind::Pipe => "|",
        TokenKind::Lt => "<",
        TokenKind::Gt => ">",
        TokenKind::LParen => "(",
        TokenKind::RParen => ")",
        TokenKind::LBrace => "{",
        TokenKind::RBrace => "}",
        TokenKind::Ident | TokenKind::Str | TokenKind::LineComment | TokenKind::End => "",
    }
}
