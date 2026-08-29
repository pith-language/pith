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

/// Whether a rule is exported to importers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    /// A module-private definition, elaborated to a rule no importer names.
    Local,
    /// Part of the module's interface surface, nameable through an import.
    Public,
}

pub struct ElaboratedRule {
    pub label: Box<str>,
    pub category: RuleCategory,
    pub interface: Interface,
    pub span: Span,
    /// `None` identifies a host implementation; represented rules carry a body.
    pub body: Option<RuleBody>,
    pub visibility: Visibility,
}

pub struct ElaboratedEntry {
    pub name: Box<str>,
    pub interface: Interface,
    pub span: Span,
    pub body: RuleBody,
}

pub struct Elaborated {
    pub table: DeclarationTable,
    pub rules: Vec<ElaboratedRule>,
    pub entries: Vec<ElaboratedEntry>,
    pub incomplete_rules: Vec<IncompleteRule>,
    pub references: Vec<ReferenceSite>,
}

pub struct IncompleteRule {
    pub label: Box<str>,
    pub diagnostics: Box<[Diag]>,
}

/// Whether the body being elaborated may request its own interface. Entries
/// may: planning an action whose contract shares the entry's inputs is the
/// wrapper pattern. Rules may not: a request of their own pure interface
/// would wait on itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelfRequests {
    Allowed,
    Forbidden,
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
    let order = order_declarations(surface, &declared, files, diagnostics);
    let mut elaborator = Elaborator {
        module,
        surface,
        imports,
        definitions,
        files,
        declared: &declared,
        resolved: BTreeMap::new(),
        table: DeclarationTable::new(module),
        diagnostics,
        references: Vec::new(),
    };
    for name in order {
        elaborator.elaborate_declaration(name);
    }
    let (local_types, mut rules, interfaces) = elaborator.elaborate_locals();
    let (written_rules, incomplete_rules) = elaborator.elaborate_rules(&local_types, interfaces);
    rules.extend(written_rules);
    let entries = elaborator.elaborate_entries(&local_types);
    Elaborated {
        table: elaborator.table,
        rules,
        entries,
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

/// The DFS color a declaration carries while the topological order and the
/// cycle diagnosis share one pass's state: `Done` marks a node provably
/// outside any cycle, `InStack` marks the current DFS path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VisitState {
    Unvisited,
    InStack,
    Done,
}

fn order_declarations<'a>(
    surface: &ParsedSurface,
    declared: &BTreeMap<&'a str, &'a SurfaceDeclaration>,
    files: &ModuleFiles,
    diagnostics: &mut Vec<Diag>,
) -> Vec<&'a str> {
    let count = declared.len();

    let names: Vec<&'a str> = declared.keys().copied().collect();
    let index_of: BTreeMap<&'a str, usize> = names
        .iter()
        .enumerate()
        .map(|(index, &name)| (name, index))
        .collect();

    let mut edges: Vec<(usize, usize)> = Vec::new();
    let mut dep_counts = vec![0_usize; count];
    let mut last_seen = vec![usize::MAX; count];
    for (index, (&name, &declaration)) in declared.iter().enumerate() {
        let mut push = |dep: usize, edges: &mut Vec<(usize, usize)>| {
            if let Some(seen) = last_seen.get_mut(dep)
                && *seen != index
            {
                *seen = index;
                if let Some(slot) = dep_counts.get_mut(index) {
                    *slot = slot.saturating_add(1);
                }
                edges.push((dep, index));
            }
        };
        collect_dependencies(surface, declaration, name, &index_of, &mut |dep| {
            push(dep, &mut edges)
        });
    }

    let (dep_offsets, dep_targets) = adjacency(count, &edges, |edge| edge.1, |edge| edge.0);
    let (dependent_offsets, dependent_targets) =
        adjacency(count, &edges, |edge| edge.0, |edge| edge.1);

    let mut in_degree = dep_counts;
    let mut queue: VecDeque<usize> = (0..count)
        .filter(|&index| in_degree.get(index).copied().unwrap_or(1) == 0)
        .collect();
    let mut order: Vec<usize> = Vec::with_capacity(count);
    while let Some(index) = queue.pop_front() {
        order.push(index);
        let start = dependent_offsets.get(index).copied().unwrap_or(0);
        let end = dependent_offsets
            .get(index.saturating_add(1))
            .copied()
            .unwrap_or(0);
        let Some(dependents) = dependent_targets.get(start..end) else {
            continue;
        };
        for &dependent in dependents {
            let Some(degree) = in_degree.get_mut(dependent) else {
                continue;
            };
            *degree = degree.saturating_sub(1);
            if *degree == 0 {
                queue.push_back(dependent);
            }
        }
    }

    let mut state = vec![VisitState::Unvisited; count];
    for &index in &order {
        if let Some(slot) = state.get_mut(index) {
            *slot = VisitState::Done;
        }
    }
    let unordered: Vec<usize> = (0..count)
        .filter(|&index| state.get(index).copied().unwrap_or(VisitState::Done) != VisitState::Done)
        .collect();
    diagnose_cycles(
        &names,
        &dep_offsets,
        &dep_targets,
        &mut state,
        declared,
        files,
        diagnostics,
    );
    order.extend(unordered);
    order
        .into_iter()
        .filter_map(|index| names.get(index).copied())
        .collect()
}

fn adjacency(
    count: usize,
    edges: &[(usize, usize)],
    key: fn(&(usize, usize)) -> usize,
    value: fn(&(usize, usize)) -> usize,
) -> (Vec<usize>, Vec<usize>) {
    let mut counts = vec![0_usize; count];
    for edge in edges {
        if let Some(slot) = counts.get_mut(key(edge)) {
            *slot = slot.saturating_add(1);
        }
    }
    let mut running = 0_usize;
    let mut offsets = Vec::with_capacity(count.saturating_add(1));
    offsets.push(0);
    offsets.extend(counts.iter().map(|count| {
        running = running.saturating_add(*count);
        running
    }));
    let mut cursor = offsets.clone();
    let mut targets = vec![0_usize; edges.len()];
    for edge in edges {
        let Some(&slot) = cursor.get(key(edge)) else {
            continue;
        };
        if let Some(next) = cursor.get_mut(key(edge)) {
            *next = next.saturating_add(1);
        }
        if let Some(target) = targets.get_mut(slot) {
            *target = value(edge);
        }
    }
    (offsets, targets)
}

fn collect_dependencies(
    surface: &ParsedSurface,
    declaration: &SurfaceDeclaration,
    self_name: &str,
    index_of: &BTreeMap<&str, usize>,
    push: &mut impl FnMut(usize),
) {
    match &declaration.body {
        SurfaceBody::Nominal(body) | SurfaceBody::Alias(body) => {
            collect_type_dependencies(surface, *body, self_name, index_of, push);
        }
        SurfaceBody::Sum(constructors) => {
            for constructor in constructors.iter() {
                if let Some(payload) = constructor.payload {
                    collect_type_dependencies(surface, payload, self_name, index_of, push);
                }
            }
        }
    }
}

fn collect_type_dependencies(
    surface: &ParsedSurface,
    type_id: SurfaceTypeId,
    self_name: &str,
    index_of: &BTreeMap<&str, usize>,
    push: &mut impl FnMut(usize),
) {
    let Some(node) = surface.types.get(type_id) else {
        return;
    };
    match node {
        SurfaceTypeNode::List(element) => {
            collect_type_dependencies(surface, *element, self_name, index_of, push);
        }
        SurfaceTypeNode::Record { fields } => {
            let (Ok(start), Ok(end)) = (usize::try_from(fields.start), usize::try_from(fields.end))
            else {
                return;
            };
            let Some(fields) = surface.fields.get(start..end) else {
                return;
            };
            for field in fields {
                collect_type_dependencies(surface, field.payload, self_name, index_of, push);
            }
        }
        SurfaceTypeNode::Reference {
            module: None, name, ..
        } if name.as_ref() != self_name => {
            if let Some(&dep) = index_of.get(name.as_ref()) {
                push(dep);
            }
        }
        // Scalars and qualified references declare no in-module dependency; a
        // new composite type must add its dependency walk here.
        _ => {}
    }
}

fn diagnose_cycles(
    names: &[&str],
    dep_offsets: &[usize],
    dep_targets: &[usize],
    state: &mut [VisitState],
    declared: &BTreeMap<&str, &SurfaceDeclaration>,
    files: &ModuleFiles,
    diagnostics: &mut Vec<Diag>,
) {
    let frame_of = |node: usize| -> Option<std::ops::Range<usize>> {
        let start = dep_offsets.get(node).copied().unwrap_or(0);
        let end = dep_offsets
            .get(node.saturating_add(1))
            .copied()
            .unwrap_or(0);
        (start <= end).then_some(start..end)
    };
    let mut path: Vec<usize> = Vec::new();
    for root in 0..state.len() {
        if state.get(root).copied().unwrap_or(VisitState::Done) != VisitState::Unvisited {
            continue;
        }
        let mut stack = vec![frame_of(root).unwrap_or(0..0)];
        if let Some(slot) = state.get_mut(root) {
            *slot = VisitState::InStack;
        }
        path.push(root);
        while let Some(deps) = stack.last_mut() {
            let Some(dep) = deps.next().and_then(|slot| dep_targets.get(slot).copied()) else {
                stack.pop();
                if let Some(node) = path.pop()
                    && let Some(slot) = state.get_mut(node)
                {
                    *slot = VisitState::Done;
                }
                continue;
            };
            match state.get(dep).copied().unwrap_or(VisitState::Done) {
                VisitState::Unvisited => {
                    if let Some(slot) = state.get_mut(dep) {
                        *slot = VisitState::InStack;
                    }
                    path.push(dep);
                    stack.push(frame_of(dep).unwrap_or(0..0));
                }
                VisitState::InStack => {
                    let Some(start) = path.iter().position(|&candidate| candidate == dep) else {
                        continue;
                    };
                    let Some(cycle_path) = path.get(start..) else {
                        continue;
                    };
                    let cycle = cycle_path
                        .iter()
                        .copied()
                        .chain(std::iter::once(dep))
                        .filter_map(|index| names.get(index).copied())
                        .map(|part| format!("`{part}`"))
                        .collect::<Vec<_>>()
                        .join(" -> ");
                    let span = declared
                        .get(names.get(dep).copied().unwrap_or(""))
                        .map_or(Span::none(), |declaration| declaration.name_span);
                    diagnostics.push(files.error(
                        FrontendCode::CyclicDeclaration,
                        span,
                        format!("declarations form a mutual cycle: {cycle}"),
                    ));
                }
                VisitState::Done => {}
            }
        }
    }
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
            let body = self.value_body(
                &local.value,
                &interface,
                &types,
                local.span,
                SelfRequests::Forbidden,
            );
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
                visibility: Visibility::Local,
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
            visibility: Visibility::Public,
        })
    }

    fn elaborate_entries(&mut self, visible: &[(&'a str, Type)]) -> Vec<ElaboratedEntry> {
        let mut entries = Vec::new();
        for entry in &self.surface.entries {
            let Some(output) = self.elaborate_type(entry.output, None) else {
                continue;
            };
            let interface = Interface {
                inputs: Box::from([]),
                output,
            };
            let value = SurfaceValue::Request(entry.request.clone());
            let Some(body) = self.value_body(
                &value,
                &interface,
                visible,
                entry.span,
                SelfRequests::Allowed,
            ) else {
                continue;
            };
            entries.push(ElaboratedEntry {
                name: entry.name.clone(),
                interface,
                span: entry.span,
                body,
            });
        }
        entries
    }

    fn value_body(
        &mut self,
        value: &SurfaceValue,
        interface: &Interface,
        visible: &[(&'a str, Type)],
        span: Span,
        self_requests: SelfRequests,
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
        if self_requests == SelfRequests::Allowed {
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
        let Some(&declaration) = self.declared.get(name) else {
            return;
        };
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
