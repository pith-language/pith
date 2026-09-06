//! The dependency walk: resolve one module's `use` clauses, emitting
//! dependencies before their consumers, with each fresh location admitted
//! against what its clause asked.

use std::sync::Arc;

use pith_diag::SourceFile;
use pith_hir::{FrontendCode, Manifest, ManifestUse};

use super::super::acquire::{AcquireFailure, ModuleStore};
use super::super::admission;
use super::super::module::SourceSet;
use super::{ManifestFile, PendingModule, Resolution, at, claims, view_of};

/// What a `use` clause's route reached.
pub(super) enum Reached<L> {
    /// The route returns to a location already on the walk; the cycle
    /// diagnostic is attached where the route was taken.
    Cycle,
    /// A location already emitted into the graph.
    Loaded(usize),
    /// A location read for the first time on this walk.
    Fresh(L, Arc<ManifestFile>),
}

impl<S: ModuleStore> Resolution<S> {
    /// Resolve the module's `use` clauses, emitting dependencies before
    /// their consumer. Repeated routes to one location load once.
    pub(super) fn walk(&mut self, module: PendingModule<S::Location>) {
        let PendingModule {
            location,
            file,
            view,
            sources,
        } = module;
        self.stack.push(view.subject().clone());
        self.stack_locations.insert(location.clone());
        for use_ in view.uses().to_vec() {
            self.resolve_use(&location, &file, &use_);
        }
        self.stack_locations.remove(&location);
        self.stack.pop();
        let index = self.modules.len();
        self.by_location.insert(location.clone(), index);
        self.modules.push(PendingModule {
            location,
            file,
            view,
            sources,
        });
    }

    /// Resolve one `use` clause against the routes already walked: the
    /// builtin-name refusal, then whatever the clause's path reaches.
    fn resolve_use(
        &mut self,
        consumer_location: &S::Location,
        consumer: &Arc<ManifestFile>,
        use_: &ManifestUse,
    ) {
        if use_.alias.as_ref() == crate::import::BUILTIN_MODULE {
            self.diagnostics.push(at(
                FrontendCode::BuiltinShadowed,
                use_.alias_span,
                consumer.parsed.source(),
                format!(
                    "the alias `{}` names the builtin module and stays bound to it",
                    use_.alias
                ),
            ));
        }
        match self.route(consumer_location, consumer, use_) {
            Some(Reached::Cycle) | None => {}
            Some(Reached::Loaded(index)) => self.admit_loaded(index, consumer, use_),
            Some(Reached::Fresh(location, dependency)) => {
                self.admit_dependency(location, dependency, consumer, use_)
            }
        }
    }

    /// What a `use` clause's route reaches: already walked, currently
    /// being walked, or a freshly read location.
    ///
    /// Which routes are servable is the store's question, not this one's. A
    /// store that cannot serve a source kind says so, and the refusal lands
    /// on the clause that wrote it.
    fn route(
        &mut self,
        consumer_location: &S::Location,
        consumer: &Arc<ManifestFile>,
        use_: &ManifestUse,
    ) -> Option<Reached<S::Location>> {
        let span = use_.source_span();
        let route = match self.routing.route(use_, consumer.parsed.source()) {
            Ok(route) => route,
            Err(diagnostic) => {
                self.diagnostics.push(diagnostic);
                return None;
            }
        };
        let location = match self.claims.locate(
            &self.store,
            consumer_location,
            &route,
            &use_.subject,
            consumer.parsed.source(),
            span,
        ) {
            Ok(location) => location,
            Err(claims::Failure::Conflict(diagnostics)) => {
                self.diagnostics.extend(*diagnostics);
                return None;
            }
            Err(claims::Failure::Acquire(failure)) => {
                let code = match failure {
                    AcquireFailure::Unsupported { .. } => FrontendCode::UnsupportedSource,
                    _ => FrontendCode::MissingManifest,
                };
                self.diagnostics.push(at(
                    code,
                    span,
                    consumer.parsed.source(),
                    format!(
                        "`{}` binds {}: {}",
                        use_.alias,
                        use_.subject,
                        failure.describe()
                    ),
                ));
                return None;
            }
        };
        if self.stack_locations.contains(&location) {
            let chain = self
                .stack
                .iter()
                .map(ToString::to_string)
                .chain([use_.subject.to_string()])
                .collect::<Vec<_>>()
                .join(" -> ");
            self.diagnostics.push(at(
                FrontendCode::DependencyCycle,
                span,
                consumer.parsed.source(),
                format!("the dependency route cycles: {chain}"),
            ));
            return Some(Reached::Cycle);
        }
        if let Some(&index) = self.by_location.get(&location) {
            return Some(Reached::Loaded(index));
        }
        let dependency = match self.read(&location) {
            Ok(manifest) => manifest,
            Err(failure) => {
                self.diagnostics.push(at(
                    FrontendCode::MissingManifest,
                    span,
                    consumer.parsed.source(),
                    format!(
                        "cannot read the dependency `{}`: {}",
                        use_.subject,
                        failure.describe()
                    ),
                ));
                return None;
            }
        };
        Some(Reached::Fresh(location, dependency))
    }

    fn admit_loaded(&mut self, index: usize, consumer: &Arc<ManifestFile>, use_: &ManifestUse) {
        if let Some(module) = self.modules.get(index)
            && let Some(diagnostic) =
                admission_diagnostic(&module.view, &module.location, consumer, use_)
        {
            self.diagnostics.push(diagnostic);
        }
    }

    /// A freshly read location: admission against what the clause asked,
    /// membership rules, then the dependency's own uses.
    fn admit_dependency(
        &mut self,
        location: S::Location,
        dependency: Arc<ManifestFile>,
        consumer: &Arc<ManifestFile>,
        use_: &ManifestUse,
    ) {
        let Some(view) = view_of(&dependency) else {
            return;
        };
        if let Some(diagnostic) = admission_diagnostic(&view, &location, consumer, use_) {
            self.diagnostics.push(diagnostic);
            return;
        }
        if let Some(nested) = view.workspace() {
            self.diagnostics.push(at(
                FrontendCode::NestedWorkspace,
                nested.span,
                dependency.parsed.source(),
                "a dependency declares no workspace; only the root lists members",
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
            sources: SourceSet::empty(),
        });
    }

    /// A dependency's registry bindings and domain routes are diagnosed and
    /// carry no authority. Both are consumer configuration under 0069: a
    /// module that is a root in its own checkout keeps them, and they stop
    /// meaning anything the moment something else selects it.
    fn refuse_supplied_authority(&mut self, view: &Manifest, source: &Arc<SourceFile>) {
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

    /// Record that `directory` declares the view's subject. A second
    /// location declaring it is refused even if the bytes match; the same
    /// location declaring it twice is one module seen twice.
    pub(super) fn declare_subject(
        &mut self,
        location: &S::Location,
        view: &Manifest,
        source: &Arc<SourceFile>,
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
    view: &Manifest,
    location: &impl std::fmt::Display,
    consumer: &ManifestFile,
    use_: &ManifestUse,
) -> Option<pith_diag::Diag> {
    let request = admission::Request::new(use_.subject.clone(), use_.range.clone());
    let refusal = admission::admit(&request, view).err()?;
    let span = match refusal.clause {
        admission::Clause::Subject { .. } => use_.subject_span,
        admission::Clause::Version { .. } => use_.range_span,
    };
    Some(at(
        FrontendCode::UnexpectedSubject,
        span,
        consumer.parsed.source(),
        format!(
            "`{}` binds {}: {} (acquired at `{location}`)",
            use_.alias, use_.subject, refusal
        ),
    ))
}
