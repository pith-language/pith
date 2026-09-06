//! One source claim per subject across the dependency closure.

use std::collections::BTreeMap;
use std::sync::Arc;

use pith_diag::{Diag, SourceFile, Span};
use pith_hir::{FrontendCode, ModuleSubject, RootKey};

use super::super::acquire::{AcquireFailure, ModuleStore, Route};
use super::at;

#[derive(PartialEq, Eq)]
enum Identity<L> {
    Path(L),
    Registry {
        name: Box<str>,
        root_key: RootKey,
    },
    Git {
        revision: Box<str>,
        subpath: Option<Box<str>>,
    },
    Archive(Box<str>),
}

struct Claim<L> {
    identity: Identity<L>,
    location: L,
    source: Arc<SourceFile>,
    span: Span,
}

pub(super) struct Claims<L>(BTreeMap<ModuleSubject, Claim<L>>);

impl<L> Default for Claims<L> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

pub(super) enum Failure {
    Acquire(AcquireFailure),
    Conflict(Box<[Diag; 2]>),
}

impl<L: Clone + Ord> Claims<L> {
    pub(super) fn locate<S: ModuleStore<Location = L>>(
        &mut self,
        store: &S,
        base: &L,
        route: &Route<'_>,
        subject: &ModuleSubject,
        source: &Arc<SourceFile>,
        span: Span,
    ) -> Result<L, Failure> {
        let (identity, located) = Identity::locate(store, base, route).map_err(Failure::Acquire)?;
        if let Some(prior) = self.0.get(subject) {
            return prior.agree(&identity, subject, source, span);
        }

        let location = match located {
            Some(location) => location,
            None => store.locate(base, route).map_err(Failure::Acquire)?,
        };
        self.0.insert(
            subject.clone(),
            Claim {
                identity,
                location: location.clone(),
                source: Arc::clone(source),
                span,
            },
        );
        Ok(location)
    }
}

impl<L: Clone + Ord> Identity<L> {
    fn locate<S: ModuleStore<Location = L>>(
        store: &S,
        base: &L,
        route: &Route<'_>,
    ) -> Result<(Self, Option<L>), AcquireFailure> {
        Ok(match route {
            Route::Path { .. } | Route::Member { .. } => {
                let location = store.locate(base, route)?;
                (Identity::Path(location.clone()), Some(location))
            }
            Route::Registry(registry) => (
                Identity::Registry {
                    name: registry.name().into(),
                    root_key: registry.root_key().clone(),
                },
                None,
            ),
            Route::Git {
                revision, subpath, ..
            } => (
                Identity::Git {
                    revision: (*revision).into(),
                    subpath: subpath.map(Into::into),
                },
                None,
            ),
            Route::Archive { digest, .. } => (Identity::Archive((*digest).into()), None),
        })
    }
}

impl<L: Clone + PartialEq> Claim<L> {
    fn agree(
        &self,
        identity: &Identity<L>,
        subject: &ModuleSubject,
        source: &Arc<SourceFile>,
        span: Span,
    ) -> Result<L, Failure> {
        if &self.identity == identity {
            return Ok(self.location.clone());
        }
        Err(Failure::Conflict(Box::new([
            at(
                FrontendCode::ConflictingRoutes,
                span,
                source,
                format!(
                    "the route for `{subject}` conflicts with its route in `{}`",
                    self.source.label
                ),
            ),
            at(
                FrontendCode::ConflictingRoutes,
                self.span,
                &self.source,
                format!("the other route for `{subject}` is declared here"),
            ),
        ])))
    }
}
