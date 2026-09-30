//! The dependency walk: resolve one module's inputs, emitting dependencies
//! before their consumers, with each fresh location admitted against what
//! its clause asked.

use std::sync::Arc;

use pith_hir::{FrontendCode, InputLocator, ModuleSubject, Project, ProjectInput};

use super::super::acquire::ProjectStore;
use super::super::admission;
use super::super::module::ProjectFiles;
use super::{PendingModule, ProjectFile, Resolution, at, claims, view_of};

/// What an input's route reached.
pub(super) enum Reached<L> {
    /// The route returns to a location already on the walk; the cycle
    /// diagnostic is attached where the route was taken.
    Cycle,
    /// A location already emitted into the graph, carrying the subject its
    /// project declared, so the consumer's binding resolves without
    /// reading it again.
    Loaded(ModuleSubject),
    /// A location whose project file was read for the first time on this
    /// walk.
    Fresh(L, Arc<ProjectFile>),
}

impl<S: ProjectStore> Resolution<S> {
    /// Resolve the module's inputs, emitting dependencies before their
    /// consumer. Repeated routes to one location load once.
    pub(super) fn walk(&mut self, module: PendingModule<S::Location>) {
        let PendingModule {
            location,
            file,
            view,
            files,
            bindings,
        } = module;
        self.stack.push(view.subject().clone());
        self.stack_locations.insert(location.clone());
        let mut resolved = bindings;
        for input in view.inputs().to_vec() {
            self.resolve_input(&location, &file, &input, &mut resolved);
        }
        self.stack_locations.remove(&location);
        self.stack.pop();
        let index = self.modules.len();
        self.by_location.insert(location.clone(), index);
        self.modules.push(PendingModule {
            location,
            file,
            view,
            files,
            bindings: resolved,
        });
    }

    /// Resolve one input against the routes already walked: the
    /// builtin-name refusal, then whatever the input's locator reaches.
    fn resolve_input(
        &mut self,
        consumer_location: &S::Location,
        consumer: &Arc<ProjectFile>,
        input: &ProjectInput,
        resolved: &mut Vec<(Box<str>, ModuleSubject)>,
    ) {
        if input.name.as_ref() == crate::import::BUILTIN_MODULE {
            self.diagnostics.push(at(
                FrontendCode::BuiltinShadowed,
                input.name_span,
                consumer.parsed.source(),
                format!(
                    "the input name `{}` names the builtin module and stays bound to it",
                    input.name
                ),
            ));
        }
        match self.route(consumer_location, consumer, input) {
            Some(Reached::Cycle) | None => {}
            Some(Reached::Loaded(subject)) => resolved.push((input.name.clone(), subject)),
            Some(Reached::Fresh(location, dependency)) => {
                self.admit_dependency(location, dependency, input, resolved)
            }
        }
    }

    /// What an input's locator reaches: already walked, currently being
    /// walked, or a freshly read location. A locator names content, so the
    /// subject it selects is declared by the project the route reaches,
    /// except a registry locator, which names its own.
    ///
    /// Which routes are servable is the store's question, not this one's. A
    /// store that cannot serve a source kind says so, and the refusal lands
    /// on the clause that wrote it.
    fn route(
        &mut self,
        consumer_location: &S::Location,
        consumer: &Arc<ProjectFile>,
        input: &ProjectInput,
    ) -> Option<Reached<S::Location>> {
        let span = input.locator.span();
        let route = match self.routing.route(input, consumer.parsed.source()) {
            Ok(route) => route,
            Err(diagnostic) => {
                self.diagnostics.push(diagnostic);
                return None;
            }
        };
        let located = match claims::LocatedSource::acquire(&self.store, consumer_location, &route) {
            Ok(located) => located,
            Err(failure) => {
                let code = failure
                    .diagnostic_code()
                    .unwrap_or(FrontendCode::MissingProject);
                self.diagnostics.push(at(
                    code,
                    span,
                    consumer.parsed.source(),
                    format!(
                        "`{}` binds its input through {}: {}",
                        input.name,
                        input.locator.kind(),
                        failure.describe()
                    ),
                ));
                return None;
            }
        };
        let location = located.location().clone();
        if self.stack_locations.contains(&location) {
            let chain = self
                .stack
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" -> ");
            self.diagnostics.push(at(
                FrontendCode::DependencyCycle,
                span,
                consumer.parsed.source(),
                format!("the input route returns to `{location}`, already on the route {chain}"),
            ));
            return Some(Reached::Cycle);
        }
        if let Some(&index) = self.by_location.get(&location) {
            if let Some(module) = self.modules.get(index) {
                if let InputLocator::Registry { .. } = &input.locator
                    && let Some(diagnostic) =
                        admission_diagnostic(&module.view, &location, consumer, input)
                {
                    self.diagnostics.push(diagnostic);
                    return None;
                }
                // A route to a location already loaded still passes the
                // claim check: one subject keeps one route, and a second
                // route of another authority to the same content is the
                // same contradiction it would be on a fresh read.
                if let Err(claims::Failure::Conflict(diagnostics)) = self.claims.claim(
                    module.view.subject(),
                    located,
                    consumer.parsed.source(),
                    span,
                ) {
                    self.diagnostics.extend(diagnostics.to_vec());
                    return None;
                }
                return Some(Reached::Loaded(module.view.subject().clone()));
            }
            return None;
        }
        let dependency = match self.read(&location) {
            Ok(file) => file,
            Err(failure) => {
                let code = failure
                    .diagnostic_code()
                    .unwrap_or(FrontendCode::MissingProject);
                self.diagnostics.push(at(
                    code,
                    span,
                    consumer.parsed.source(),
                    format!(
                        "cannot read the project at `{location}`, which `{}` names: {}",
                        input.name,
                        failure.describe()
                    ),
                ));
                return None;
            }
        };
        let Some(dependency_view) = view_of(&dependency) else {
            self.diagnostics.push(at(
                FrontendCode::MissingSubject,
                span,
                consumer.parsed.source(),
                format!(
                    "the project at `{location}`, which `{}` names, declares no `module` \
                     clause, and an input target declares its subject",
                    input.name
                ),
            ));
            return None;
        };
        if let Err(claims::Failure::Conflict(diagnostics)) = self.claims.claim(
            dependency_view.subject(),
            located,
            consumer.parsed.source(),
            span,
        ) {
            self.diagnostics.extend(diagnostics.to_vec());
            return None;
        }
        if let InputLocator::Registry { .. } = &input.locator
            && let Some(diagnostic) =
                admission_diagnostic(&dependency_view, &location, consumer, input)
        {
            self.diagnostics.push(diagnostic);
            return None;
        }
        Some(Reached::Fresh(location, dependency))
    }

    /// A freshly read location: membership rules, then the dependency's
    /// own inputs.
    fn admit_dependency(
        &mut self,
        location: S::Location,
        dependency: Arc<ProjectFile>,
        input: &ProjectInput,
        resolved: &mut Vec<(Box<str>, ModuleSubject)>,
    ) {
        let Some(view) = view_of(&dependency) else {
            return;
        };
        resolved.push((input.name.clone(), view.subject().clone()));
        if let Some(nested) = view.workspace() {
            self.diagnostics.push(at(
                FrontendCode::NestedWorkspace,
                nested.span,
                dependency.parsed.source(),
                "a dependency declares no workspace; only the root lists members",
            ));
            return;
        }
        if let Some(host) = view.host() {
            self.diagnostics.push(at(
                FrontendCode::UnboundHost,
                host.span,
                dependency.parsed.source(),
                "the `host` clause names the component that serves this project's `= host` \
                 rules, and no component binding exists",
            ));
            return;
        }
        self.refuse_supplied_authority(&view, dependency.parsed.source());
        if !view.registries().is_empty() || !view.domains().is_empty() {
            return;
        }
        self.declare_subject(&location, &view, dependency.parsed.source());
        self.walk(PendingModule {
            location,
            file: dependency,
            view,
            files: ProjectFiles::unacquired(),
            bindings: Vec::new(),
        });
    }

    /// A dependency's registry bindings and domain routes are diagnosed and
    /// carry no authority. Both are consumer configuration: a project that
    /// is a root in its own checkout keeps them, and they stop meaning
    /// anything the moment something else selects it.
    fn refuse_supplied_authority(&mut self, view: &Project, source: &Arc<pith_diag::SourceFile>) {
        let registries = view
            .registries()
            .iter()
            .map(|registry| (registry.span, format!("registry `{}`", registry.name)));
        let domains = view.domains().iter().map(|route| {
            (
                route.span,
                format!("a route for domain `{}`", route.domain.as_str()),
            )
        });
        for (span, what) in registries.chain(domains).collect::<Vec<_>>() {
            self.diagnostics.push(at(
                FrontendCode::DependencySuppliedAuthority,
                span,
                source,
                format!(
                    "a dependency declares {what}, and routing is the consumer's; this clause \
                     carries no authority here"
                ),
            ));
        }
    }

    /// Record that `location` declares the view's subject. A second
    /// location declaring it is refused even if the bytes match; the same
    /// location declaring it twice is one module seen twice.
    pub(super) fn declare_subject(
        &mut self,
        location: &S::Location,
        view: &Project,
        source: &Arc<pith_diag::SourceFile>,
    ) {
        let subject = view.subject();
        if let Some(prior) = self.subjects.get(subject) {
            if prior == location {
                return;
            }
            self.diagnostics.push(at(
                FrontendCode::DuplicateSubject,
                view.span(),
                source,
                format!(
                    "the subject {subject} is declared by two locations: `{prior}` and \
                     `{location}`"
                ),
            ));
            return;
        }
        self.subjects.insert(subject.clone(), location.clone());
    }
}

fn admission_diagnostic(
    view: &Project,
    location: &impl std::fmt::Display,
    consumer: &ProjectFile,
    input: &ProjectInput,
) -> Option<pith_diag::Diag> {
    let InputLocator::Registry {
        subject,
        subject_span,
        range,
        range_span,
        ..
    } = &input.locator
    else {
        return None;
    };
    let request = admission::Request::new(subject.clone(), range.clone());
    let refusal = admission::admit(&request, view).err()?;
    let span = match refusal.clause {
        admission::Clause::Subject { .. } => *subject_span,
        admission::Clause::Version { .. } => *range_span,
    };
    Some(at(
        FrontendCode::UnexpectedSubject,
        span,
        consumer.parsed.source(),
        format!(
            "`{}` binds {}: {} (acquired at `{location}`)",
            input.name, subject, refusal
        ),
    ))
}
