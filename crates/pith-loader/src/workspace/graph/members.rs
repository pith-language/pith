//! Workspace membership: the root's member list validated before the
//! dependency walk, so a member cannot inject itself into the closure.

use std::collections::BTreeSet;
use std::sync::Arc;

use pith_diag::SourceFile;
use pith_hir::{FrontendCode, Project};

use super::super::acquire::{ProjectStore, Route};
use super::{Resolution, at, view_of};

impl<S: ProjectStore> Resolution<S> {
    /// Validate the workspace members the root lists: each names a project
    /// directory, uniquely, declaring no workspace of its own and no
    /// subject another location declared. Member files are not acquired;
    /// only the selected dependency closure reaches acquisition.
    pub(super) fn validate_members(
        &mut self,
        root_location: &S::Location,
        root_source: &Arc<SourceFile>,
        root: &Project,
    ) {
        let Some(workspace) = root.workspace() else {
            return;
        };
        let mut listed = BTreeSet::new();
        for member in &workspace.members {
            // A member is a location, so it is reached through the same
            // store a path input takes.
            let route = Route::Member {
                path: member.path.as_ref(),
            };
            let location = match self.store.locate(root_location, &route) {
                Ok(location) => location,
                Err(failure) => {
                    let code = failure
                        .diagnostic_code()
                        .unwrap_or(FrontendCode::MissingProject);
                    self.diagnostics.push(at(
                        code,
                        member.span,
                        root_source,
                        format!(
                            "the member path `{}` names no directory: {}",
                            member.path,
                            failure.describe()
                        ),
                    ));
                    continue;
                }
            };
            if !listed.insert(location.clone()) {
                self.diagnostics.push(at(
                    FrontendCode::DuplicateMember,
                    member.span,
                    root_source,
                    format!("the workspace lists the member `{}` twice", member.path),
                ));
                continue;
            }
            let member_project = match self.read(&location) {
                Ok(file) => file,
                Err(failure) => {
                    self.diagnostics.push(at(
                        FrontendCode::MissingProject,
                        member.span,
                        root_source,
                        format!(
                            "cannot read the member at `{}`: {}",
                            member.path,
                            failure.describe()
                        ),
                    ));
                    continue;
                }
            };
            let Some(view) = view_of(&member_project) else {
                if member_project.parsed.diagnostics().is_empty() {
                    self.diagnostics.push(at(
                        FrontendCode::MissingSubject,
                        member.span,
                        root_source,
                        format!(
                            "the member at `{}` declares no `module` clause, and a member \
                             names its subject",
                            member.path
                        ),
                    ));
                }
                continue;
            };
            if let Some(nested) = view.workspace() {
                self.diagnostics.push(at(
                    FrontendCode::NestedWorkspace,
                    nested.span,
                    member_project.parsed.source(),
                    "a member declares no workspace of its own; membership is a root \
                     concern",
                ));
            }
            self.declare_subject(&location, &view, member_project.parsed.source());
        }
    }
}
