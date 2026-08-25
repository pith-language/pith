mod expression;
mod request;

use core::range::Range;
use std::sync::Arc;

use pith_core::{Int, Value};
use pith_diag::{ByteOffset, Diag, SourceFile, Span};
use pith_hir::{
    FrontendCode, ParsedSurface, RuleCategory, SurfaceAbout, SurfaceAboutValue, SurfaceArm,
    SurfaceBatchMember, SurfaceBinder, SurfaceBody, SurfaceClause, SurfaceComment,
    SurfaceConstructor, SurfaceDeclaration, SurfaceEntry, SurfaceExpr, SurfaceExprArena,
    SurfaceExprId, SurfaceField, SurfaceImport, SurfaceLocal, SurfaceOperator, SurfaceParam,
    SurfaceRequest, SurfaceRule, SurfaceRuleBody, SurfaceStatement, SurfaceTypeArena,
    SurfaceTypeId, SurfaceTypeNode, SurfaceValue, SurfaceValueField, SurfaceWrittenBody,
};

use crate::lex::{Token, TokenKind, error, lex};

pub fn parse(source: &Arc<SourceFile>) -> (ParsedSurface, Vec<Diag>) {
    let (tokens, mut diagnostics) = lex(source);
    let mut parser = Parser {
        tokens: &tokens,
        position: 0,
        source,
        types: SurfaceTypeArena::new(),
        exprs: SurfaceExprArena::new(),
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
    exprs: SurfaceExprArena<SurfaceExpr>,
    fields: Vec<SurfaceField>,
    diagnostics: Vec<Diag>,
}

impl Parser<'_> {
    fn module(&mut self) -> ParsedSurface {
        let mut imports = Vec::new();
        let mut declarations = Vec::new();
        let mut rules = Vec::new();
        let mut locals = Vec::new();
        let mut entries = Vec::new();
        let mut about = Vec::new();
        let mut comments = Vec::new();
        let mut documentation = Vec::new();
        while self.peek().kind != TokenKind::End {
            let start = self.position;
            if self.peek().kind == TokenKind::LineComment {
                let trailing = self.comment_is_trailing();
                let span = self.take().span;
                comments.push(SurfaceComment { span, trailing });
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
                    self.diagnostics.push(error(
                        FrontendCode::UnexpectedToken,
                        token.span,
                        format!(
                            "expected `import`, `nominal`, `sum`, `type`, `pure rule`, \
                             `action rule`, `let`, `entry`, or `about`, found {}",
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
            exprs: std::mem::take(&mut self.exprs),
            fields: std::mem::take(&mut self.fields),
            imports: imports.into(),
            declarations: declarations.into(),
            rules: rules.into(),
            locals: locals.into(),
            entries: entries.into(),
            about: about.into(),
            comments: comments.into(),
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

    fn import(&mut self, documentation: &[Span]) -> Option<SurfaceImport> {
        let keyword = self.take();
        let module = self.name("an imported module's name")?;
        Some(SurfaceImport {
            module: module.text,
            span: keyword.span,
            documentation: documentation.into(),
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
        let mut bound = Vec::new();
        let params = self.params(&mut bound)?;
        self.expect(TokenKind::RParen, "`)` after the inputs")?;
        self.expect(TokenKind::Arrow, "`->` before the output type")?;
        let output = self.type_expression()?;
        self.expect(TokenKind::Eq, "`=` before the body tier")?;
        let body = match category {
            RuleCategory::Action => {
                self.expect_ident("host", "`host`, the only action rule body")?;
                SurfaceRuleBody::Host
            }
            RuleCategory::Pure => self.rule_body(Some(output), &mut bound)?,
        };
        Some(SurfaceRule {
            label: label.text,
            label_span: label.span,
            params: params.into(),
            output,
            category,
            body,
            span: category_token.span,
            documentation: documentation.into(),
        })
    }

    fn params(&mut self, bound: &mut Vec<Box<str>>) -> Option<Vec<SurfaceParam>> {
        let mut params = Vec::new();
        if self.peek().kind == TokenKind::RParen {
            return Some(params);
        }
        loop {
            let named = self.at_named_position();
            let payload = if let Some(parameter) = named.as_ref() {
                self.check_binder(&parameter.text, parameter.span, bound)?;
                self.expect(TokenKind::Colon, "`:` between a parameter's name and type")?;
                self.type_expression()?
            } else {
                self.type_expression()?
            };
            params.push(SurfaceParam {
                name: named.map(|named| (named.text, named.span)),
                payload,
            });
            if self.peek().kind != TokenKind::Comma {
                break;
            }
            self.take();
        }
        Some(params)
    }

    fn rule_body(
        &mut self,
        output: Option<SurfaceTypeId>,
        bound: &mut Vec<Box<str>>,
    ) -> Option<SurfaceRuleBody> {
        if self.peek().kind == TokenKind::Ident && self.lexeme(self.peek()) == "host" {
            self.take();
            return Some(SurfaceRuleBody::Host);
        }
        Some(SurfaceRuleBody::Written(Box::new(
            self.written_body(output, bound)?,
        )))
    }

    fn written_body(
        &mut self,
        expected: Option<SurfaceTypeId>,
        bound: &mut Vec<Box<str>>,
    ) -> Option<SurfaceWrittenBody> {
        let open = self.take();
        let mut statements = Vec::new();
        let mut tail = None;
        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::End {
            let at_statement =
                self.peek().kind == TokenKind::Ident && self.lexeme(self.peek()) == "let";
            match (at_statement, tail.is_none()) {
                (true, _) => match self.statement(bound) {
                    Some(statement) => statements.push(statement),
                    None => self.skip_to_statement(),
                },
                (false, true) => match self.value(expected) {
                    Some(value) => tail = Some(value),
                    None => self.skip_to_statement(),
                },
                (false, false) => {
                    let token = self.take();
                    self.diagnostics.push(error(
                        FrontendCode::UnexpectedToken,
                        token.span,
                        "`}` closes the body after its tail; only `let` may follow a binding",
                        self.source,
                    ));
                    self.skip_to_statement();
                }
            }
        }
        self.expect(TokenKind::RBrace, "`}` closing the body")?;
        Some(SurfaceWrittenBody {
            statements: statements.into(),
            tail,
            span: Span::new(open.span.start, ByteOffset(self.previous_end())),
        })
    }

    fn statement(&mut self, bound: &mut Vec<Box<str>>) -> Option<SurfaceStatement> {
        let keyword = self.take();
        let binder = self.binder()?;
        self.check_binders(&binder, bound)?;
        let annotation = if self.peek().kind == TokenKind::Colon {
            self.take();
            Some(self.type_expression()?)
        } else {
            None
        };
        self.expect(TokenKind::Eq, "`=` after the binder")?;
        let value = self.value(annotation)?;
        self.check_binder_shape(&binder, &value)?;
        Some(SurfaceStatement {
            binder,
            annotation,
            value,
            span: Span::new(keyword.span.start, ByteOffset(self.previous_end())),
        })
    }

    fn check_binder_shape(&mut self, binder: &SurfaceBinder, value: &SurfaceValue) -> Option<()> {
        let batch = matches!(value, SurfaceValue::Request(SurfaceRequest::AskAll { .. }));
        match (binder, batch) {
            (SurfaceBinder::Group { .. }, true) | (SurfaceBinder::Name { .. }, false) => Some(()),
            (SurfaceBinder::Group { span, .. }, false) => {
                self.diagnostics.push(error(
                    FrontendCode::UnexpectedToken,
                    *span,
                    "a binder group pairs with `ask all ( … )`",
                    self.source,
                ));
                None
            }
            (SurfaceBinder::Name { span, .. }, true) => {
                self.diagnostics.push(error(
                    FrontendCode::UnexpectedToken,
                    *span,
                    "a heterogeneous `ask all` binds a parenthesized group of names",
                    self.source,
                ));
                None
            }
        }
    }

    fn binder(&mut self) -> Option<SurfaceBinder> {
        if self.peek().kind != TokenKind::LParen {
            let named = self.name("a binder's name")?;
            return Some(SurfaceBinder::Name {
                name: named.text,
                span: named.span,
            });
        }
        let open = self.take();
        let mut names = Vec::new();
        loop {
            let named = self.name("a binder in the group")?;
            names.push(SurfaceBinder::Name {
                name: named.text,
                span: named.span,
            });
            if self.peek().kind != TokenKind::Comma {
                break;
            }
            self.take();
        }
        self.expect(TokenKind::RParen, "`)` closing the binder group")?;
        Some(SurfaceBinder::Group {
            names: names.into(),
            span: Span::new(open.span.start, ByteOffset(self.previous_end())),
        })
    }

    fn check_binder(&mut self, name: &str, span: Span, bound: &mut Vec<Box<str>>) -> Option<()> {
        let fresh = bound.iter().all(|bound| bound.as_ref() != name);
        if fresh {
            bound.push(Box::from(name));
            return Some(());
        }
        self.diagnostics.push(error(
            FrontendCode::DuplicateBinder,
            span,
            format!("the body binds the name `{name}` twice"),
            self.source,
        ));
        None
    }

    fn check_binders(&mut self, binder: &SurfaceBinder, bound: &mut Vec<Box<str>>) -> Option<()> {
        match binder {
            SurfaceBinder::Name { name, span } => self.check_binder(name, *span, bound),
            SurfaceBinder::Group { names, .. } => names
                .iter()
                .try_fold((), |(), name| self.check_binders(name, bound)),
        }
    }

    fn module_local(&mut self, documentation: &[Span]) -> Option<SurfaceLocal> {
        let keyword = self.take();
        let name = self.name("a local definition's name")?;
        self.expect(
            TokenKind::Colon,
            "`:` and a type: a local definition is annotated",
        )?;
        let annotation = self.type_expression()?;
        self.expect(TokenKind::Eq, "`=` before the definition's value")?;
        let value = self.value(Some(annotation))?;
        if matches!(value, SurfaceValue::Request(SurfaceRequest::AskAll { .. })) {
            self.diagnostics.push(error(
                FrontendCode::UnexpectedToken,
                keyword.span,
                "a definition binds one value; a heterogeneous `ask all` binds a group",
                self.source,
            ));
            return None;
        }
        Some(SurfaceLocal {
            name: name.text,
            name_span: name.span,
            annotation,
            value,
            span: keyword.span,
            documentation: documentation.into(),
        })
    }

    fn entry(&mut self, documentation: &[Span]) -> Option<SurfaceEntry> {
        let keyword = self.take();
        let name = self.name("an entry's name")?;
        self.expect(TokenKind::Colon, "`:` and the entry's type")?;
        let output = self.type_expression()?;
        self.expect(TokenKind::Eq, "`=` before the entry's request")?;
        if self.peek().kind == TokenKind::Ident && self.lexeme(self.peek()) == "run" {
            let token = self.take();
            self.diagnostics.push(error(
                FrontendCode::UnexpectedToken,
                token.span,
                "an entry is bound to a pure request; the caller performs the effect",
                self.source,
            ));
            return None;
        }
        let SurfaceValue::Request(request) = self.value(Some(output))? else {
            self.diagnostics.push(error(
                FrontendCode::UnexpectedToken,
                keyword.span,
                "an entry is bound to a request, not an expression",
                self.source,
            ));
            return None;
        };
        Some(SurfaceEntry {
            name: name.text,
            name_span: name.span,
            output,
            request,
            span: keyword.span,
            documentation: documentation.into(),
        })
    }

    fn about_block(&mut self, documentation: &[Span]) -> Option<SurfaceAbout> {
        let keyword = self.take();
        self.expect(TokenKind::LBrace, "`{` opening the about block")?;
        let mut fields = Vec::new();
        while self.peek().kind != TokenKind::RBrace {
            let key = self.name("an about key")?;
            self.expect(TokenKind::Colon, "`:` between an about key and its value")?;
            let value = self.about_value()?;
            fields.push((key.text, value));
            if self.peek().kind != TokenKind::Comma {
                break;
            }
            self.take();
        }
        self.expect(TokenKind::RBrace, "`}` closing the about block")?;
        Some(SurfaceAbout {
            fields: fields.into(),
            span: keyword.span,
            documentation: documentation.into(),
        })
    }

    fn about_value(&mut self) -> Option<SurfaceAboutValue> {
        match self.peek().kind {
            TokenKind::Str => {
                let token = self.take();
                Some(SurfaceAboutValue::Text(
                    token.text.unwrap_or_else(|| Box::from("")),
                ))
            }
            TokenKind::LBracket => {
                self.take();
                let mut items = Vec::new();
                if self.peek().kind != TokenKind::RBracket {
                    loop {
                        match self.peek().kind {
                            TokenKind::Str => {
                                let token = self.take();
                                items.push(token.text.unwrap_or_else(|| Box::from("")));
                            }
                            _ => {
                                let token = self.take();
                                self.diagnostics.push(error(
                                    FrontendCode::UnexpectedToken,
                                    token.span,
                                    "an about list holds strings",
                                    self.source,
                                ));
                                return None;
                            }
                        }
                        if self.peek().kind != TokenKind::Comma {
                            break;
                        }
                        self.take();
                    }
                }
                self.expect(TokenKind::RBracket, "`]` closing the about list")?;
                Some(SurfaceAboutValue::List(items.into()))
            }
            _ => {
                let token = self.take();
                self.diagnostics.push(error(
                    FrontendCode::UnexpectedToken,
                    token.span,
                    format!(
                        "about values are strings or lists of strings, found {}",
                        self.describe(&token)
                    ),
                    self.source,
                ));
                None
            }
        }
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

    fn at_named_position(&mut self) -> Option<Named> {
        let nameable = matches!(self.peek().kind, TokenKind::Str)
            || (self.peek().kind == TokenKind::Ident && !is_keyword(self.lexeme(self.peek())));
        let followed_by_colon = self
            .tokens
            .get(self.position.saturating_add(1))
            .is_some_and(|next| next.kind == TokenKind::Colon);
        (nameable && followed_by_colon).then(|| self.take_named())
    }

    fn take_named(&mut self) -> Named {
        let token = self.take();
        Named {
            text: match token.kind {
                TokenKind::Str => token.text.unwrap_or_else(|| Box::from("")),
                _ => self.lexeme(&token).into(),
            },
            span: token.span,
        }
    }

    fn name(&mut self, expected: &str) -> Option<Named> {
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

    fn skip_to_statement(&mut self) {
        while self.peek().kind != TokenKind::End {
            let at_statement =
                self.peek().kind == TokenKind::Ident && self.lexeme(self.peek()) == "let";
            if self.peek().kind == TokenKind::RBrace || at_statement {
                return;
            }
            self.take();
        }
    }

    fn is_item_start(&self) -> bool {
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

    fn previous_end(&self) -> u32 {
        self.tokens
            .get(self.position.saturating_sub(1))
            .map_or(0, |token| token.span.end.0)
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
            TokenKind::Int => format!("`{}`", self.lexeme(token)),
            TokenKind::LineComment => "a comment".to_owned(),
            _ => format!("`{}`", punctuation(token.kind)),
        }
    }
}

fn expr_span(expr: &SurfaceExpr) -> Span {
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

fn int_from(digits: &str) -> Int {
    let ten = Int::from(10_u32);
    digits.bytes().fold(Int::zero(), |accumulator, digit| {
        accumulator
            .multiplied(&ten)
            .added(&Int::from(u32::from(digit.saturating_sub(b'0'))))
    })
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
        TokenKind::EqEq => "==",
        TokenKind::NotEq => "!=",
        TokenKind::Minus => "-",
        TokenKind::Plus => "+",
        TokenKind::Star => "*",
        TokenKind::Pipe => "|",
        TokenKind::Lt => "<",
        TokenKind::Gt => ">",
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
