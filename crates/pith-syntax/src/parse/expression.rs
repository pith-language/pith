use super::*;

impl Parser<'_> {
    pub(super) fn expression(&mut self, allow_record: bool) -> Option<SurfaceExprId> {
        self.equality(allow_record)
    }

    fn equality(&mut self, allow_record: bool) -> Option<SurfaceExprId> {
        let mut left = self.additive(allow_record)?;
        loop {
            let operator = match self.peek().kind {
                TokenKind::EqEq => SurfaceOperator::Equal,
                TokenKind::NotEq => SurfaceOperator::NotEqual,
                _ => return Some(left),
            };
            self.take();
            let right = self.additive(allow_record)?;
            let span = Span::new(
                ByteOffset(self.span_of(left).start.0),
                ByteOffset(self.span_of(right).end.0),
            );
            left = self.exprs.push(SurfaceExpr::Binary {
                operator,
                left,
                right,
                span,
            });
        }
    }

    fn additive(&mut self, allow_record: bool) -> Option<SurfaceExprId> {
        let mut left = self.multiplicative(allow_record)?;
        loop {
            let operator = match self.peek().kind {
                TokenKind::Plus => SurfaceOperator::IntAdd,
                TokenKind::Minus => SurfaceOperator::IntSubtract,
                _ => return Some(left),
            };
            self.take();
            let right = self.multiplicative(allow_record)?;
            let span = Span::new(
                ByteOffset(self.span_of(left).start.0),
                ByteOffset(self.span_of(right).end.0),
            );
            left = self.exprs.push(SurfaceExpr::Binary {
                operator,
                left,
                right,
                span,
            });
        }
    }

    fn multiplicative(&mut self, allow_record: bool) -> Option<SurfaceExprId> {
        let mut left = self.postfix(allow_record)?;
        while self.peek().kind == TokenKind::Star {
            self.take();
            let right = self.postfix(allow_record)?;
            let span = Span::new(
                ByteOffset(self.span_of(left).start.0),
                ByteOffset(self.span_of(right).end.0),
            );
            left = self.exprs.push(SurfaceExpr::Binary {
                operator: SurfaceOperator::IntMultiply,
                left,
                right,
                span,
            });
        }
        Some(left)
    }

    pub(super) fn postfix(&mut self, allow_record: bool) -> Option<SurfaceExprId> {
        let mut value = self.atom(allow_record)?;
        while self.peek().kind == TokenKind::Dot {
            self.take();
            let field = self.name("the field's name after the dot")?;
            let span = Span::new(
                ByteOffset(self.span_of(value).start.0),
                ByteOffset(field.span.end.0),
            );
            value = self.exprs.push(SurfaceExpr::Field {
                record: value,
                name: field.text,
                span,
            });
        }
        Some(value)
    }

    fn atom(&mut self, allow_record: bool) -> Option<SurfaceExprId> {
        match (self.peek().kind, self.lexeme(self.peek())) {
            (TokenKind::Int, _) => {
                let token = self.take();
                Some(self.literal(Value::int(int_from(self.lexeme(&token))), token.span))
            }
            (TokenKind::Str, _) => {
                let applied = self
                    .tokens
                    .get(self.position.saturating_add(1))
                    .is_some_and(|next| next.kind == TokenKind::LParen);
                if applied {
                    return self.name_or_construct();
                }
                let token = self.take();
                Some(self.literal(
                    Value::Text(token.text.unwrap_or_else(|| Box::from(""))),
                    token.span,
                ))
            }
            (TokenKind::Ident, "true" | "false") => {
                let token = self.take();
                Some(self.literal(Value::Bool(self.lexeme(&token) == "true"), token.span))
            }
            (TokenKind::LParen, _) => {
                let open = self.take();
                if self.peek().kind == TokenKind::RParen {
                    let close = self.take();
                    return Some(
                        self.literal(Value::Unit, Span::new(open.span.start, close.span.end)),
                    );
                }
                let grouped = self.expression(true)?;
                self.expect(TokenKind::RParen, "`)` closing the grouped expression")?;
                Some(grouped)
            }
            (TokenKind::LBracket, _) => self.list_literal(),
            (TokenKind::LBrace, _) if allow_record => self.record_literal(),
            (TokenKind::Ident, "if") => self.if_expression(),
            (TokenKind::Ident, "match") => self.match_expression(),
            (TokenKind::Ident, "fold") => self.fold_expression(),
            (TokenKind::Ident, "unwrap") => {
                let keyword = self.take();
                let value = self.postfix(true)?;
                Some(self.exprs.push(SurfaceExpr::Unwrap {
                    value,
                    span: Span::new(keyword.span.start, ByteOffset(self.span_of(value).end.0)),
                }))
            }
            (TokenKind::Ident, "ask" | "run" | "bytes") => {
                let token = self.take();
                self.diagnostics.push(error(
                    FrontendCode::UnexpectedToken,
                    token.span,
                    "a request appears only as the whole right-hand side of a `let` or in tail \
                     position",
                    self.source,
                ));
                None
            }
            (TokenKind::Ident, _) => self.name_or_construct(),
            _ => {
                let token = self.take();
                self.diagnostics.push(error(
                    FrontendCode::UnexpectedToken,
                    token.span,
                    format!("expected an expression, found {}", self.describe(&token)),
                    self.source,
                ));
                None
            }
        }
    }

    fn name_or_construct(&mut self) -> Option<SurfaceExprId> {
        let named = self.name("a name")?;
        if self.peek().kind != TokenKind::LParen {
            return Some(self.exprs.push(SurfaceExpr::Name {
                name: named.text,
                span: named.span,
            }));
        }
        let start = named.span.start;
        let arguments = self.arguments()?;
        Some(self.exprs.push(SurfaceExpr::Construct {
            name: named.text,
            arguments,
            span: Span::new(start, ByteOffset(self.previous_end())),
        }))
    }

    fn if_expression(&mut self) -> Option<SurfaceExprId> {
        let keyword = self.take();
        let condition = self.expression(false)?;
        self.branch("`{` after the condition")?;
        let then = self.expression(true)?;
        self.expect(TokenKind::RBrace, "`}` closing the then branch")?;
        self.expect_ident("else", "`else` after the then branch")?;
        let otherwise = if self.peek().kind == TokenKind::Ident && self.lexeme(self.peek()) == "if"
        {
            self.if_expression()?
        } else {
            self.branch("`{` opening the else branch")?;
            let otherwise = self.expression(true)?;
            self.expect(TokenKind::RBrace, "`}` closing the else branch")?;
            otherwise
        };
        Some(self.exprs.push(SurfaceExpr::If {
            condition,
            then,
            otherwise,
            span: Span::new(
                keyword.span.start,
                ByteOffset(self.span_of(otherwise).end.0),
            ),
        }))
    }

    fn match_expression(&mut self) -> Option<SurfaceExprId> {
        let keyword = self.take();
        let scrutinee = self.expression(false)?;
        self.expect(TokenKind::LBrace, "`{` opening the match's arms")?;
        let mut arms = Vec::new();
        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::End {
            arms.push(self.arm()?);
        }
        let closing = self.peek().span;
        self.expect(TokenKind::RBrace, "`}` closing the match's arms")?;
        if arms.is_empty() {
            self.diagnostics.push(error(
                FrontendCode::InvalidBody,
                closing,
                "a match requires at least one arm",
                self.source,
            ));
            return None;
        }
        Some(self.exprs.push(SurfaceExpr::Match {
            scrutinee,
            arms: arms.into(),
            span: Span::new(keyword.span.start, ByteOffset(self.previous_end())),
        }))
    }

    fn arm(&mut self) -> Option<SurfaceArm> {
        let constructor = self.name("a constructor's name in a match arm")?;
        let binder = if self.peek().kind == TokenKind::LParen {
            self.take();
            let binder = self.name("the arm's payload binder")?;
            self.expect(TokenKind::RParen, "`)` after the payload binder")?;
            Some(binder.text)
        } else {
            None
        };
        self.branch("`{` opening the arm")?;
        let body = self.expression(true)?;
        self.expect(TokenKind::RBrace, "`}` closing the arm")?;
        Some(SurfaceArm {
            constructor: constructor.text,
            binder,
            body,
            span: Span::new(constructor.span.start, ByteOffset(self.previous_end())),
        })
    }

    fn fold_expression(&mut self) -> Option<SurfaceExprId> {
        let keyword = self.take();
        let source = self.expression(true)?;
        self.expect_ident(
            "from",
            "`from` between the list and the initial accumulator",
        )?;
        let init = self.expression(true)?;
        self.branch("`{` opening the fold's step")?;
        self.expect(TokenKind::LParen, "`(` opening the step's binders")?;
        let element = self.name("the element's binder")?;
        self.expect(
            TokenKind::Comma,
            "`,` between the element and accumulator binders",
        )?;
        let accumulator = self.name("the accumulator's binder")?;
        self.expect(TokenKind::RParen, "`)` closing the step's binders")?;
        self.expect(TokenKind::Arrow, "`->` before the step")?;
        let step = self.expression(true)?;
        self.expect(TokenKind::RBrace, "`}` closing the fold's step")?;
        Some(self.exprs.push(SurfaceExpr::Fold {
            source,
            init,
            element: element.text,
            accumulator: accumulator.text,
            step,
            span: Span::new(keyword.span.start, ByteOffset(self.previous_end())),
        }))
    }

    fn list_literal(&mut self) -> Option<SurfaceExprId> {
        let open = self.take();
        let mut items = Vec::new();
        if self.peek().kind != TokenKind::RBracket {
            loop {
                items.push(self.expression(true)?);
                if self.peek().kind != TokenKind::Comma {
                    break;
                }
                self.take();
            }
        }
        self.expect(TokenKind::RBracket, "`]` closing the list")?;
        Some(self.exprs.push(SurfaceExpr::List {
            items: items.into(),
            span: Span::new(open.span.start, ByteOffset(self.previous_end())),
        }))
    }

    fn record_literal(&mut self) -> Option<SurfaceExprId> {
        let open = self.take();
        let mut fields = Vec::new();
        if self.peek().kind != TokenKind::RBrace {
            loop {
                let name = self.name("a record field's name")?;
                self.expect(TokenKind::Colon, "`:` after the field's name")?;
                let value = self.expression(true)?;
                fields.push(SurfaceValueField {
                    name: name.text,
                    value,
                    span: name.span,
                });
                if self.peek().kind != TokenKind::Comma {
                    break;
                }
                self.take();
            }
        }
        self.expect(TokenKind::RBrace, "`}` closing the record")?;
        Some(self.exprs.push(SurfaceExpr::Record {
            fields: fields.into(),
            span: Span::new(open.span.start, ByteOffset(self.previous_end())),
        }))
    }

    fn branch(&mut self, expected: &str) -> Option<()> {
        self.expect(TokenKind::LBrace, expected)
    }

    fn literal(&mut self, value: Value, span: Span) -> SurfaceExprId {
        self.exprs.push(SurfaceExpr::Literal { value, span })
    }

    fn span_of(&self, id: SurfaceExprId) -> Span {
        self.exprs.get(id).map_or(Span::none(), expr_span)
    }
}
