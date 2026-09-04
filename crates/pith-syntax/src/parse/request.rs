use super::*;

impl Parser<'_> {
    pub(super) fn value(&mut self, expected: Option<SurfaceTypeId>) -> Option<SurfaceValue> {
        match (self.peek().kind, self.lexeme(self.peek())) {
            (TokenKind::Ident, "ask" | "run") | (TokenKind::Ident, "bytes") => {
                Some(SurfaceValue::Request(self.request(expected)?))
            }
            _ => Some(SurfaceValue::Expression(self.expression(true)?)),
        }
    }

    fn request(&mut self, expected: Option<SurfaceTypeId>) -> Option<SurfaceRequest> {
        match self.lexeme(self.peek()) {
            "ask" => self.ask(expected),
            "run" => self.run(expected),
            _ => self.bytes_of(),
        }
    }

    fn ask(&mut self, expected: Option<SurfaceTypeId>) -> Option<SurfaceRequest> {
        let keyword = self.take();
        if self.peek().kind == TokenKind::Ident && self.lexeme(self.peek()) == "all" {
            self.take();
            return self.ask_all(keyword.span.start, expected);
        }
        let head = self.request_head(expected)?;
        let arguments = self.arguments()?;
        Some(SurfaceRequest::Ask {
            head,
            arguments,
            span: Span::new(keyword.span.start, ByteOffset(self.previous_end())),
        })
    }

    fn run(&mut self, expected: Option<SurfaceTypeId>) -> Option<SurfaceRequest> {
        let keyword = self.take();
        let head = self.request_head(expected)?;
        let arguments = self.arguments()?;
        Some(SurfaceRequest::Run {
            head,
            arguments,
            span: Span::new(keyword.span.start, ByteOffset(self.previous_end())),
        })
    }

    fn ask_all(
        &mut self,
        start: ByteOffset,
        expected: Option<SurfaceTypeId>,
    ) -> Option<SurfaceRequest> {
        match self.peek().kind {
            TokenKind::LParen => self.ask_all_batch(start),
            TokenKind::LBracket => {
                let element = self.element_of(expected)?;
                self.ask_each(start, Some(element))
            }
            _ => {
                let head = self.type_expression()?;
                self.ask_each(start, Some(head))
            }
        }
    }

    fn ask_all_batch(&mut self, start: ByteOffset) -> Option<SurfaceRequest> {
        self.take();
        let mut requests = Vec::new();
        loop {
            let member = self.batch_member()?;
            requests.push(member);
            if self.peek().kind != TokenKind::Comma {
                break;
            }
            self.take();
        }
        self.expect(TokenKind::RParen, "`)` closing the batch")?;
        Some(SurfaceRequest::AskAll {
            requests: requests.into(),
            span: Span::new(start, ByteOffset(self.previous_end())),
        })
    }

    fn batch_member(&mut self) -> Option<SurfaceBatchMember> {
        match (self.peek().kind, self.lexeme(self.peek())) {
            (TokenKind::Ident, "ask") => {
                let keyword = self.take();
                let head = self.request_head(None)?;
                let arguments = self.arguments()?;
                Some(SurfaceBatchMember {
                    head,
                    arguments,
                    span: Span::new(keyword.span.start, ByteOffset(self.previous_end())),
                })
            }
            (TokenKind::Ident, "bytes") => {
                let token = self.take();
                self.diagnostics.push(error(
                    FrontendCode::UnexpectedToken,
                    token.span,
                    "`bytes of` cannot be batched; bind it in a separate statement",
                    self.source,
                ));
                None
            }
            (TokenKind::Ident, "run") => {
                let token = self.take();
                self.diagnostics.push(error(
                    FrontendCode::UnexpectedToken,
                    token.span,
                    "a heterogeneous batch holds pure requests only; `run` is an action request",
                    self.source,
                ));
                None
            }
            _ => {
                let token = self.take();
                self.diagnostics.push(error(
                    FrontendCode::UnexpectedToken,
                    token.span,
                    format!(
                        "expected `ask` in the batch, found {}",
                        self.describe(&token)
                    ),
                    self.source,
                ));
                None
            }
        }
    }

    fn ask_each(
        &mut self,
        start: ByteOffset,
        head: Option<SurfaceTypeId>,
    ) -> Option<SurfaceRequest> {
        self.expect(TokenKind::LBracket, "`[` opening the comprehension")?;
        self.expect_ident("for", "`for` opening the comprehension")?;
        let binder = self.name("the comprehension's element binder")?;
        self.expect_ident("in", "`in` after the element's binder")?;
        let source = self.expression(true)?;
        self.expect(TokenKind::LBrace, "`{` opening the comprehension's clauses")?;
        let mut clauses = Vec::new();
        let mut bound = vec![binder.text.clone()];
        if self.peek().kind != TokenKind::RBrace {
            loop {
                let clause = self.clause(&mut bound)?;
                clauses.push(clause);
                if self.peek().kind != TokenKind::Pipe {
                    break;
                }
                self.take();
            }
        }
        self.expect(TokenKind::RBrace, "`}` closing the comprehension's clauses")?;
        let arguments = self.arguments()?;
        self.expect(TokenKind::RBracket, "`]` closing the comprehension")?;
        Some(SurfaceRequest::AskEach {
            head,
            binder: binder.text,
            source,
            clauses: clauses.into(),
            arguments,
            span: Span::new(start, ByteOffset(self.previous_end())),
        })
    }

    fn clause(&mut self, bound: &mut Vec<Box<str>>) -> Option<SurfaceClause> {
        match (self.peek().kind, self.lexeme(self.peek())) {
            (TokenKind::Ident, "if") => {
                let keyword = self.take();
                let condition = self.expression(true)?;
                if bound.len() > 1 {
                    self.diagnostics.push(error(
                        FrontendCode::FilterAfterBinding,
                        Span::new(keyword.span.start, ByteOffset(self.previous_end())),
                        "a filter cannot follow a binding; move it before the bindings",
                        self.source,
                    ));
                    return None;
                }
                Some(SurfaceClause::Filter {
                    condition,
                    span: Span::new(keyword.span.start, ByteOffset(self.previous_end())),
                })
            }
            (TokenKind::Ident, "let") => {
                let keyword = self.take();
                let name = self.name("the clause's binder")?;
                if bound.iter().any(|existing| **existing == *name.text) {
                    self.diagnostics.push(error(
                        FrontendCode::DuplicateBinder,
                        name.span,
                        format!("the comprehension binds `{}` twice", name.text),
                        self.source,
                    ));
                    return None;
                }
                bound.push(name.text.clone());
                self.expect(TokenKind::Eq, "`=` in a comprehension clause")?;
                let value = self.expression(true)?;
                Some(SurfaceClause::Let {
                    name: name.text,
                    value,
                    span: Span::new(keyword.span.start, ByteOffset(self.previous_end())),
                })
            }
            _ => {
                let token = self.take();
                self.diagnostics.push(error(
                    FrontendCode::UnexpectedToken,
                    token.span,
                    format!(
                        "expected `if` or `let` in a comprehension's clauses, found {}",
                        self.describe(&token)
                    ),
                    self.source,
                ));
                None
            }
        }
    }

    fn bytes_of(&mut self) -> Option<SurfaceRequest> {
        let keyword = self.take();
        self.expect_ident("of", "`of` after `bytes`")?;
        let content = self.postfix(true)?;
        Some(SurfaceRequest::BytesOf {
            content,
            span: Span::new(keyword.span.start, ByteOffset(self.previous_end())),
        })
    }

    /// The head type a request reads, elided onto the type of the position
    /// that checks it when the spelling omits one.
    fn request_head(&mut self, expected: Option<SurfaceTypeId>) -> Option<Option<SurfaceTypeId>> {
        if self.peek().kind == TokenKind::LParen {
            return match expected {
                Some(expected) => Some(Some(expected)),
                None => {
                    self.diagnostics.push(error(
                        FrontendCode::HeadlessRequest,
                        self.peek().span,
                        "a request with no head type reads it from the position that checks it; \
                         write the type, or annotate the `let`",
                        self.source,
                    ));
                    None
                }
            };
        }
        Some(Some(self.type_expression()?))
    }

    /// The element type a comprehension's annotation carries, which is the
    /// only spelling an elided `ask all [ … ]` can read.
    fn element_of(&mut self, expected: Option<SurfaceTypeId>) -> Option<SurfaceTypeId> {
        let Some(expected) = expected else {
            self.diagnostics.push(error(
                FrontendCode::HeadlessRequest,
                self.peek().span,
                "a comprehension with no head type reads it from a `List<…>` annotation on the \
                 position that checks it",
                self.source,
            ));
            return None;
        };
        let element = match self.types.get(expected) {
            Some(SurfaceTypeNode::List(element)) => Some(*element),
            _ => None,
        };
        match element {
            Some(element) => Some(element),
            None => {
                self.diagnostics.push(error(
                    FrontendCode::HeadlessRequest,
                    self.peek().span,
                    "eliding a comprehension's head type needs a `List<…>` annotation to read it \
                     from",
                    self.source,
                ));
                None
            }
        }
    }

    pub(super) fn arguments(&mut self) -> Option<Box<[SurfaceExprId]>> {
        self.expect(TokenKind::LParen, "`(` opening the request's arguments")?;
        let mut arguments = Vec::new();
        if self.peek().kind != TokenKind::RParen {
            loop {
                arguments.push(self.expression(true)?);
                if self.peek().kind != TokenKind::Comma {
                    break;
                }
                self.take();
            }
        }
        self.expect(TokenKind::RParen, "`)` closing the request's arguments")?;
        Some(arguments.into())
    }
}
