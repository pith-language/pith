//! File-set acquisition: every resolved module's includes read through the
//! store, after resolution has decided which modules the closure holds.

use pith_hir::FrontendCode;

use super::super::acquire::ProjectStore;
use super::super::module::ModuleFile;
use super::{Resolution, at};

impl<S: ProjectStore> Resolution<S> {
    /// Acquire every resolved module's includes through the store, in
    /// canonical order. Refusals name the include clause that caused them.
    pub(in crate::workspace) fn acquire_file_sets(&mut self) {
        for index in 0..self.modules.len() {
            let (location, source, project_text, includes) = {
                let module = self
                    .modules
                    .get(index)
                    .unwrap_or_else(|| unreachable!("the index came from the module vector"));
                (
                    module.location.clone(),
                    module.file.parsed.source().clone(),
                    module.file.parsed.source().source_text().to_string(),
                    module.view.includes().to_vec(),
                )
            };
            let mut files = Vec::new();
            let mut refused = false;
            for include in &includes {
                match self.store.include(&location, &include.path) {
                    Ok(acquired) => files.push(ModuleFile::new(acquired.path, acquired.text)),
                    Err(failure) => {
                        let code = failure
                            .diagnostic_code()
                            .unwrap_or(FrontendCode::UnreadableSource);
                        self.diagnostics.push(at(
                            code,
                            include.span,
                            &source,
                            format!(
                                "cannot read the include `{}`, which the project names: {}",
                                include.path,
                                failure.describe()
                            ),
                        ));
                        refused = true;
                    }
                }
            }
            if refused {
                continue;
            }
            let files = super::super::module::ProjectFiles::new(project_text, files);
            let files = match files {
                Ok(files) => files,
                Err(error) => {
                    self.diagnostics.push(at(
                        FrontendCode::UnreadableSource,
                        self.modules
                            .get(index)
                            .map(|module| module.view.span())
                            .unwrap_or_else(pith_diag::Span::none),
                        &source,
                        format!("the file set at `{location}` is not canonical: {error}"),
                    ));
                    continue;
                }
            };
            if let Err(failure) = self.store.verify(&location) {
                let code = failure
                    .diagnostic_code()
                    .unwrap_or(FrontendCode::UnreadableSource);
                self.diagnostics.push(at(
                    code,
                    self.modules
                        .get(index)
                        .map(|module| module.view.span())
                        .unwrap_or_else(pith_diag::Span::none),
                    &source,
                    failure.describe(),
                ));
                continue;
            }
            if let Some(module) = self.modules.get_mut(index) {
                module.files = files;
            }
        }
    }
}
