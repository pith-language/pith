//! Project precedence without retargeting user-owned domain routes.

use std::collections::{BTreeMap, btree_map::Entry};

use super::Routing;
use super::layer::{Domain, Layer, Project, Registry, UserBindings};
use super::provenance::{BindingOverride, BindingSite};

/// The authority-origin requirement; network and lock permissions are separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingPolicy {
    AllowUser,
    ProjectOnly,
}

impl Routing {
    pub(super) fn merge(
        project: Layer<Project>,
        user: Option<UserBindings>,
        policy: BindingPolicy,
    ) -> Self {
        let (registries, domains) = user.map_or_else(
            || (BTreeMap::new(), BTreeMap::new()),
            |user| (user.0.registries, user.0.domains),
        );
        let mut overrides = Vec::new();
        let registries = overlay(
            registries,
            project.registries,
            |key| format!("registry `{key}`"),
            |binding: &std::sync::Arc<Registry>| &binding.site,
            &mut overrides,
        );
        let domains = overlay(
            domains,
            project.domains,
            |key: &pith_hir::SubjectSegment| format!("domain `{}`", key.as_str()),
            |binding: &Domain| &binding.site,
            &mut overrides,
        );
        Self {
            registries,
            domains,
            overrides,
            policy,
        }
    }
}

fn overlay<K: Ord, V>(
    mut user: BTreeMap<K, V>,
    project: BTreeMap<K, V>,
    name: impl Fn(&K) -> String,
    site: impl Fn(&V) -> &BindingSite,
    overrides: &mut Vec<BindingOverride>,
) -> BTreeMap<K, V> {
    for (key, value) in project {
        match user.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(value);
            }
            Entry::Occupied(mut entry) => {
                overrides.push(BindingOverride {
                    name: name(entry.key()).into(),
                    project: site(&value).clone(),
                    user: site(entry.get()).clone(),
                });
                entry.insert(value);
            }
        }
    }
    user
}
