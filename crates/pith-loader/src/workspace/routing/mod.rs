//! Consumer-owned registry routes, validated before dependency acquisition.

mod layer;
mod merge;
mod provenance;

use std::collections::BTreeMap;
use std::sync::Arc;

use pith_diag::{Diag, SourceFile};
use pith_hir::{
    DependencySource, FrontendCode, Manifest, ManifestUse, ModuleSubject, RootKey, SubjectSegment,
};

use super::acquire::Route;
use super::graph::at;
pub use layer::UserBindings;
use layer::{Domain, Layer, Registry};
pub use merge::BindingPolicy;
pub use provenance::{BindingOrigin, BindingOverride, BindingSite};

/// A registry selected by consumer configuration for one subject.
/// Its fields cannot be constructed from a dependency's registry clause.
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
        root: &Manifest,
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
        use_: &'a ManifestUse,
        source: &Arc<SourceFile>,
    ) -> Result<Route<'a>, Diag> {
        let subject = &use_.subject;
        Ok(match &use_.source {
            DependencySource::Registry { registry, .. } => {
                Route::Registry(self.registry(use_, registry.as_deref(), source)?)
            }
            DependencySource::Path { path, .. } => Route::Path { subject, path },
            DependencySource::Git {
                url,
                revision,
                subpath,
                ..
            } => Route::Git {
                subject,
                url,
                revision,
                subpath: subpath.as_deref(),
            },
            DependencySource::Archive { url, digest, .. } => Route::Archive {
                subject,
                url,
                digest,
            },
        })
    }

    fn registry<'a>(
        &'a self,
        use_: &'a ManifestUse,
        name: Option<&str>,
        source: &Arc<SourceFile>,
    ) -> Result<RegistryRoute<'a>, Diag> {
        let selected = match name {
            Some(name) => self
                .registries
                .get(name)
                .map(|registry| (registry.as_ref(), None)),
            None => self
                .domains
                .get(use_.subject.domain())
                .map(|domain| (domain.registry.as_ref(), Some(&domain.site))),
        };
        let (binding, domain) = selected.ok_or_else(|| self.unrouted(use_, name, source))?;
        if matches!(self.policy, BindingPolicy::ProjectOnly)
            && std::iter::once(&binding.site)
                .chain(domain)
                .any(|site| site.origin() == BindingOrigin::User)
        {
            return Err(at(
                FrontendCode::ModeExceeded,
                use_.source_span(),
                source,
                format!(
                    "project-only authority refuses user-owned binding for `{}` from `{}`",
                    use_.subject,
                    binding.site.source().label
                ),
            ));
        }
        Ok(RegistryRoute {
            subject: &use_.subject,
            binding,
            domain,
        })
    }

    fn unrouted(&self, use_: &ManifestUse, name: Option<&str>, source: &Arc<SourceFile>) -> Diag {
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
            || format!("domain `{}`", use_.subject.domain().as_str()),
            |name| format!("registry `{name}`"),
        );
        at(
            FrontendCode::UnroutedDomain,
            use_.source_span(),
            source,
            format!(
                "no registry configured for {requested}; configured domain bindings: [{configured}]"
            ),
        )
    }
}
