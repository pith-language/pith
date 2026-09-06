//! Consumer-owned registry routes, validated before dependency acquisition.

use std::collections::{BTreeMap, btree_map::Entry};
use std::sync::Arc;

use pith_diag::{Diag, SourceFile};
use pith_hir::{
    DependencySource, FrontendCode, Manifest, ManifestRegistry, ManifestUse, ModuleSubject,
    RootKey, SubjectSegment,
};

use super::acquire::Route;
use super::graph::at;

/// A registry selected by consumer configuration for one subject.
/// Its fields cannot be constructed from a dependency's registry clause.
pub struct RegistryRoute<'a> {
    subject: &'a ModuleSubject,
    binding: &'a ManifestRegistry,
}

impl RegistryRoute<'_> {
    #[must_use]
    pub fn subject(&self) -> &ModuleSubject {
        self.subject
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.binding.name
    }

    #[must_use]
    pub fn locator(&self) -> &str {
        &self.binding.locator
    }

    #[must_use]
    pub fn root_key(&self) -> &RootKey {
        &self.binding.root_key
    }
}

#[derive(Default)]
pub(super) struct Routing {
    registries: BTreeMap<Box<str>, ManifestRegistry>,
    domains: BTreeMap<SubjectSegment, ManifestRegistry>,
}

impl Routing {
    pub(super) fn from_root(root: &Manifest, source: &Arc<SourceFile>) -> Result<Self, Vec<Diag>> {
        let registries: BTreeMap<_, _> = root
            .registries()
            .iter()
            .map(|binding| (binding.name.clone(), binding.clone()))
            .collect();
        let mut domains = BTreeMap::new();
        let mut declared = BTreeMap::new();
        let mut diagnostics = Vec::new();
        for domain in root.domains() {
            match declared.entry(domain.domain.clone()) {
                Entry::Occupied(prior) => {
                    diagnostics.push(
                        at(
                            FrontendCode::DuplicateRegistry,
                            domain.span,
                            source,
                            format!(
                                "the domain `{}` has two registry bindings",
                                domain.domain.as_str()
                            ),
                        )
                        .with_note(*prior.get(), "the first binding is here"),
                    );
                    continue;
                }
                Entry::Vacant(entry) => {
                    entry.insert(domain.span);
                }
            }
            match registries.get(&domain.registry) {
                Some(binding) => {
                    domains.insert(domain.domain.clone(), binding.clone());
                }
                None => diagnostics.push(at(
                    FrontendCode::UnroutedDomain,
                    domain.registry_span,
                    source,
                    format!(
                        "domain `{}` names unconfigured registry `{}`",
                        domain.domain.as_str(),
                        domain.registry
                    ),
                )),
            }
        }
        if diagnostics.is_empty() {
            Ok(Self {
                registries,
                domains,
            })
        } else {
            Err(diagnostics)
        }
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
        let binding = match name {
            Some(name) => self.registries.get(name),
            None => self.domains.get(use_.subject.domain()),
        };
        binding.map(|binding| RegistryRoute { subject: &use_.subject, binding }).ok_or_else(|| {
            let configured = self.domains.iter().map(|(domain, binding)|
                format!("{} from {}", domain.as_str(), binding.name)).collect::<Vec<_>>().join(", ");
            let requested = name.map_or_else(|| format!("domain `{}`", use_.subject.domain().as_str()), |name| format!("registry `{name}`"));
            at(FrontendCode::UnroutedDomain, use_.source_span(), source,
                format!("no registry configured for {requested}; configured domain bindings: [{configured}]"))
        })
    }
}
