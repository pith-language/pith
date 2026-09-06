//! The declaration responsible for an effective authority binding.

use std::sync::Arc;

use pith_diag::{Diag, Severity, SourceFile, Span};
use pith_hir::FrontendCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingOrigin {
    Project,
    User,
}

#[derive(Clone, Debug)]
pub struct BindingSite {
    origin: BindingOrigin,
    source: Arc<SourceFile>,
    span: Span,
}

impl BindingSite {
    pub(super) fn new(origin: BindingOrigin, source: &Arc<SourceFile>, span: Span) -> Self {
        Self {
            origin,
            source: Arc::clone(source),
            span,
        }
    }

    #[must_use]
    pub const fn origin(&self) -> BindingOrigin {
        self.origin
    }

    #[must_use]
    pub fn source(&self) -> &Arc<SourceFile> {
        &self.source
    }

    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    pub(super) fn warning(&self, message: impl Into<Box<str>>) -> Diag {
        Diag::new(
            Severity::Warning,
            FrontendCode::DuplicateRegistry.stable(),
            self.span,
            message,
        )
        .with_source(Arc::clone(&self.source))
    }
}

/// Both declarations survive a project override, including their source identities.
#[derive(Clone, Debug)]
pub struct BindingOverride {
    pub(super) name: Box<str>,
    pub(super) project: BindingSite,
    pub(super) user: BindingSite,
}

impl BindingOverride {
    #[must_use]
    pub fn project(&self) -> &BindingSite {
        &self.project
    }

    #[must_use]
    pub fn user(&self) -> &BindingSite {
        &self.user
    }

    #[must_use]
    pub fn diagnostics(&self) -> [Diag; 2] {
        [
            self.project.warning(format!(
                "project binding for {} wins over the user binding in `{}`",
                self.name, self.user.source.label
            )),
            self.user.warning(format!(
                "user binding for {} is shadowed by the project binding in `{}`",
                self.name, self.project.source.label
            )),
        ]
    }
}
