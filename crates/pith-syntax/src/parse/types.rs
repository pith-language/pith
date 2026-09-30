//! The type productions: scalars, lists, records, and references to
//! declared names.

use core::range::Range;
use pith_diag::Span;
use pith_hir::{FrontendCode, SurfaceField, SurfaceTypeId, SurfaceTypeNode};

use super::Parser;
use crate::lex::{TokenKind, error};

impl Parser<'_> {
    pub(super) fn type_expression(&mut self) -> Option<SurfaceTypeId> {
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
}
