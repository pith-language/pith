use super::*;

impl Bodies<'_> {
    pub(super) fn span_of(&self, id: SurfaceExprId) -> Span {
        self.site
            .surface
            .exprs
            .get(id)
            .map_or(Span::none(), expression_span)
    }

    fn lookup(&self, name: &str) -> Option<(usize, Found)> {
        for (depth, binder) in self.binders.iter().rev().enumerate() {
            if let Some(found) = binder.lookup(name) {
                return Some((depth, found));
            }
        }
        None
    }

    pub(super) fn expression(
        &mut self,
        id: SurfaceExprId,
        expected: Option<&Type>,
    ) -> Option<(BodyExpr, Type)> {
        match self.site.surface.exprs.get(id)? {
            SurfaceExpr::Literal { value, .. } => {
                let payload = value.value_type();
                Some((BodyExpr::Literal(value.clone()), payload))
            }
            SurfaceExpr::Name { name, span } => self.name(name, *span),
            SurfaceExpr::Field { record, name, .. } => {
                let record_span = self.span_of(*record);
                let (record, found) = self.expression(*record, None)?;
                let Type::Record(fields) = found.clone() else {
                    self.diagnostics.push(self.site.files.error(
                        FrontendCode::TypeMismatch,
                        record_span,
                        format!("`{name}` reads a field, but {found} is not a record"),
                    ));
                    return None;
                };
                let Some(field) = fields
                    .iter()
                    .find(|field| field.name.as_ref() == name.as_ref())
                else {
                    self.diagnostics.push(self.site.files.error(
                        FrontendCode::UnknownName,
                        self.span_of(id),
                        format!(
                            "the record {} has no field `{name}`",
                            Type::Record(fields.clone())
                        ),
                    ));
                    return None;
                };
                Some((
                    BodyExpr::Field {
                        record: Box::new(record),
                        name: name.clone(),
                    },
                    field.payload.clone(),
                ))
            }
            SurfaceExpr::Record { fields, .. } => {
                let (expr, payload) = self.record(fields)?;
                Some((expr, Type::Record(payload)))
            }
            SurfaceExpr::List { items, .. } => self.list(items, expected, self.span_of(id)),
            SurfaceExpr::Construct {
                name,
                arguments,
                span,
            } => self.construct(name, arguments, *span),
            SurfaceExpr::Unwrap { value, .. } => {
                let value_span = self.span_of(*value);
                let (value, found) = self.expression(*value, None)?;
                let Type::Nominal(declared) = found else {
                    self.diagnostics.push(self.site.files.error(
                        FrontendCode::TypeMismatch,
                        value_span,
                        format!("unwrap reads a nominal, found {found}"),
                    ));
                    return None;
                };
                Some((
                    BodyExpr::Unwrap {
                        nominal: Box::new(value),
                    },
                    declared.representation.clone(),
                ))
            }
            SurfaceExpr::If {
                condition,
                then,
                otherwise,
                ..
            } => {
                let condition_span = self.span_of(*condition);
                let (condition, found) = self.expression(*condition, None)?;
                self.expect_type(found, Type::Bool, condition_span);
                let (then, then_type) = self.expression(*then, expected)?;
                let (otherwise, otherwise_type) = self.expression(*otherwise, expected)?;
                if then_type != otherwise_type {
                    self.diagnostics.push(self.site.files.error(
                        FrontendCode::TypeMismatch,
                        self.span_of(id),
                        format!("the branches disagree: {then_type} and {otherwise_type}"),
                    ));
                    return None;
                }
                Some((
                    BodyExpr::If {
                        condition: Box::new(condition),
                        then: Box::new(then),
                        otherwise: Box::new(otherwise),
                    },
                    then_type,
                ))
            }
            SurfaceExpr::Match {
                scrutinee,
                arms,
                span,
            } => self.matching(*scrutinee, arms, *span),
            SurfaceExpr::Fold {
                source,
                init,
                element,
                accumulator,
                step,
                ..
            } => {
                let source_span = self.span_of(*source);
                let (source, found) = self.expression(*source, None)?;
                let element_type = match found {
                    Type::List(element) => *element,
                    found => {
                        self.diagnostics.push(self.site.files.error(
                            FrontendCode::TypeMismatch,
                            source_span,
                            format!("a fold runs over a list, found {found}"),
                        ));
                        return None;
                    }
                };
                let (init, accumulator_type) = self.expression(*init, None)?;
                self.binders.push(Binder::named(
                    Some(accumulator.clone()),
                    accumulator_type.clone(),
                ));
                self.binders
                    .push(Binder::named(Some(element.clone()), element_type));
                let step_span = self.span_of(*step);
                let (step, step_type) = self.expression(*step, None)?;
                self.binders.pop();
                self.binders.pop();
                if step_type != accumulator_type {
                    self.diagnostics.push(self.site.files.error(
                        FrontendCode::TypeMismatch,
                        step_span,
                        format!(
                            "the step produces {step_type}, but the accumulator is {accumulator_type}"
                        ),
                    ));
                    return None;
                }
                Some((
                    BodyExpr::Fold {
                        source: Box::new(source),
                        init: Box::new(init),
                        step: Box::new(step),
                    },
                    accumulator_type,
                ))
            }
            SurfaceExpr::Binary {
                operator,
                left,
                right,
                ..
            } => self.binary(*operator, *left, *right, self.span_of(id)),
        }
    }

    fn name(&mut self, name: &str, span: Span) -> Option<(BodyExpr, Type)> {
        if let Some((depth, found)) = self.lookup(name) {
            return match found {
                Found::Direct(payload) => Some((BodyExpr::Bound(depth), payload)),
                Found::Projected(payload) => Some((
                    BodyExpr::Field {
                        record: Box::new(BodyExpr::Bound(depth)),
                        name: name.into(),
                    },
                    payload,
                )),
            };
        }
        if let Some((_, payload)) = self.locals.iter().find(|(local, _)| *local == name) {
            // A local definition is a first-order call, not an inlined
            // expansion: the request names only its annotation.
            return Some((
                BodyExpr::Need {
                    request: BodyRequest {
                        interface: Interface {
                            inputs: Box::from([]),
                            output: payload.clone(),
                        },
                        inputs: Box::from([]),
                    },
                    resume: Box::new(BodyExpr::Bound(0)),
                },
                payload.clone(),
            ));
        }
        if name == MODULE_BUILTIN {
            return Some((
                BodyExpr::Literal(Value::Text(Box::from(self.site.module))),
                Type::Text,
            ));
        }
        if self.deferred.contains(&name) {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::OutOfOrderLocal,
                span,
                format!(
                    "`{name}` is defined later in the file; local definitions can only use earlier definitions"
                ),
            ));
            return None;
        }
        self.diagnostics.push(self.site.files.error(
            FrontendCode::UnknownName,
            span,
            format!("the body names no `{name}`"),
        ));
        None
    }

    fn record(
        &mut self,
        fields: &[SurfaceValueField],
    ) -> Option<(BodyExpr, Box<[RecordField<Type>]>)> {
        let mut elaborated = Vec::with_capacity(fields.len());
        for field in fields {
            let (value, payload) = self.expression(field.value, None)?;
            elaborated.push(RecordField {
                name: field.name.clone(),
                payload: (value, payload, field.span),
            });
        }
        elaborated.sort_by(|left, right| left.name.cmp(&right.name));
        for [earlier, later] in elaborated.array_windows::<2>() {
            if earlier.name == later.name {
                self.diagnostics.push(self.site.files.error(
                    FrontendCode::DuplicateField,
                    later.payload.2,
                    format!("the record names field `{}` twice", later.name),
                ));
                return None;
            }
        }
        let typed = elaborated
            .iter()
            .map(|field| RecordField {
                name: field.name.clone(),
                payload: field.payload.1.clone(),
            })
            .collect::<Vec<_>>();
        let expr = BodyExpr::Record {
            fields: elaborated
                .into_iter()
                .map(|field| RecordField {
                    name: field.name,
                    payload: field.payload.0,
                })
                .collect::<Box<[_]>>(),
        };
        Some((expr, typed.into()))
    }

    fn list(
        &mut self,
        items: &[SurfaceExprId],
        expected: Option<&Type>,
        span: Span,
    ) -> Option<(BodyExpr, Type)> {
        let mut elaborated = Vec::with_capacity(items.len());
        let mut element: Option<Type> = None;
        for item in items {
            let (expr, found) = self.expression(*item, None)?;
            match &element {
                None => element = Some(found),
                Some(element) if element == &found => {}
                Some(element) => {
                    self.diagnostics.push(self.site.files.error(
                        FrontendCode::TypeMismatch,
                        self.span_of(*item),
                        format!("the list holds {element} and {found}"),
                    ));
                    return None;
                }
            }
            elaborated.push(expr);
        }
        let element = match (element, expected) {
            (Some(element), _) => element,
            (None, Some(Type::List(element))) => (**element).clone(),
            (None, _) => {
                self.diagnostics.push(self.site.files.error(
                    FrontendCode::TypeMismatch,
                    span,
                    "an empty list does not say its element type; annotate the `let` it is bound \
                     by",
                ));
                return None;
            }
        };
        Some((
            BodyExpr::List {
                element: element.clone(),
                items: elaborated.into(),
            },
            Type::List(Box::new(element)),
        ))
    }

    fn construct(
        &mut self,
        name: &str,
        arguments: &[SurfaceExprId],
        span: Span,
    ) -> Option<(BodyExpr, Type)> {
        if BUILTIN_NAMES.contains(&name) && name != MODULE_BUILTIN {
            return self.builtin(name, arguments, span);
        }
        let elaborated = arguments
            .iter()
            .map(|argument| self.expression(*argument, None))
            .collect::<Option<Vec<_>>>()?;
        match self.construct_target(name, span)? {
            Type::Nominal(nominal) => self.wrap(*nominal, elaborated, span),
            Type::Sum(sum) => self.make_sum(name, *sum, elaborated, span),
            found => {
                self.diagnostics.push(self.site.files.error(
                    FrontendCode::TypeMismatch,
                    span,
                    format!("`{name}` constructs nothing; it declares {found}"),
                ));
                None
            }
        }
    }

    /// What a declared-name application constructs: a nominal declared under
    /// the name itself, or the sum that declares a constructor under it. The
    /// constructors are fields of the sum's handler record, so they are
    /// spelled unqualified and resolved by search.
    fn construct_target(&mut self, name: &str, span: Span) -> Option<Type> {
        let mut candidates = Vec::new();
        for (declared, found) in self.site.resolved.iter() {
            if constructs(found, name, declared) {
                candidates.push(found.clone());
            }
        }
        for (_, imported) in self.site.imports.iter() {
            for declaration in imported.table.iter() {
                let found = Type::of_declaration(declaration);
                let declared = declaration.coordinate().name.as_ref();
                if constructs(&found, name, declared) {
                    candidates.push(found);
                }
            }
        }
        match candidates.len() {
            1 => candidates.pop(),
            0 => {
                self.diagnostics.push(self.site.files.error(
                    FrontendCode::UnknownName,
                    span,
                    format!("no declaration or builtin constructs `{name}`"),
                ));
                None
            }
            _ => {
                let spellings = candidates
                    .iter()
                    .map(Type::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                self.diagnostics.push(self.site.files.error(
                    FrontendCode::UnknownName,
                    span,
                    format!("`{name}` names more than one declaration: {spellings}"),
                ));
                None
            }
        }
    }

    fn wrap(
        &mut self,
        nominal: NominalType,
        elaborated: Vec<(BodyExpr, Type)>,
        span: Span,
    ) -> Option<(BodyExpr, Type)> {
        let [(_, representation)] = elaborated.as_slice() else {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::TypeMismatch,
                span,
                "wrapping a nominal takes exactly its representation",
            ));
            return None;
        };
        if representation != &nominal.representation {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::TypeMismatch,
                span,
                format!(
                    "the nominal wraps {}, found {representation}",
                    nominal.representation
                ),
            ));
            return None;
        }
        let [(value, _)] = &elaborated[..] else {
            return None;
        };
        Some((
            BodyExpr::Wrap {
                declared: nominal.clone(),
                representation: Box::new(value.clone()),
            },
            Type::Nominal(Box::new(nominal)),
        ))
    }

    fn make_sum(
        &mut self,
        constructor: &str,
        sum: SumType,
        elaborated: Vec<(BodyExpr, Type)>,
        span: Span,
    ) -> Option<(BodyExpr, Type)> {
        let declared = sum
            .constructors
            .iter()
            .find(|candidate| candidate.name.as_ref() == constructor);
        let Some(declared) = declared else {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::UnknownName,
                span,
                format!(
                    "the sum {} declares no `{constructor}`",
                    sum.coordinate.spelling()
                ),
            ));
            return None;
        };
        let payload = match (declared.payload.as_ref(), elaborated.as_slice()) {
            (None, []) => None,
            (Some(payload), [(value, found)]) => {
                if found != payload {
                    self.diagnostics.push(self.site.files.error(
                        FrontendCode::TypeMismatch,
                        span,
                        format!("`{constructor}` carries {payload}, found {found}"),
                    ));
                    return None;
                }
                Some(value.clone())
            }
            _ => {
                self.diagnostics.push(self.site.files.error(
                    FrontendCode::TypeMismatch,
                    span,
                    format!(
                        "`{constructor}` takes the payload its sum declares, no more and no less"
                    ),
                ));
                return None;
            }
        };
        Some((
            BodyExpr::MakeSum {
                declared: sum.clone(),
                constructor: constructor.into(),
                payload: payload.map(Box::new),
            },
            Type::Sum(Box::new(sum)),
        ))
    }

    /// One arm's binder is present exactly when the constructor it covers
    /// carries a payload; an arm over a payloadless constructor that binds a
    /// name would shift every index under it.
    fn matching(
        &mut self,
        scrutinee: SurfaceExprId,
        arms: &[SurfaceArm],
        span: Span,
    ) -> Option<(BodyExpr, Type)> {
        let (scrutinee_expr, found) = self.expression(scrutinee, None)?;
        let Type::Sum(sum) = found else {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::TypeMismatch,
                self.span_of(scrutinee),
                format!("a match eliminates a declared sum, found {found}"),
            ));
            return None;
        };
        if arms.is_empty() {
            self.diagnostics.push(self.site.files.error(
                FrontendCode::InvalidBody,
                span,
                "a match requires at least one arm",
            ));
            return None;
        }
        let mut ordered = arms.to_vec();
        ordered.sort_by(|left, right| left.constructor.cmp(&right.constructor));
        for [earlier, later] in ordered.array_windows::<2>() {
            if earlier.constructor == later.constructor {
                self.diagnostics.push(self.site.files.error(
                    FrontendCode::DuplicateArm,
                    later.span,
                    format!("the match covers `{}` twice", later.constructor),
                ));
                return None;
            }
        }
        let mut lowered = Vec::with_capacity(ordered.len());
        let mut matched: Option<Type> = None;
        for arm in &ordered {
            let declared = sum
                .constructors
                .iter()
                .find(|candidate| candidate.name == arm.constructor);
            let Some(declared) = declared else {
                self.diagnostics.push(self.site.files.error(
                    FrontendCode::UnknownName,
                    arm.span,
                    format!(
                        "`{}` is not a constructor of {}",
                        arm.constructor,
                        sum.coordinate.spelling()
                    ),
                ));
                return None;
            };
            let binds = match (declared.payload.as_ref(), arm.binder.as_ref()) {
                (Some(payload), Some(name)) => {
                    self.check_builtin_shadow(Some(name), arm.span);
                    self.binders
                        .push(Binder::named(Some(name.clone()), payload.clone()));
                    true
                }
                (None, None) => false,
                _ => {
                    self.diagnostics.push(self.site.files.error(
                        FrontendCode::TypeMismatch,
                        arm.span,
                        format!(
                            "the binder on `{}` disagrees with the payload its sum declares",
                            arm.constructor
                        ),
                    ));
                    return None;
                }
            };
            let (body, arm_type) = self.expression(arm.body, None)?;
            if binds {
                self.binders.pop();
            }
            matched = match matched {
                None => Some(arm_type),
                Some(matched) if matched == arm_type => Some(matched),
                Some(matched) => {
                    self.diagnostics.push(self.site.files.error(
                        FrontendCode::TypeMismatch,
                        arm.span,
                        format!("the arms disagree: {matched} and {arm_type}"),
                    ));
                    return None;
                }
            };
            lowered.push(MatchArm {
                constructor: arm.constructor.clone(),
                body: Box::new(body),
            });
        }
        Some((
            BodyExpr::Match {
                scrutinee: Box::new(scrutinee_expr),
                arms: lowered.into(),
            },
            matched.unwrap_or_else(|| unreachable!("a non-empty match establishes an arm type")),
        ))
    }

    fn binary(
        &mut self,
        operator: SurfaceOperator,
        left: SurfaceExprId,
        right: SurfaceExprId,
        span: Span,
    ) -> Option<(BodyExpr, Type)> {
        let (left_expr, left_type) = self.expression(left, None)?;
        let (right_expr, right_type) = self.expression(right, None)?;
        match operator {
            SurfaceOperator::Equal => {
                self.expect_shared_operand_type(left_type, right_type, span)?;
                Some((
                    BodyExpr::Equal {
                        left: Box::new(left_expr),
                        right: Box::new(right_expr),
                    },
                    Type::Bool,
                ))
            }
            SurfaceOperator::NotEqual => {
                self.expect_shared_operand_type(left_type, right_type, span)?;
                Some((
                    BodyExpr::If {
                        condition: Box::new(BodyExpr::Equal {
                            left: Box::new(left_expr),
                            right: Box::new(right_expr),
                        }),
                        then: Box::new(BodyExpr::Literal(Value::Bool(false))),
                        otherwise: Box::new(BodyExpr::Literal(Value::Bool(true))),
                    },
                    Type::Bool,
                ))
            }
            SurfaceOperator::IntAdd
            | SurfaceOperator::IntSubtract
            | SurfaceOperator::IntMultiply => {
                self.expect_type(left_type, Type::Int, span);
                self.expect_type(right_type, Type::Int, span);
                let expr = match operator {
                    SurfaceOperator::IntAdd => BodyExpr::IntAdd {
                        left: Box::new(left_expr),
                        right: Box::new(right_expr),
                    },
                    SurfaceOperator::IntSubtract => BodyExpr::IntSubtract {
                        left: Box::new(left_expr),
                        right: Box::new(right_expr),
                    },
                    SurfaceOperator::IntMultiply => BodyExpr::IntMultiply {
                        left: Box::new(left_expr),
                        right: Box::new(right_expr),
                    },
                    SurfaceOperator::Equal | SurfaceOperator::NotEqual => {
                        unreachable!("the equality operators returned above")
                    }
                };
                Some((expr, Type::Int))
            }
        }
    }

    fn expect_shared_operand_type(&mut self, left: Type, right: Type, span: Span) -> Option<()> {
        if left == right {
            return Some(());
        }
        self.diagnostics.push(self.site.files.error(
            FrontendCode::TypeMismatch,
            span,
            format!("`==` compares one type, found {left} and {right}"),
        ));
        None
    }

    fn builtin(
        &mut self,
        name: &str,
        arguments: &[SurfaceExprId],
        span: Span,
    ) -> Option<(BodyExpr, Type)> {
        let elaborated = arguments
            .iter()
            .map(|argument| self.expression(*argument, None))
            .collect::<Option<Vec<_>>>()?;
        match (name, elaborated.as_slice()) {
            ("describe", [(value, _)]) => Some((
                BodyExpr::Describe {
                    value: Box::new(value.clone()),
                },
                Type::Text,
            )),
            ("concat", [(left, left_type), (right, right_type)]) => {
                self.expect_type(left_type.clone(), Type::Text, span);
                self.expect_type(right_type.clone(), Type::Text, span);
                Some((
                    BodyExpr::TextConcat {
                        left: Box::new(left.clone()),
                        right: Box::new(right.clone()),
                    },
                    Type::Text,
                ))
            }
            ("decode", [(bytes, found)]) => {
                self.expect_type(found.clone(), Type::Bytes, span);
                Some((
                    BodyExpr::TextOfBytes {
                        bytes: Box::new(bytes.clone()),
                    },
                    Type::Text,
                ))
            }
            ("append", [(left, left_type), (right, right_type)]) => {
                let element = match (left_type, right_type) {
                    (Type::List(left), Type::List(right)) if left == right => (**left).clone(),
                    (found, other) => {
                        self.diagnostics.push(self.site.files.error(
                            FrontendCode::TypeMismatch,
                            span,
                            format!("`append` joins two lists, found {found} and {other}"),
                        ));
                        return None;
                    }
                };
                Some((
                    BodyExpr::Append {
                        left: Box::new(left.clone()),
                        right: Box::new(right.clone()),
                    },
                    Type::List(Box::new(element)),
                ))
            }
            _ => {
                self.diagnostics.push(self.site.files.error(
                    FrontendCode::UnknownName,
                    span,
                    format!(
                        "the builtin `{name}` does not take {} arguments",
                        arguments.len()
                    ),
                ));
                None
            }
        }
    }
}

fn constructs(found: &Type, constructor: &str, declared: &str) -> bool {
    match found {
        Type::Sum(sum) => sum
            .constructors
            .iter()
            .any(|candidate| candidate.name.as_ref() == constructor),
        other => declared == constructor && !matches!(other, Type::Sum(_)),
    }
}

fn expression_span(expr: &SurfaceExpr) -> Span {
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
