use std::path::Path;

use crate::files;
use crate::report::{Diagnostic, Report};

pub(crate) fn run(root: &Path) -> Report {
    let mut report = Report::new("determinism", "no HashMap use in crate source");
    let (paths, walk_diagnostics) = files::with_extension(root, Path::new("crates"), "rs");
    report.extend(walk_diagnostics);

    for path in paths {
        if path
            .components()
            .any(|component| component.as_os_str() == "snapshots")
        {
            continue;
        }

        let source = match std::fs::read_to_string(root.join(&path)) {
            Ok(source) => source,
            Err(error) => {
                report.push(Diagnostic::file(
                    &path,
                    format!("could not read source: {error}"),
                ));
                continue;
            }
        };

        for (line_index, line) in source.lines().enumerate() {
            if contains_hash_map(line) {
                let line_number = line_index.checked_add(1).unwrap_or(line_index);
                report.push(Diagnostic::line(
                    &path,
                    line_number,
                    "HashMap is forbidden by decision 0021",
                ));
            }
        }
    }

    report.sort_diagnostics();
    report
}

fn contains_hash_map(line: &str) -> bool {
    line.contains("HashMap<")
        || line.contains("use std::collections::HashMap")
        || line.contains("use ::std::collections::HashMap")
}
