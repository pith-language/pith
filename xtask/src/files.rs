use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::report::Diagnostic;

pub(crate) fn with_extension(
    root: &Path,
    relative_directory: &Path,
    extension: &str,
) -> (Vec<PathBuf>, Vec<Diagnostic>) {
    let mut paths = Vec::new();
    let mut diagnostics = Vec::new();
    let directory = root.join(relative_directory);

    for result in WalkBuilder::new(&directory).standard_filters(true).build() {
        let entry = match result {
            Ok(entry) => entry,
            Err(error) => {
                diagnostics.push(Diagnostic::file(
                    relative_directory,
                    format!("could not walk directory: {error}"),
                ));
                continue;
            }
        };

        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            continue;
        }
        if entry.path().extension() != Some(OsStr::new(extension)) {
            continue;
        }

        match entry.path().strip_prefix(root) {
            Ok(relative_path) => paths.push(relative_path.to_path_buf()),
            Err(error) => diagnostics.push(Diagnostic::file(
                entry.path(),
                format!("could not make path relative to workspace: {error}"),
            )),
        }
    }

    paths.sort();
    (paths, diagnostics)
}
