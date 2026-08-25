//! Elaboration of written rule bodies into represented-body IR. This layer
//! resolves names, synthesizes request interfaces, and validates the finished
//! body against its declared interface.

mod expression;

use std::collections::BTreeMap;

use pith_core::{
    BodyExpr, BodyRequest, Interface, MatchArm, NominalType, RecordField, RuleBody, SumType, Type,
    Value,
};
use pith_diag::{Diag, Span};
use pith_hir::{
    FrontendCode, ModuleFiles, ParsedSurface, SurfaceArm, SurfaceBatchMember, SurfaceBinder,
    SurfaceClause, SurfaceExpr, SurfaceExprId, SurfaceOperator, SurfaceRequest, SurfaceStatement,
    SurfaceTypeId, SurfaceValue, SurfaceValueField, SurfaceWrittenBody,
};

/// Names resolved without a module declaration.
pub const BUILTIN_NAMES: &[&str] = &["module", "describe", "append", "concat", "decode"];

const MODULE_BUILTIN: &str = "module";

/// One entry of the binder stack. A binder either names itself, or projects a
/// set of names out of the record value it holds, which is how a
/// comprehension's per-element environment is spelled without a tuple.
struct Binder {
    name: Option<Box<str>>,
    payload: Type,
    projections: Box<[(Box<str>, Type)]>,
}

enum Found {
    Direct(Type),
    Projected(Type),
}

/// One written body's elaboration state. `types` holds every type the body's
/// annotations and request heads name, resolved against the module's
/// declarations before the body elaborates, so name resolution inside the
/// body sees only finished types.
/// Where a request's resumption continues: the statements after it, the
/// body's tail, and the span diagnostics name.
struct Resume<'statements> {
    rest: std::slice::Iter<'statements, SurfaceStatement>,
    tail: Option<&'statements SurfaceValue>,
}

/// The per-element record a binding comprehension iterates.
struct Environment {
    record: Type,
    step: BodyExpr,
    projections: Box<[(Box<str>, Type)]>,
}

/// What one body's elaboration borrows from the module around it.
pub(crate) struct BodySite<'a> {
    pub module: &'a str,
    pub surface: &'a ParsedSurface,
    pub resolved: &'a BTreeMap<&'a str, Type>,
    pub imports: &'a crate::ScopedImports<'a>,
    pub files: &'a ModuleFiles,
}

pub(crate) struct Bodies<'a> {
    site: &'a BodySite<'a>,
    locals: &'a [(&'a str, Type)],
    deferred: &'a [&'a str],
    types: &'a BTreeMap<SurfaceTypeId, Type>,
    own_interface: &'a Interface,
    forbid_self_request: bool,
    diagnostics: &'a mut Vec<Diag>,
    binders: Vec<Binder>,
}

impl Binder {
    fn named(name: Option<Box<str>>, payload: Type) -> Self {
        Self {
            name,
            payload,
            projections: Box::from([]),
        }
    }

    fn projecting(payload: Type, projections: Box<[(Box<str>, Type)]>) -> Self {
        Self {
            name: None,
            payload,
            projections,
        }
    }

    fn lookup(&self, name: &str) -> Option<Found> {
        if self.name.as_deref() == Some(name) {
            return Some(Found::Direct(self.payload.clone()));
        }
        self.projections
            .iter()
            .find(|(projected, _)| projected.as_ref() == name)
            .map(|(_, payload)| Found::Projected(payload.clone()))
    }
}

impl<'a> Bodies<'a> {
    pub(crate) fn new(
        site: &'a BodySite<'a>,
        locals: &'a [(&'a str, Type)],
        deferred: &'a [&'a str],
        types: &'a BTreeMap<SurfaceTypeId, Type>,
        own_interface: &'a Interface,
        diagnostics: &'a mut Vec<Diag>,
    ) -> Self {
        Self {
            site,
            locals,
            deferred,
            types,
            own_interface,
            forbid_self_request: true,
            diagnostics,
            binders: Vec::new(),
        }
    }

    pub(crate) fn allow_self_requests(&mut self) {
        self.forbid_self_request = false;
    }

    /// A module-private definition's value: one expression or request
    /// elaborated under nothing but the definitions above it, checked against
    /// the annotation that names the rule it becomes.
    pub(crate) fn local_body(
        &mut self,
        value: &SurfaceValue,
        annotation: &Type,
        span: Span,
    ) -> Option<RuleBody> {
        let (expression, found) = self.value_typed(value)?;
        if &found != annotation {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::TypeMismatch,
                span,
                format!("the annotation says {annotation}, the definition is {found}"),
            ));
            return None;
        }
        let body = RuleBody::new(expression);
        if let Err(error) = body.validate(&Interface {
            inputs: Box::from([]),
            output: annotation.clone(),
        }) {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::InvalidBody,
                span,
                format!("the definition does not check against its annotation: {error}"),
            ));
            return None;
        }
        Some(body)
    }

    fn value_typed(&mut self, value: &SurfaceValue) -> Option<(BodyExpr, Type)> {
        match value {
            SurfaceValue::Expression(id) => self.expression(*id, None),
            SurfaceValue::Request(request) => {
                let payload = self.request_output(request)?;
                let expr = self.tail_request(request)?;
                Some((expr, payload))
            }
        }
    }

    fn request_output(&mut self, request: &SurfaceRequest) -> Option<Type> {
        match request {
            SurfaceRequest::Ask { head, .. } | SurfaceRequest::Run { head, .. } => {
                self.head_type(head, request.span())
            }
            SurfaceRequest::AskEach { head, .. } => {
                Some(Type::List(Box::new(self.head_type(head, request.span())?)))
            }
            SurfaceRequest::BytesOf { .. } => Some(Type::Bytes),
            SurfaceRequest::AskAll { span, .. } => {
                self.diagnostics.push(self.site.files.error(
                    FrontendCode::UnexpectedToken,
                    *span,
                    "a definition binds one value; a heterogeneous `ask all` binds a group",
                ));
                None
            }
        }
    }

    /// Collect every type a body names, so the declaration elaborator
    /// resolves them once with its own diagnostics and reference sites.
    pub(crate) fn type_sites(written: &SurfaceWrittenBody, sites: &mut Vec<SurfaceTypeId>) {
        for statement in &written.statements {
            if let Some(annotation) = statement.annotation {
                sites.push(annotation);
            }
            Self::value_sites(&statement.value, sites);
        }
        if let Some(tail) = &written.tail {
            Self::value_sites(tail, sites);
        }
    }

    fn value_sites(value: &SurfaceValue, sites: &mut Vec<SurfaceTypeId>) {
        match value {
            SurfaceValue::Expression(_) => {}
            SurfaceValue::Request(request) => Self::request_sites(request, sites),
        }
    }

    pub(crate) fn request_sites(request: &SurfaceRequest, sites: &mut Vec<SurfaceTypeId>) {
        match request {
            SurfaceRequest::Ask { head, .. } | SurfaceRequest::Run { head, .. } => {
                if let Some(head) = head {
                    sites.push(*head);
                }
            }
            SurfaceRequest::AskAll { requests, .. } => {
                for member in requests.iter() {
                    if let Some(head) = member.head {
                        sites.push(head);
                    }
                }
            }
            SurfaceRequest::AskEach { head, .. } => {
                if let Some(head) = head {
                    sites.push(*head);
                }
            }
            SurfaceRequest::BytesOf { .. } => {}
        }
    }

    pub(crate) fn rule_body(
        &mut self,
        written: &SurfaceWrittenBody,
        inputs: &[Type],
        names: &[Option<Box<str>>],
    ) -> Option<RuleBody> {
        if written.tail.is_none() {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::InvalidBody,
                written.span,
                "a body's value is its tail; a body of only bindings has none",
            ));
            return None;
        }
        for (input, name) in inputs.iter().zip(names) {
            self.check_builtin_shadow(name.as_deref(), written.span);
            self.binders
                .push(Binder::named(name.clone(), input.clone()));
        }
        let expression =
            self.continuation(&mut written.statements.iter(), written.tail.as_ref())?;
        let body = RuleBody::new(expression);
        if let Err(error) = body.validate(self.own_interface) {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::InvalidBody,
                written.span,
                format!("the body does not check against its rule's interface: {error}"),
            ));
            return None;
        }
        Some(body)
    }

    fn check_builtin_shadow(&mut self, name: Option<&str>, span: Span) {
        let Some(name) = name else {
            return;
        };
        if BUILTIN_NAMES.contains(&name) {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::BuiltinShadowed,
                span,
                format!("`{name}` is a builtin name and cannot be shadowed"),
            ));
        }
    }

    fn continuation(
        &mut self,
        statements: &mut std::slice::Iter<'_, SurfaceStatement>,
        tail: Option<&SurfaceValue>,
    ) -> Option<BodyExpr> {
        let Some(statement) = statements.next() else {
            return match tail {
                Some(value) => self.value(value),
                None => None,
            };
        };
        self.statement(statement, statements, tail)
    }

    fn statement(
        &mut self,
        statement: &SurfaceStatement,
        rest: &mut std::slice::Iter<'_, SurfaceStatement>,
        tail: Option<&SurfaceValue>,
    ) -> Option<BodyExpr> {
        let expected = statement
            .annotation
            .and_then(|annotation| self.types.get(&annotation))
            .cloned();
        match &statement.value {
            SurfaceValue::Expression(id) => {
                let (bound, found) = self.expression(*id, expected.as_ref())?;
                self.check_annotation(&found, expected.as_ref(), self.span_of(*id));
                let name = self.binder_name(&statement.binder);
                self.check_builtin_shadow(name.as_deref(), statement.binder.span());
                self.binders.push(Binder::named(name, found));
                let continued = self.continuation(rest, tail);
                self.binders.pop();
                Some(BodyExpr::Let {
                    bound: Box::new(bound),
                    rest: Box::new(continued?),
                })
            }
            SurfaceValue::Request(request) => {
                let mut resume = Resume {
                    rest: rest.clone(),
                    tail,
                };
                self.request(request, &statement.binder, &mut resume)
            }
        }
    }

    fn check_annotation(&mut self, found: &Type, expected: Option<&Type>, span: Span) {
        let Some(expected) = expected else {
            return;
        };
        if found != expected {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::TypeMismatch,
                span,
                format!("the annotation says {expected}, the expression is {found}"),
            ));
        }
    }

    fn binder_name(&self, binder: &SurfaceBinder) -> Option<Box<str>> {
        match binder {
            SurfaceBinder::Name { name, .. } => Some(name.clone()),
            SurfaceBinder::Group { .. } => None,
        }
    }

    fn value(&mut self, value: &SurfaceValue) -> Option<BodyExpr> {
        match value {
            SurfaceValue::Expression(id) => self.expression(*id, None).map(|(expr, _)| expr),
            SurfaceValue::Request(request) => self.tail_request(request),
        }
    }

    /// A request in tail position resumes under its result and returns it.
    fn tail_request(&mut self, request: &SurfaceRequest) -> Option<BodyExpr> {
        let resumed = || Box::new(BodyExpr::Bound(0));
        match request {
            SurfaceRequest::Ask {
                head, arguments, ..
            } => {
                let (interface, inputs) = self.request_parts(head, arguments, request.span())?;
                Some(BodyExpr::Need {
                    request: BodyRequest {
                        interface,
                        inputs: inputs.into(),
                    },
                    resume: resumed(),
                })
            }
            SurfaceRequest::Run {
                head, arguments, ..
            } => {
                let (interface, inputs) = self.request_parts(head, arguments, request.span())?;
                Some(BodyExpr::NeedAction {
                    request: BodyRequest {
                        interface,
                        inputs: inputs.into(),
                    },
                    resume: resumed(),
                })
            }
            SurfaceRequest::BytesOf { content, .. } => {
                let content_span = self.span_of(*content);
                let (content, found) = self.expression(*content, None)?;
                self.expect_type(found, Type::Blob, content_span);
                Some(BodyExpr::NeedBlob {
                    content: Box::new(content),
                    resume: resumed(),
                })
            }
            SurfaceRequest::AskEach { .. } => {
                let (source, request, _) = self.comprehension_parts(request)?;
                Some(BodyExpr::NeedEach {
                    source: Box::new(source),
                    request,
                    resume: resumed(),
                })
            }
            SurfaceRequest::AskAll { span, .. } => {
                self.diagnostics.push(self.site.files.error(
                    FrontendCode::UnexpectedToken,
                    *span,
                    "a heterogeneous `ask all` binds a group of names; it cannot be a body's tail",
                ));
                None
            }
        }
    }

    fn request(
        &mut self,
        request: &SurfaceRequest,
        binder: &SurfaceBinder,
        resume: &mut Resume<'_>,
    ) -> Option<BodyExpr> {
        match request {
            SurfaceRequest::Ask {
                head, arguments, ..
            } => {
                let (interface, inputs) = self.request_parts(head, arguments, request.span())?;
                self.need(false, interface, inputs, binder, resume)
            }
            SurfaceRequest::Run {
                head, arguments, ..
            } => {
                let (interface, inputs) = self.request_parts(head, arguments, request.span())?;
                self.need(true, interface, inputs, binder, resume)
            }
            SurfaceRequest::BytesOf { content, .. } => {
                let content_span = self.span_of(*content);
                let (content, found) = self.expression(*content, None)?;
                self.expect_type(found, Type::Blob, content_span);
                let name = self.binder_name(binder);
                self.check_builtin_shadow(name.as_deref(), binder.span());
                self.binders.push(Binder::named(name, Type::Bytes));
                let tail = resume.tail;
                let resumed = self.continuation(&mut resume.rest, tail);
                self.binders.pop();
                Some(BodyExpr::NeedBlob {
                    content: Box::new(content),
                    resume: Box::new(resumed?),
                })
            }
            SurfaceRequest::AskAll { requests, .. } => self.batch(requests, binder, resume),
            SurfaceRequest::AskEach { .. } => {
                let (source, request, output) = self.comprehension_parts(request)?;
                let name = self.binder_name(binder);
                self.binders
                    .push(Binder::named(name, Type::List(Box::new(output))));
                let tail = resume.tail;
                let resumed = self.continuation(&mut resume.rest, tail);
                self.binders.pop();
                Some(BodyExpr::NeedEach {
                    source: Box::new(source),
                    request,
                    resume: Box::new(resumed?),
                })
            }
        }
    }

    fn need(
        &mut self,
        action: bool,
        interface: Interface,
        inputs: Vec<BodyExpr>,
        binder: &SurfaceBinder,
        resume: &mut Resume<'_>,
    ) -> Option<BodyExpr> {
        let name = self.binder_name(binder);
        self.check_builtin_shadow(name.as_deref(), binder.span());
        self.binders
            .push(Binder::named(name, interface.output.clone()));
        let tail = resume.tail;
        let resumed = self.continuation(&mut resume.rest, tail);
        self.binders.pop();
        let request = BodyRequest {
            interface,
            inputs: inputs.into(),
        };
        let resume = Box::new(resumed?);
        Some(if action {
            BodyExpr::NeedAction { request, resume }
        } else {
            BodyExpr::Need { request, resume }
        })
    }

    fn request_parts(
        &mut self,
        head: &Option<SurfaceTypeId>,
        arguments: &[SurfaceExprId],
        span: Span,
    ) -> Option<(Interface, Vec<BodyExpr>)> {
        let output = self.head_type(head, span)?;
        let mut inputs = Vec::with_capacity(arguments.len());
        let mut payloads = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let (expr, found) = self.expression(*argument, None)?;
            inputs.push(expr);
            payloads.push(found);
        }
        let interface = Interface {
            inputs: payloads.into(),
            output,
        };
        if self.forbid_self_request && &interface == self.own_interface {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::SelfRequest,
                span,
                "a rule may not request its own interface",
            ));
            return None;
        }
        Some((interface, inputs))
    }

    fn head_type(&mut self, head: &Option<SurfaceTypeId>, span: Span) -> Option<Type> {
        let Some(head) = head else {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::HeadlessRequest,
                span,
                "the request's head type did not survive parsing",
            ));
            return None;
        };
        self.types.get(head).cloned()
    }

    fn batch(
        &mut self,
        requests: &[SurfaceBatchMember],
        binder: &SurfaceBinder,
        resume: &mut Resume<'_>,
    ) -> Option<BodyExpr> {
        let SurfaceBinder::Group { names, span } = binder else {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::UnexpectedToken,
                binder.span(),
                "a heterogeneous `ask all` binds a parenthesized group of names",
            ));
            return None;
        };
        let mut built = Vec::with_capacity(requests.len());
        let mut outputs = Vec::with_capacity(requests.len());
        for member in requests {
            let (interface, inputs) =
                self.request_parts(&member.head, &member.arguments, member.span)?;
            outputs.push(interface.output.clone());
            built.push(BodyRequest {
                interface,
                inputs: inputs.into(),
            });
        }
        let names: Vec<Option<Box<str>>> =
            names.iter().map(|name| self.binder_name(name)).collect();
        if names.len() != outputs.len() {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::TypeMismatch,
                *span,
                format!(
                    "the batch answers {} requests and binds {} names",
                    outputs.len(),
                    names.len()
                ),
            ));
            return None;
        }
        for name in names.iter().flatten() {
            self.check_builtin_shadow(Some(name), *span);
        }
        // The engine binds the first request's result at `Bound(0)`, so the
        // binders go on the stack in reverse.
        for (name, output) in names.iter().zip(outputs.iter()).rev() {
            self.binders
                .push(Binder::named(name.clone(), output.clone()));
        }
        let tail = resume.tail;
        let resumed = self.continuation(&mut resume.rest, tail);
        for _ in names {
            self.binders.pop();
        }
        Some(BodyExpr::NeedAll {
            requests: built.into(),
            resume: Box::new(resumed?),
        })
    }

    /// The comprehension's parts: the list it iterates and the request it
    /// builds per element, with the output the results list carries.
    fn comprehension_parts(
        &mut self,
        request: &SurfaceRequest,
    ) -> Option<(BodyExpr, BodyRequest, Type)> {
        let SurfaceRequest::AskEach {
            head,
            binder: element,
            source,
            clauses,
            arguments,
            span,
        } = request
        else {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::UnexpectedToken,
                request.span(),
                "expected a comprehension",
            ));
            return None;
        };
        self.check_builtin_shadow(Some(element), *span);
        let (source_expr, source_type) = self.expression(*source, None)?;
        let element_type = match source_type {
            Type::List(element) => *element,
            found => {
                self.diagnostics.push(self.site.files.error(
                    FrontendCode::TypeMismatch,
                    self.span_of(*source),
                    format!("a comprehension iterates a list, found {found}"),
                ));
                return None;
            }
        };
        let source_expr = self.filtered_source(source_expr, &element_type, element, clauses)?;
        let bindings = clause_bindings(clauses);
        let (iterated, built) = if bindings.is_empty() {
            self.binders
                .push(Binder::named(Some(element.clone()), element_type.clone()));
            let built = self.request_parts(head, arguments, *span);
            self.binders.pop();
            (source_expr, built?)
        } else {
            let environment = self.environment_step(&element_type, element, &bindings)?;
            let iterated = BodyExpr::Fold {
                source: Box::new(source_expr),
                init: Box::new(BodyExpr::List {
                    element: environment.record.clone(),
                    items: Box::from([]),
                }),
                step: Box::new(environment.step),
            };
            self.binders.push(Binder::projecting(
                environment.record,
                environment.projections,
            ));
            let built = self.request_parts(head, arguments, *span);
            self.binders.pop();
            (iterated, built?)
        };
        let (interface, inputs) = built;
        let output = interface.output.clone();
        let request = BodyRequest {
            interface,
            inputs: inputs.into(),
        };
        Some((iterated, request, output))
    }

    /// Apply leading filters before derived comprehension bindings.
    fn filtered_source(
        &mut self,
        source: BodyExpr,
        element_type: &Type,
        element: &str,
        clauses: &[SurfaceClause],
    ) -> Option<BodyExpr> {
        let filters = clauses
            .iter()
            .take_while(|clause| matches!(clause, SurfaceClause::Filter { .. }))
            .filter_map(|clause| match clause {
                SurfaceClause::Filter { condition, span } => Some((*condition, *span)),
                _ => None,
            })
            .collect::<Vec<_>>();
        if filters.is_empty() {
            return Some(source);
        }
        self.binders.push(Binder::named(
            None,
            Type::List(Box::new(element_type.clone())),
        ));
        self.binders
            .push(Binder::named(Some(element.into()), element_type.clone()));
        let mut conditions = Vec::with_capacity(filters.len());
        for (condition, span) in filters {
            let (condition, found) = self.expression(condition, None)?;
            self.expect_type(found, Type::Bool, span);
            conditions.push(condition);
        }
        let unchanged = BodyExpr::Bound(1);
        let mut step = BodyExpr::Append {
            left: Box::new(unchanged.clone()),
            right: Box::new(BodyExpr::List {
                element: element_type.clone(),
                items: Box::new([BodyExpr::Bound(0)]),
            }),
        };
        for condition in conditions.into_iter().rev() {
            step = BodyExpr::If {
                condition: Box::new(condition),
                then: Box::new(step),
                otherwise: Box::new(unchanged.clone()),
            };
        }
        self.binders.pop();
        self.binders.pop();
        Some(BodyExpr::Fold {
            source: Box::new(source),
            init: Box::new(BodyExpr::List {
                element: element_type.clone(),
                items: Box::from([]),
            }),
            step: Box::new(step),
        })
    }

    /// The fold step that turns each element into the per-element
    /// environment: the element and every derived binding, in canonical field
    /// order. The accumulator binder starts as a placeholder because the
    /// environment's type is what the clause types add up to, and nothing
    /// reads the placeholder before it is fixed.
    fn environment_step(
        &mut self,
        element_type: &Type,
        element: &str,
        bindings: &[(Box<str>, SurfaceExprId)],
    ) -> Option<Environment> {
        let accumulator = self.binders.len();
        self.binders.push(Binder::named(None, Type::Unit));
        self.binders
            .push(Binder::named(Some(element.into()), element_type.clone()));
        let mut values = Vec::with_capacity(bindings.len());
        for (name, value) in bindings {
            let (expr, payload) = self.expression(*value, None)?;
            self.binders
                .push(Binder::named(Some(name.clone()), payload.clone()));
            values.push((name.clone(), expr, payload));
        }
        let mut fields: Vec<(Box<str>, Type)> = vec![(Box::from(element), element_type.clone())];
        fields.extend(
            values
                .iter()
                .map(|(name, _, payload)| (name.clone(), payload.clone())),
        );
        fields.sort_by(|left, right| left.0.cmp(&right.0));
        let depth_of = |name: &str| -> Option<usize> {
            self.binders
                .iter()
                .rev()
                .position(|binder| binder.name.as_deref() == Some(name))
        };
        let mut sorted = Vec::with_capacity(fields.len());
        for (name, _) in fields.iter() {
            sorted.push(RecordField {
                name: name.clone(),
                payload: BodyExpr::Bound(depth_of(name)?),
            });
        }
        let record = BodyExpr::Record {
            fields: sorted.into(),
        };
        let environment = Type::record(
            fields
                .clone()
                .into_iter()
                .map(|(name, payload)| RecordField { name, payload })
                .collect::<Vec<_>>(),
        )
        .ok()?;
        let cons = BodyExpr::Append {
            left: Box::new(BodyExpr::Bound(
                self.binders
                    .len()
                    .saturating_sub(accumulator)
                    .saturating_sub(1),
            )),
            right: Box::new(BodyExpr::List {
                element: environment.clone(),
                items: Box::new([record]),
            }),
        };
        let mut step = cons;
        for (_, expr, _) in values.iter().rev() {
            self.binders.pop();
            step = BodyExpr::Let {
                bound: Box::new(expr.clone()),
                rest: Box::new(step),
            };
        }
        self.binders.pop();
        if let Some(binder) = self.binders.get_mut(accumulator) {
            binder.payload = environment.clone();
        }
        self.binders.pop();
        Some(Environment {
            record: environment,
            step,
            projections: fields.into(),
        })
    }

    fn expect_type(&mut self, found: Type, expected: Type, span: Span) {
        if found != expected {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::TypeMismatch,
                span,
                format!("expected {expected}, found {found}"),
            ));
        }
    }
}

fn clause_bindings(clauses: &[SurfaceClause]) -> Vec<(Box<str>, SurfaceExprId)> {
    clauses
        .iter()
        .filter_map(|clause| match clause {
            SurfaceClause::Let { name, value, .. } => Some((name.clone(), *value)),
            _ => None,
        })
        .collect()
}
