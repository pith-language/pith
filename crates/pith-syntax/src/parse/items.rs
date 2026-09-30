//! The declaration items a module holds: imports, type declarations,
//! rules with their written bodies, module locals, entries, and `about`
//! blocks.

use pith_diag::{ByteOffset, Span};
use pith_hir::{
    FrontendCode, RuleCategory, SurfaceAbout, SurfaceAboutValue, SurfaceBinder, SurfaceBody,
    SurfaceConstructor, SurfaceDeclaration, SurfaceEntry, SurfaceImport, SurfaceLocal,
    SurfaceParam, SurfaceRequest, SurfaceRule, SurfaceRuleBody, SurfaceStatement, SurfaceTypeId,
    SurfaceValue, SurfaceWrittenBody,
};

use super::Parser;
use crate::lex::{TokenKind, error};

impl Parser<'_> {
    pub(super) fn import(&mut self, documentation: &[Span]) -> Option<SurfaceImport> {
        let keyword = self.take();
        let module = self.name("an imported module's name")?;
        Some(SurfaceImport {
            module: module.text,
            span: keyword.span,
            documentation: documentation.into(),
        })
    }

    pub(super) fn declaration(&mut self, documentation: &[Span]) -> Option<SurfaceDeclaration> {
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

    pub(super) fn constructors(&mut self) -> Option<Box<[SurfaceConstructor]>> {
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

    pub(super) fn rule(
        &mut self,
        documentation: &[Span],
        category: RuleCategory,
    ) -> Option<SurfaceRule> {
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

    pub(super) fn params(&mut self, bound: &mut Vec<Box<str>>) -> Option<Vec<SurfaceParam>> {
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

    pub(super) fn rule_body(
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

    pub(super) fn written_body(
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

    pub(super) fn statement(&mut self, bound: &mut Vec<Box<str>>) -> Option<SurfaceStatement> {
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

    pub(super) fn check_binder_shape(
        &mut self,
        binder: &SurfaceBinder,
        value: &SurfaceValue,
    ) -> Option<()> {
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

    pub(super) fn binder(&mut self) -> Option<SurfaceBinder> {
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

    pub(super) fn check_binder(
        &mut self,
        name: &str,
        span: Span,
        bound: &mut Vec<Box<str>>,
    ) -> Option<()> {
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

    pub(super) fn check_binders(
        &mut self,
        binder: &SurfaceBinder,
        bound: &mut Vec<Box<str>>,
    ) -> Option<()> {
        match binder {
            SurfaceBinder::Name { name, span } => self.check_binder(name, *span, bound),
            SurfaceBinder::Group { names, .. } => names
                .iter()
                .try_fold((), |(), name| self.check_binders(name, bound)),
        }
    }

    pub(super) fn module_local(&mut self, documentation: &[Span]) -> Option<SurfaceLocal> {
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

    pub(super) fn entry(&mut self, documentation: &[Span]) -> Option<SurfaceEntry> {
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

    pub(super) fn about_block(&mut self, documentation: &[Span]) -> Option<SurfaceAbout> {
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

    pub(super) fn about_value(&mut self) -> Option<SurfaceAboutValue> {
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
}
