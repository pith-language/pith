//! Only a configured root can start dependency traversal.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use pith_diag::{Diag, SourceId};
use pith_hir::FrontendCode;

use super::super::acquire::ModuleStore;
use super::super::module::SourceSet;
use super::super::routing::{BindingPolicy, Routing, UserBindings};
use super::{ManifestFile, PendingModule, Resolution, sourceless};
use crate::{ManifestSource, parse_manifest};

impl<S: ModuleStore> Resolution<S> {
    pub(in crate::workspace) fn prepare(
        mut store: S,
        location: &S::Location,
        user: Option<UserBindings>,
        policy: BindingPolicy,
    ) -> Result<Self, Box<[Diag]>> {
        let root_id = SourceId::from_raw(u32::from(user.is_some()));
        let acquired = store.manifest(location).map_err(|failure| {
            Box::from([sourceless(
                FrontendCode::MissingManifest,
                failure.describe(),
            )])
        })?;
        let root = Arc::new(ManifestFile {
            parsed: parse_manifest(&ManifestSource::new(root_id, acquired.label, acquired.text)),
        });
        let view = root
            .parsed
            .validated()
            .map_err(|_| Box::from(root.parsed.diagnostics()))?;
        let routing =
            Routing::from_root(&view, root.parsed.source(), user, policy).map_err(Box::from)?;
        let diagnostics: Vec<_> = view
            .uses()
            .iter()
            .filter_map(|use_| routing.route(use_, root.parsed.source()).err())
            .collect();
        if !diagnostics.is_empty() {
            return Err(diagnostics.into());
        }
        let mut resolution = Self {
            store,
            routing,
            claims: super::claims::Claims::default(),
            modules: Vec::new(),
            by_location: BTreeMap::new(),
            subjects: BTreeMap::new(),
            stack: Vec::new(),
            stack_locations: BTreeSet::new(),
            read: BTreeMap::from([(location.clone(), Arc::clone(&root))]),
            next_source_id: root_id.to_raw().saturating_add(1),
            diagnostics: Vec::new(),
        };
        resolution.declare_subject(location, &view, root.parsed.source());
        resolution.validate_members(location, root.parsed.source(), &view);
        resolution.walk(PendingModule {
            location: location.clone(),
            file: root,
            view,
            sources: SourceSet::empty(),
        });
        Ok(resolution)
    }
}
