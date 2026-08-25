use core::range::Range;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::ScopedImports;
use pith_core::{
    Coordinate, DeclarationError, DeclarationTable, Interface, RecordField, RuleBody,
    SumConstructor, Type,
};
use pith_diag::{Diag, Severity, Span};
use pith_hir::{
    DefinitionLocation, FrontendCode, ModuleFiles, ParsedSurface, ReferenceSite, RuleCategory,
    SurfaceBody, SurfaceDeclaration, SurfaceRule, SurfaceRuleBody, SurfaceTypeId, SurfaceTypeNode,
    SurfaceValue,
};

use crate::body::{BUILTIN_NAMES, Bodies};

pub struct ElaboratedRule {
    pub label: Box<str>,
    pub category: RuleCategory,
    pub interface: Interface,
    pub span: Span,
    /// `None` identifies a host implementation; represented rules carry a body.
    pub body: Option<RuleBody>,
    /// A module-private definition elaborated to a rule no importer names.
    pub local: bool,
}

pub struct Elaborated {
    pub table: DeclarationTable,
    pub rules: Vec<ElaboratedRule>,
    pub incomplete_rules: Vec<IncompleteRule>,
    pub references: Vec<ReferenceSite>,
}

pub struct IncompleteRule {
    pub label: Box<str>,
    pub diagnostics: Box<[Diag]>,
}

pub fn elaborate(
    module: &str,
    surface: &ParsedSurface,
    imports: &ScopedImports<'_>,
    definitions: &BTreeMap<Box<str>, DefinitionLocation>,
    files: &ModuleFiles,
    diagnostics: &mut Vec<Diag>,
) -> Elaborated {
    let declared = collect_declarations(module, surface, files, diagnostics);
    diagnose_rule_coordinates(module, surface, files, diagnostics);
    diagnose_entry_coordinates(module, surface, files, diagnostics);
    let mut elaborator = Elaborator {
        module,
        surface,
        imports,
        definitions,
        files,
        declared: &declared,
        resolved: BTreeMap::new(),
        elaborating: Vec::new(),
        table: DeclarationTable::new(module),
        diagnostics,
        references: Vec::new(),
    };
    for name in declared.keys() {
        elaborator.elaborate_declaration(name);
    }
    let (local_types, mut rules, interfaces) = elaborator.elaborate_locals();
    let (written_rules, incomplete_rules) = elaborator.elaborate_rules(&local_types, interfaces);
    rules.extend(written_rules);
    elaborator.elaborate_entries(&local_types);
    Elaborated {
        table: elaborator.table,
        rules,
        incomplete_rules,
        references: elaborator.references,
    }
}

fn collect_declarations<'a>(
    module: &str,
    surface: &'a ParsedSurface,
    files: &ModuleFiles,
    diagnostics: &mut Vec<Diag>,
) -> BTreeMap<&'a str, &'a SurfaceDeclaration> {
    let mut declarations: BTreeMap<&str, &SurfaceDeclaration> = BTreeMap::new();
    for declaration in &surface.declarations {
        if let Some(previous) = declarations.get(declaration.name.as_ref()) {
            diagnostics.push(files.error(
                FrontendCode::DuplicateDeclaration,
                declaration.name_span,
                format!("module `{module}` declares `{}` twice", declaration.name),
            ));
            diagnostics.push(
                Diag::new(
                    Severity::Error,
                    FrontendCode::DuplicateDeclaration.stable(),
                    previous.name_span,
                    format!("`{}` is first declared here", declaration.name),
                )
                .with_source(files.source_of(previous.name_span).clone()),
            );
        } else {
            declarations.insert(declaration.name.as_ref(), declaration);
        }
    }
    declarations
}

fn diagnose_rule_coordinates(
    module: &str,
    surface: &ParsedSurface,
    files: &ModuleFiles,
    diagnostics: &mut Vec<Diag>,
) {
    let mut coordinates = BTreeMap::new();
    for rule in &surface.rules {
        if let Some(previous) = coordinates.insert(rule.label.as_ref(), rule.label_span) {
            diagnostics.push(files.error(
                FrontendCode::DuplicateRule,
                rule.label_span,
                format!("module `{module}` declares rule `{}` twice", rule.label),
            ));
            diagnostics.push(files.error(
                FrontendCode::DuplicateRule,
                previous,
                format!("rule `{}` is first declared here", rule.label),
            ));
        }
    }
}

fn diagnose_entry_coordinates(
    module: &str,
    surface: &ParsedSurface,
    files: &ModuleFiles,
    diagnostics: &mut Vec<Diag>,
) {
    let mut coordinates = BTreeMap::new();
    for entry in &surface.entries {
        if let Some(previous) = coordinates.insert(entry.name.as_ref(), entry.name_span) {
            diagnostics.push(files.error(
                FrontendCode::DuplicateEntry,
                entry.name_span,
                format!("module `{module}` declares entry `{}` twice", entry.name),
            ));
            diagnostics.push(files.error(
                FrontendCode::DuplicateEntry,
                previous,
                format!("entry `{}` is first declared here", entry.name),
            ));
        }
    }
}

struct Elaborator<'a> {
    module: &'a str,
    surface: &'a ParsedSurface,
    imports: &'a ScopedImports<'a>,
    definitions: &'a BTreeMap<Box<str>, DefinitionLocation>,
    files: &'a ModuleFiles,
    declared: &'a BTreeMap<&'a str, &'a SurfaceDeclaration>,
    resolved: BTreeMap<&'a str, Type>,
    elaborating: Vec<&'a str>,
    table: DeclarationTable,
    diagnostics: &'a mut Vec<Diag>,
    references: Vec<ReferenceSite>,
}

type Interfaces = BTreeMap<(RuleCategory, Interface), Span>;

impl<'a> Elaborator<'a> {
    /// The module-private definitions, each elaborated to a represented rule
    /// whose interface is its annotation, with uses elaborating to requests
    /// against that interface: a first-order call, not an inlined expansion.
    fn elaborate_locals(&mut self) -> (Vec<(&'a str, Type)>, Vec<ElaboratedRule>, Interfaces) {
        let mut interfaces = Interfaces::new();
        let mut rules = Vec::new();
        let mut types = Vec::new();
        let mut seen = BTreeMap::new();
        for local in &self.surface.locals {
            if seen.insert(local.name.as_ref(), local.name_span).is_some() {
                self.diagnostics.push(self.files.error(
                    FrontendCode::DuplicateLocal,
                    local.name_span,
                    format!("module `{}` defines `{}` twice", self.module, local.name),
                ));
                continue;
            }
            if BUILTIN_NAMES.contains(&local.name.as_ref()) {
                self.diagnostics.push(self.files.error(
                    FrontendCode::BuiltinShadowed,
                    local.name_span,
                    format!("`{}` is a builtin name and cannot be shadowed", local.name),
                ));
            }
            let Some(annotation) = self.elaborate_type(local.annotation, None) else {
                continue;
            };
            let interface = Interface {
                inputs: Box::from([]),
                output: annotation.clone(),
            };
            let key = (RuleCategory::Pure, interface.clone());
            if let Some(previous) = interfaces.insert(key, local.span) {
                self.diagnostics.push(self.files.error(
                    FrontendCode::DuplicateInterface,
                    local.span,
                    format!(
                        "the definition of `{}` provides an interface already provided in this \
                         module",
                        local.name
                    ),
                ));
                self.diagnostics.push(self.files.error(
                    FrontendCode::DuplicateInterface,
                    previous,
                    "the first provider is declared here",
                ));
                continue;
            }
            let body = self.value_body(&local.value, &interface, &types, local.span, false);
            types.push((local.name.as_ref(), annotation));
            let Some(body) = body else {
                continue;
            };
            rules.push(ElaboratedRule {
                label: local.name.clone(),
                category: RuleCategory::Pure,
                interface,
                span: local.span,
                body: Some(body),
                local: true,
            });
        }
        (types, rules, interfaces)
    }

    fn elaborate_rules(
        &mut self,
        local_types: &[(&'a str, Type)],
        mut interfaces: Interfaces,
    ) -> (Vec<ElaboratedRule>, Vec<IncompleteRule>) {
        let mut rules = Vec::new();
        let mut incomplete_rules = Vec::new();
        for rule in &self.surface.rules {
            let first_diagnostic = self.diagnostics.len();
            match self.elaborate_rule(rule, local_types, &mut interfaces) {
                Some(elaborated) => rules.push(elaborated),
                None => incomplete_rules.push(IncompleteRule {
                    label: rule.label.clone(),
                    diagnostics: self
                        .diagnostics
                        .get(first_diagnostic..)
                        .unwrap_or_else(|| unreachable!("elaboration only appends diagnostics"))
                        .into(),
                }),
            }
        }
        (rules, incomplete_rules)
    }

    fn elaborate_rule(
        &mut self,
        rule: &SurfaceRule,
        local_types: &[(&'a str, Type)],
        interfaces: &mut Interfaces,
    ) -> Option<ElaboratedRule> {
        let inputs = rule
            .params
            .iter()
            .map(|param| self.elaborate_type(param.payload, None))
            .collect::<Option<Vec<_>>>()?;
        let output = self.elaborate_type(rule.output, None)?;
        let interface = Interface {
            inputs: inputs.clone().into(),
            output: output.clone(),
        };
        let key = (rule.category, interface.clone());
        if let Some(previous) = interfaces.insert(key, rule.span) {
            self.diagnostics.push(self.files.error(
                FrontendCode::DuplicateInterface,
                rule.span,
                format!(
                    "the {:?} rule `{}` provides an interface already provided in this module",
                    rule.category, rule.label
                ),
            ));
            self.diagnostics.push(self.files.error(
                FrontendCode::DuplicateInterface,
                previous,
                "the first provider is declared here",
            ));
            return None;
        }
        let body = match &rule.body {
            SurfaceRuleBody::Host => None,
            SurfaceRuleBody::Written(written) => {
                let types = self.resolve_body_types(written);
                let names = rule
                    .params
                    .iter()
                    .map(|param| param.name.as_ref().map(|(name, _)| name.clone()))
                    .collect::<Vec<_>>();
                let deferred = self
                    .surface
                    .locals
                    .iter()
                    .map(|local| local.name.as_ref())
                    .collect::<Vec<_>>();
                let Elaborator {
                    module,
                    surface,
                    imports,
                    resolved,
                    files,
                    diagnostics,
                    ..
                } = self;
                let site = crate::body::BodySite {
                    module,
                    surface,
                    resolved,
                    imports,
                    files,
                };
                let mut bodies = Bodies::new(
                    &site,
                    local_types,
                    &deferred,
                    &types,
                    &interface,
                    diagnostics,
                );
                Some(bodies.rule_body(written, &inputs, &names)?)
            }
        };
        Some(ElaboratedRule {
            label: rule.label.clone(),
            category: rule.category,
            interface,
            span: rule.span,
            body,
            local: false,
        })
    }

    fn elaborate_entries(&mut self, visible: &[(&'a str, Type)]) {
        for entry in &self.surface.entries {
            let Some(output) = self.elaborate_type(entry.output, None) else {
                continue;
            };
            let interface = Interface {
                inputs: Box::from([]),
                output,
            };
            let value = SurfaceValue::Request(entry.request.clone());
            let _ = self.value_body(&value, &interface, visible, entry.span, true);
        }
    }

    fn value_body(
        &mut self,
        value: &SurfaceValue,
        interface: &Interface,
        visible: &[(&'a str, Type)],
        span: Span,
        allow_self_requests: bool,
    ) -> Option<RuleBody> {
        let empty = BTreeMap::new();
        let types = match value {
            SurfaceValue::Expression(_) => &empty,
            SurfaceValue::Request(_) => &self.value_types(value),
        };
        let Elaborator {
            module,
            surface,
            imports,
            resolved,
            files,
            diagnostics,
            ..
        } = self;
        let site = crate::body::BodySite {
            module,
            surface,
            resolved,
            imports,
            files,
        };
        let deferred = surface
            .locals
            .iter()
            .map(|local| local.name.as_ref())
            .collect::<Vec<_>>();
        let mut bodies = Bodies::new(&site, visible, &deferred, types, interface, diagnostics);
        if allow_self_requests {
            bodies.allow_self_requests();
        }
        bodies.local_body(value, &interface.output, span)
    }

    fn value_types(&mut self, value: &SurfaceValue) -> BTreeMap<SurfaceTypeId, Type> {
        let SurfaceValue::Request(request) = value else {
            return BTreeMap::new();
        };
        let mut sites = Vec::new();
        Bodies::request_sites(request, &mut sites);
        self.resolve_types(&sites)
    }

    fn resolve_body_types(
        &mut self,
        written: &pith_hir::SurfaceWrittenBody,
    ) -> BTreeMap<SurfaceTypeId, Type> {
        let mut sites = Vec::new();
        Bodies::type_sites(written, &mut sites);
        self.resolve_types(&sites)
    }

    fn resolve_types(&mut self, sites: &[SurfaceTypeId]) -> BTreeMap<SurfaceTypeId, Type> {
        let mut types = BTreeMap::new();
        for &site in sites {
            if let Some(resolved) = self.elaborate_type(site, None) {
                types.insert(site, resolved);
            }
        }
        types
    }

    fn elaborate_declaration(&mut self, name: &'a str) {
        if self.resolved.contains_key(name) {
            return;
        }
        if let Some(cycle_start) = self
            .elaborating
            .iter()
            .position(|candidate| *candidate == name)
        {
            let cycle = self.elaborating.get(cycle_start..).unwrap_or(&[]);
            let path = cycle
                .iter()
                .copied()
                .chain(std::iter::once(name))
                .map(|part| format!("`{part}`"))
                .collect::<Vec<_>>()
                .join(" -> ");
            let span = self
                .declared
                .get(name)
                .map_or(Span::none(), |declaration| declaration.name_span);
            self.diagnostics.push(self.files.error(
                FrontendCode::CyclicDeclaration,
                span,
                format!("declarations form a mutual cycle: {path}"),
            ));
            return;
        }
        let Some(&declaration) = self.declared.get(name) else {
            return;
        };
        self.elaborating.push(name);
        let outcome = match &declaration.body {
            SurfaceBody::Nominal(body) => self
                .elaborate_type(*body, Some(name))
                .map(|representation| self.table.nominal(name, representation)),
            SurfaceBody::Alias(body) => self
                .elaborate_type(*body, Some(name))
                .map(|target| self.table.alias(name, target)),
            SurfaceBody::Sum(constructors) => {
                let mut names = BTreeSet::new();
                let mut duplicate = false;
                for constructor in constructors.iter() {
                    if !names.insert(constructor.name.as_ref()) {
                        duplicate = true;
                        self.diagnostics.push(self.files.error(
                            FrontendCode::DuplicateField,
                            constructor.span,
                            format!("the sum declares constructor `{}` twice", constructor.name),
                        ));
                    }
                }
                if duplicate {
                    None
                } else {
                    let lowered: Option<Vec<SumConstructor>> = constructors
                        .iter()
                        .map(|constructor| {
                            let payload = match constructor.payload {
                                Some(payload) => Some(self.elaborate_type(payload, Some(name))?),
                                None => None,
                            };
                            Some(SumConstructor {
                                name: constructor.name.clone(),
                                payload,
                            })
                        })
                        .collect();
                    lowered.map(|constructors| self.table.sum(name, constructors))
                }
            }
        };
        if let Some(outcome) = outcome {
            self.register(name, declaration, outcome);
        }
        self.elaborating.pop();
    }

    fn register(
        &mut self,
        name: &'a str,
        declaration: &SurfaceDeclaration,
        outcome: Result<Type, DeclarationError>,
    ) {
        match outcome {
            Ok(declared) => {
                self.resolved.insert(name, declared);
            }
            Err(DeclarationError::InvalidName { .. }) => self.diagnostics.push(self.files.error(
                FrontendCode::UnexpectedToken,
                declaration.name_span,
                format!("`{name}` must be non-empty and dot-free"),
            )),
            Err(DeclarationError::RecursiveAlias { .. }) => {
                self.diagnostics.push(self.files.error(
                    FrontendCode::RecursiveAlias,
                    declaration.name_span,
                    format!("alias `{}.{name}` refers to itself", self.module),
                ))
            }
            Err(DeclarationError::DuplicateName { .. }) => {}
        }
    }

    fn elaborate_type(&mut self, type_id: SurfaceTypeId, self_name: Option<&str>) -> Option<Type> {
        match self.surface.types.get(type_id)? {
            SurfaceTypeNode::Unit => Some(Type::Unit),
            SurfaceTypeNode::Bool => Some(Type::Bool),
            SurfaceTypeNode::Int => Some(Type::Int),
            SurfaceTypeNode::Text => Some(Type::Text),
            SurfaceTypeNode::Bytes => Some(Type::Bytes),
            SurfaceTypeNode::Blob => Some(Type::Blob),
            SurfaceTypeNode::List(element) => Some(Type::List(Box::new(
                self.elaborate_type(*element, self_name)?,
            ))),
            SurfaceTypeNode::Record { fields } => self.elaborate_record(*fields, self_name),
            SurfaceTypeNode::Reference { module, name, span } => match module {
                Some(module) => self.resolve_import(module, name, *span),
                None => self.resolve_local(name, self_name, *span),
            },
        }
    }

    fn elaborate_record(&mut self, fields: Range<u32>, self_name: Option<&str>) -> Option<Type> {
        let fields = Range {
            start: usize::try_from(fields.start).ok()?,
            end: usize::try_from(fields.end).ok()?,
        };
        let mut names = BTreeSet::new();
        let mut duplicate = false;
        let mut lowered = Vec::new();
        for field in self.surface.fields.get(fields)? {
            if !names.insert(field.name.as_ref()) {
                duplicate = true;
                self.diagnostics.push(self.files.error(
                    FrontendCode::DuplicateField,
                    field.span,
                    format!("the record names field `{}` twice", field.name),
                ));
                continue;
            }
            lowered.push(RecordField {
                name: field.name.clone(),
                payload: self.elaborate_type(field.payload, self_name)?,
            });
        }
        if duplicate {
            None
        } else {
            Type::record(lowered).ok()
        }
    }

    fn resolve_local(
        &mut self,
        name: &'a str,
        self_name: Option<&str>,
        span: Span,
    ) -> Option<Type> {
        if self_name == Some(name) {
            self.record_local_reference(name, span);
            return Some(Type::Cut);
        }
        if !self.resolved.contains_key(name) && self.declared.contains_key(name) {
            self.elaborate_declaration(name);
        }
        let Some(resolved) = self.resolved.get(name).cloned() else {
            if !self.declared.contains_key(name) {
                self.diagnostics.push(self.files.error(
                    FrontendCode::UnknownName,
                    span,
                    format!("module `{}` declares no `{name}`", self.module),
                ));
            }
            return None;
        };
        self.record_local_reference(name, span);
        Some(resolved)
    }

    fn record_local_reference(&mut self, name: &str, span: Span) {
        if let Some(definition) = self.definitions.get(name) {
            self.references.push(ReferenceSite::new(
                Coordinate::new(self.module, name),
                span,
                definition.clone(),
            ));
        }
    }

    fn resolve_import(&mut self, module: &str, name: &str, span: Span) -> Option<Type> {
        let Some(imported) = self.imports.get(module) else {
            self.diagnostics.push(self.files.error(
                FrontendCode::UndeclaredQualifiedAccess,
                span,
                format!("module `{module}` is not imported by `{}`", self.module),
            ));
            return None;
        };
        let Some(declaration) = imported.table.get(name) else {
            self.diagnostics.push(self.files.error(
                FrontendCode::UnknownName,
                span,
                format!("module `{module}` declares no `{name}`"),
            ));
            return None;
        };
        if let Some(definition) = imported.declaration_definition(name) {
            self.references.push(ReferenceSite::new(
                Coordinate::new(module, name),
                span,
                definition.clone(),
            ));
        }
        Some(Type::of_declaration(declaration))
    }
}
