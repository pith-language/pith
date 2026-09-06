//! Source acquisition: every resolved module's source set read through the
//! store, after resolution has decided which modules the closure holds.

use pith_hir::FrontendCode;

use super::super::acquire::{AcquireFailure, ModuleStore};
use super::super::module::{ModuleFile, SourceSet};
use super::{Resolution, at};

impl<S: ModuleStore> Resolution<S> {
    /// Acquire every resolved module's source set through the store, in
    /// canonical module-relative order. Refusals name the manifest that
    /// owns the tree.
    pub(in crate::workspace) fn acquire_source_sets(&mut self) {
        for index in 0..self.modules.len() {
            let (location, source, span) = {
                let module = self
                    .modules
                    .get(index)
                    .unwrap_or_else(|| unreachable!("the index came from the module vector"));
                (
                    module.location.clone(),
                    module.file.parsed.source().clone(),
                    module.view.span(),
                )
            };
            let acquired = match self.store.sources(&location) {
                Ok(acquired) => acquired,
                Err(AcquireFailure::Symlink { path }) => {
                    self.diagnostics.push(at(
                        FrontendCode::SymlinkedSource,
                        span,
                        &source,
                        format!(
                            "`{path}` is a symlink, and a module owns its own source tree; a \
                             dependency on that tree is a `use` clause with a path"
                        ),
                    ));
                    continue;
                }
                Err(AcquireFailure::Irregular { path }) => {
                    self.diagnostics.push(at(
                        FrontendCode::IrregularSource,
                        span,
                        &source,
                        format!(
                            "`{path}` is not a regular file, and a module owns regular files \
                             under src/"
                        ),
                    ));
                    continue;
                }
                Err(failure) => {
                    let code = match failure {
                        AcquireFailure::Refused { code, .. } => code,
                        _ => FrontendCode::UnreadableSource,
                    };
                    self.diagnostics.push(at(
                        code,
                        span,
                        &source,
                        format!(
                            "cannot read the source set of `{location}`: {}",
                            failure.describe()
                        ),
                    ));
                    continue;
                }
            };
            if acquired.is_empty() {
                self.diagnostics.push(at(
                    FrontendCode::EmptySourceSet,
                    span,
                    &source,
                    format!("the module at `{location}` owns no source files under src/"),
                ));
                continue;
            }
            let sources = SourceSet::new(
                acquired
                    .into_iter()
                    .map(|file| ModuleFile::new(file.path, file.text)),
            );
            let sources = match sources {
                Ok(sources) => sources,
                Err(error) => {
                    self.diagnostics.push(at(
                        FrontendCode::UnreadableSource,
                        span,
                        &source,
                        format!(
                            "the store at `{location}` served a source set that is not \
                             canonical: {error}"
                        ),
                    ));
                    continue;
                }
            };
            if let Some(module) = self.modules.get_mut(index) {
                module.sources = sources;
            }
        }
    }
}
