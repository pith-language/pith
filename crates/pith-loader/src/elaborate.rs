use core::range::Range;
use std::collections::{BTreeMap, BTreeSet};

use pith_core::{
    Coordinate, DeclarationError, DeclarationTable, Interface, RecordField, SumConstructor, Type,
};
use pith_diag::{Diag, Severity, Span};

use crate::merge::ModuleFiles;
use crate::position::{DefinitionLocation, ReferenceSite};
use crate::surface::{
    ParsedSurface, SurfaceBody, SurfaceDeclaration, SurfaceTypeId, SurfaceTypeNode,
};
use crate::{FrontendCode, RuleCategory, ScopedImports};

pub(crate) struct ElaboratedRule {
    pub label: Box<str>,
    pub category: RuleCategory,
    pub interface: Interface,
    pub span: Span,
}

pub(crate) struct Elaborated {
    pub table: DeclarationTable,
    pub rules: Vec<ElaboratedRule>,
    pub incomplete_rules: Vec<IncompleteRule>,
    pub references: Vec<ReferenceSite>,
}

pub(crate) struct IncompleteRule {
    pub label: Box<str>,
    pub diagnostics: Box<[Diag]>,
}

pub(crate) fn elaborate(
    module: &str,
    surface: &ParsedSurface,
    imports: &ScopedImports<'_>,
    definitions: &BTreeMap<Box<str>, DefinitionLocation>,
    files: &ModuleFiles,
    diagnostics: &mut Vec<Diag>,
) -> Elaborated {
    let declared = collect_declarations(module, surface, files, diagnostics);
    diagnose_rule_coordinates(module, surface, files, diagnostics);
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
    let (rules, incomplete_rules) = elaborator.elaborate_rules();
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

impl<'a> Elaborator<'a> {
    fn elaborate_rules(&mut self) -> (Vec<ElaboratedRule>, Vec<IncompleteRule>) {
        let mut rules = Vec::new();
        let mut incomplete_rules = Vec::new();
        let mut interfaces = BTreeMap::new();
        for rule in &self.surface.rules {
            let first_diagnostic = self.diagnostics.len();
            match self.elaborate_rule(rule, &mut interfaces) {
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
        rule: &crate::surface::SurfaceRule,
        interfaces: &mut BTreeMap<(RuleCategory, Interface), Span>,
    ) -> Option<ElaboratedRule> {
        let inputs = rule
            .inputs
            .iter()
            .map(|input| self.elaborate_type(*input, None))
            .collect::<Option<Vec<_>>>()?;
        let output = self.elaborate_type(rule.output, None)?;
        let interface = Interface {
            inputs: inputs.into(),
            output,
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
        Some(ElaboratedRule {
            label: rule.label.clone(),
            category: rule.category,
            interface,
            span: rule.span,
        })
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
            self.diagnostics.push(self.files.error(
                FrontendCode::UnknownName,
                span,
                format!("module `{}` declares no `{name}`", self.module),
            ));
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
