//! The five request constructs, and the checking position that holds either
//! a request or an expression.

use pith_hir::{
    SurfaceBatchMember, SurfaceClause, SurfaceExprId, SurfaceRequest, SurfaceTypeId, SurfaceValue,
};

use super::Printer;
use super::expression::{Level, Records};

impl<'a> Printer<'a> {
    /// What a checking position holds: an expression, or the request it
    /// yields to.
    pub(super) fn value(&mut self, value: &SurfaceValue) {
        match value {
            SurfaceValue::Expression(id) => {
                self.expression(*id, Level::Equality, Records::Allowed);
            }
            SurfaceValue::Request(request) => self.request(request),
        }
    }

    pub(super) fn request(&mut self, request: &SurfaceRequest) {
        match request {
            SurfaceRequest::Ask {
                head, arguments, ..
            } => {
                self.out.push_str("ask ");
                self.head(*head);
                self.arguments(arguments.as_ref());
            }
            SurfaceRequest::Run {
                head, arguments, ..
            } => {
                self.out.push_str("run ");
                self.head(*head);
                self.arguments(arguments.as_ref());
            }
            SurfaceRequest::AskAll { requests, .. } => {
                self.out.push_str("ask all (");
                self.joined(requests.as_ref(), ", ", Self::batch_member);
                self.out.push(')');
            }
            SurfaceRequest::AskEach {
                head,
                binder,
                source,
                clauses,
                arguments,
                ..
            } => {
                self.out.push_str("ask all");
                if let Some(head) = *head {
                    self.out.push(' ');
                    self.type_node(head);
                }
                self.out.push_str(" [for ");
                self.name(binder);
                self.out.push_str(" in ");
                self.expression(*source, Level::Equality, Records::Allowed);
                if clauses.is_empty() {
                    self.out.push_str(" {} ");
                } else {
                    self.out.push_str(" { ");
                    self.joined(clauses.as_ref(), " | ", Self::clause);
                    self.out.push_str(" } ");
                }
                self.arguments(arguments.as_ref());
                self.out.push(']');
            }
            SurfaceRequest::BytesOf { content, .. } => {
                self.out.push_str("bytes of ");
                self.expression(*content, Level::Atom, Records::Allowed);
            }
        }
    }

    fn batch_member(&mut self, member: &SurfaceBatchMember) {
        self.out.push_str("ask ");
        self.head(member.head);
        self.arguments(&member.arguments);
    }

    /// The head type a request carries, with the space that follows it.
    fn head(&mut self, head: Option<SurfaceTypeId>) {
        if let Some(head) = head {
            self.type_node(head);
            self.out.push(' ');
        }
    }

    pub(super) fn arguments(&mut self, arguments: &[SurfaceExprId]) {
        self.out.push('(');
        self.joined(arguments, ", ", Self::argument);
        self.out.push(')');
    }

    pub(super) fn argument(&mut self, argument: &SurfaceExprId) {
        self.expression(*argument, Level::Equality, Records::Allowed);
    }

    fn clause(&mut self, clause: &SurfaceClause) {
        match clause {
            SurfaceClause::Let { name, value, .. } => {
                self.out.push_str("let ");
                self.name(name);
                self.out.push_str(" = ");
                self.expression(*value, Level::Equality, Records::Allowed);
            }
            SurfaceClause::Filter { condition, .. } => {
                self.out.push_str("if ");
                self.expression(*condition, Level::Equality, Records::Allowed);
            }
        }
    }
}
