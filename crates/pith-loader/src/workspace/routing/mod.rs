//! Consumer-owned registry routes, validated before dependency acquisition.

mod layer;
mod merge;
mod provenance;

use std::collections::BTreeMap;
use std::sync::Arc;

use pith_diag::{Diag, SourceFile};
use pith_hir::{
    FrontendCode, InputLocator, ModuleSubject, Project, ProjectInput, RootKey, SubjectSegment,
};

use super::acquire::Route;
use super::graph::at;
pub use layer::UserBindings;
use layer::{Domain, Layer, Registry};
pub use merge::BindingPolicy;
pub use provenance::{BindingOrigin, BindingOverride, BindingSite};

/// A registry selected by consumer configuration for one subject. Its
/// fields cannot be constructed from a dependency's registry clause.
pub struct RegistryRoute<'a> {
    subject: &'a ModuleSubject,
    binding: &'a Registry,
    domain: Option<&'a BindingSite>,
}

impl RegistryRoute<'_> {
    #[must_use]
    pub fn subject(&self) -> &ModuleSubject {
        self.subject
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.binding.declaration.name
    }

    #[must_use]
    pub fn locator(&self) -> &str {
        &self.binding.declaration.locator
    }

    #[must_use]
    pub fn root_key(&self) -> &RootKey {
        &self.binding.declaration.root_key
    }
    #[must_use]
    pub fn registry_site(&self) -> &BindingSite {
        &self.binding.site
    }

    #[must_use]
    pub fn domain_site(&self) -> Option<&BindingSite> {
        self.domain
    }
}

pub(super) struct Routing {
    registries: BTreeMap<Box<str>, Arc<Registry>>,
    domains: BTreeMap<SubjectSegment, Domain>,
    overrides: Vec<BindingOverride>,
    policy: BindingPolicy,
}

impl Routing {
    pub(super) fn from_root(
        root: &Project,
        source: &Arc<SourceFile>,
        user: Option<UserBindings>,
        policy: BindingPolicy,
    ) -> Result<Self, Vec<Diag>> {
        Layer::project(root, source).map(|project| Self::merge(project, user, policy))
    }

    pub(super) fn into_overrides(self) -> Box<[BindingOverride]> {
        self.overrides.into()
    }

    pub(super) fn route<'a>(
        &'a self,
        input: &'a ProjectInput,
        source: &Arc<SourceFile>,
    ) -> Result<Route<'a>, Diag> {
        Ok(match &input.locator {
            InputLocator::Registry {
                subject, registry, ..
            } => Route::Registry(self.registry(subject, registry.as_deref(), input, source)?),
            InputLocator::Path { path, .. } => Route::Path { path },
            InputLocator::Git {
                url,
                revision,
                subpath,
                ..
            } => Route::Git {
                url,
                revision,
                subpath: subpath.as_deref(),
            },
            InputLocator::Archive { url, digest, .. } => Route::Archive { url, digest },
        })
    }

    fn registry<'a>(
        &'a self,
        subject: &'a ModuleSubject,
        name: Option<&str>,
        input: &ProjectInput,
        source: &Arc<SourceFile>,
    ) -> Result<RegistryRoute<'a>, Diag> {
        let selected = match name {
            Some(name) => self
                .registries
                .get(name)
                .map(|registry| (registry.as_ref(), None)),
            None => self
                .domains
                .get(subject.domain())
                .map(|domain| (domain.registry.as_ref(), Some(&domain.site))),
        };
        let (binding, domain) =
            selected.ok_or_else(|| self.unrouted(subject, name, input, source))?;
        if matches!(self.policy, BindingPolicy::ProjectOnly)
            && std::iter::once(&binding.site)
                .chain(domain)
                .any(|site| site.origin() == BindingOrigin::User)
        {
            return Err(at(
                FrontendCode::ModeExceeded,
                input.locator.span(),
                source,
                format!(
                    "project-only authority refuses user-owned binding for `{subject}` from `{}`",
                    binding.site.source().label
                ),
            ));
        }
        Ok(RegistryRoute {
            subject,
            binding,
            domain,
        })
    }

    fn unrouted(
        &self,
        subject: &ModuleSubject,
        name: Option<&str>,
        input: &ProjectInput,
        source: &Arc<SourceFile>,
    ) -> Diag {
        let configured = self
            .domains
            .iter()
            .map(|(domain, binding)| {
                format!(
                    "{} from {}",
                    domain.as_str(),
                    binding.registry.declaration.name
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let requested = name.map_or_else(
            || format!("domain `{}`", subject.domain().as_str()),
            |name| format!("registry `{name}`"),
        );
        at(
            FrontendCode::UnroutedDomain,
            input.locator.span(),
            source,
            format!(
                "no registry configured for {requested}; configured domain bindings: [{configured}]"
            ),
        )
    }
}
