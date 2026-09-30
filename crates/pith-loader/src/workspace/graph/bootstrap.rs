//! Only a configured root can start dependency traversal.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use pith_diag::{Diag, SourceId, Span};
use pith_hir::FrontendCode;

use super::super::acquire::ProjectStore;
use super::super::module::ProjectFiles;
use super::super::routing::{BindingPolicy, Routing, UserBindings};
use super::{PendingModule, ProjectFile, Resolution, at, sourceless};
use crate::{ProjectSource, parse_project_file};

impl<S: ProjectStore> Resolution<S> {
    pub(in crate::workspace) fn prepare(
        mut store: S,
        location: &S::Location,
        user: Option<UserBindings>,
        policy: BindingPolicy,
    ) -> Result<Self, Box<[Diag]>> {
        let root_id = SourceId::from_raw(u32::from(user.is_some()));
        let acquired = store.project(location).map_err(|failure| {
            Box::from([sourceless(FrontendCode::MissingProject, failure.describe())])
        })?;
        let root = Arc::new(ProjectFile {
            parsed: parse_project_file(&ProjectSource::new(root_id, acquired.label, acquired.text)),
        });
        let view = root.parsed.validated().map_err(|_| {
            let mut diagnostics = root.parsed.diagnostics().to_vec();
            if root.parsed.header().module.is_none() {
                diagnostics.push(at(
                    FrontendCode::MissingSubject,
                    Span::point(pith_diag::ByteOffset(
                        u32::try_from(root.parsed.source().source_text().len()).unwrap_or(0),
                    )),
                    root.parsed.source(),
                    "the root project declares no `module` clause, and a loaded project names \
                     its subject",
                ));
            }
            Box::<[Diag]>::from(diagnostics)
        })?;
        let routing =
            Routing::from_root(&view, root.parsed.source(), user, policy).map_err(Box::from)?;
        let diagnostics: Vec<_> = view
            .inputs()
            .iter()
            .filter_map(|input| routing.route(input, root.parsed.source()).err())
            .collect();
        if !diagnostics.is_empty() {
            return Err(diagnostics.into());
        }
        if let Some(host) = view.host() {
            return Err(Box::from([at(
                FrontendCode::UnboundHost,
                host.span,
                root.parsed.source(),
                "the `host` clause names the component that serves this project's `= host` \
                 rules, and no component binding exists",
            )]));
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
            files: ProjectFiles::unacquired(),
            bindings: Vec::new(),
        });
        Ok(resolution)
    }
}
