//! Registry names are resolved inside their declaring configuration layer.

use std::collections::{BTreeMap, btree_map::Entry};
use std::marker::PhantomData;
use std::sync::Arc;

use pith_diag::{Diag, SourceFile, SourceId};
use pith_hir::{FrontendCode, Manifest, ManifestRegistry, SubjectSegment};

use super::super::graph::at;
use super::provenance::{BindingOrigin, BindingSite};
use crate::ParsedManifestFile;

pub(super) enum Project {}
pub(super) enum User {}

pub(super) trait Owner {
    const ORIGIN: BindingOrigin;
}
impl Owner for Project {
    const ORIGIN: BindingOrigin = BindingOrigin::Project;
}
impl Owner for User {
    const ORIGIN: BindingOrigin = BindingOrigin::User;
}

#[derive(Clone)]
pub(super) struct Registry {
    pub(super) declaration: ManifestRegistry,
    pub(super) site: BindingSite,
}

#[derive(Clone)]
pub(super) struct Domain {
    pub(super) registry: Arc<Registry>,
    pub(super) site: BindingSite,
}

pub(super) struct Layer<Owner> {
    pub(super) registries: BTreeMap<Box<str>, Arc<Registry>>,
    pub(super) domains: BTreeMap<SubjectSegment, Domain>,
    owner: PhantomData<Owner>,
}

impl Layer<Project> {
    pub(super) fn project(
        manifest: &Manifest,
        source: &Arc<SourceFile>,
    ) -> Result<Self, Vec<Diag>> {
        Self::validate(manifest, source)
    }
}

/// Authority supplied explicitly by the caller; loading never discovers a user file.
pub struct UserBindings(pub(super) Layer<User>);

impl TryFrom<&ParsedManifestFile> for UserBindings {
    type Error = Box<[Diag]>;

    fn try_from(parsed: &ParsedManifestFile) -> Result<Self, Self::Error> {
        let manifest = parsed
            .validated()
            .map_err(|_| Box::from(parsed.diagnostics()))?;
        let source = Arc::new(SourceFile::new(
            SourceId::from_raw(0),
            parsed.source().label.clone(),
            parsed.source().source_text(),
        ));
        Layer::validate(&manifest, &source)
            .map(Self)
            .map_err(Into::into)
    }
}

impl<O: Owner> Layer<O> {
    fn validate(manifest: &Manifest, source: &Arc<SourceFile>) -> Result<Self, Vec<Diag>> {
        let registries: BTreeMap<_, _> = manifest
            .registries()
            .iter()
            .map(|declaration| {
                (
                    declaration.name.clone(),
                    Arc::new(Registry {
                        site: BindingSite::new(O::ORIGIN, source, declaration.span),
                        declaration: declaration.clone(),
                    }),
                )
            })
            .collect();
        let mut domains = BTreeMap::new();
        let mut declared = BTreeMap::new();
        let mut diagnostics = Vec::new();
        for domain in manifest.domains() {
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
                Some(registry) => {
                    domains.insert(
                        domain.domain.clone(),
                        Domain {
                            registry: Arc::clone(registry),
                            site: BindingSite::new(O::ORIGIN, source, domain.span),
                        },
                    );
                }
                None => diagnostics.push(at(
                    FrontendCode::UnroutedDomain,
                    domain.registry_span,
                    source,
                    format!(
                        "domain `{}` names unconfigured registry `{}` in this configuration layer",
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
                owner: PhantomData,
            })
        } else {
            Err(diagnostics)
        }
    }
}
