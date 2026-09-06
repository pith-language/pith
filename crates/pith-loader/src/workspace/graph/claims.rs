//! One canonical source per subject across the dependency closure.

use std::collections::{BTreeMap, btree_map::Entry};
use std::sync::Arc;

use pith_diag::{Diag, SourceFile, Span};
use pith_hir::{FrontendCode, ModuleSubject, RootKey};

use super::super::acquire::{AcquireFailure, ModuleStore, Route};
use super::at;

#[derive(PartialEq, Eq)]
enum Authority {
    Path,
    Registry(RootKey),
    Git,
    Archive,
}

#[derive(PartialEq, Eq)]
struct LocatedSource<L> {
    location: L,
    authority: Authority,
}

impl<L> LocatedSource<L> {
    fn acquire<S: ModuleStore<Location = L>>(
        store: &S,
        base: &L,
        route: &Route<'_>,
    ) -> Result<Self, AcquireFailure> {
        let location = store.locate(base, route)?;
        let authority = match route {
            Route::Path { .. } | Route::Member { .. } => Authority::Path,
            Route::Registry(registry) => Authority::Registry(registry.root_key().clone()),
            Route::Git { .. } => Authority::Git,
            Route::Archive { .. } => Authority::Archive,
        };
        Ok(Self {
            location,
            authority,
        })
    }
}

struct Claim<L> {
    located: LocatedSource<L>,
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
        let located = LocatedSource::acquire(store, base, route).map_err(Failure::Acquire)?;
        match self.0.entry(subject.clone()) {
            Entry::Occupied(prior) => prior.get().agree(&located, subject, source, span),
            Entry::Vacant(entry) => {
                let claim = entry.insert(Claim {
                    located,
                    source: Arc::clone(source),
                    span,
                });
                Ok(claim.located.location.clone())
            }
        }
    }
}

impl<L: Clone + PartialEq> Claim<L> {
    fn agree(
        &self,
        located: &LocatedSource<L>,
        subject: &ModuleSubject,
        source: &Arc<SourceFile>,
        span: Span,
    ) -> Result<L, Failure> {
        if &self.located == located {
            return Ok(self.located.location.clone());
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

#[cfg(test)]
mod tests;
