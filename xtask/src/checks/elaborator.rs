use std::path::Path;

use blake3::Hasher;

use crate::report::{Diagnostic, Report};

const DIGEST_FILE: &str = "crates/pith-loader/elaborator-digest";
const VERSION_FILE: &str = "crates/pith-loader/src/graph/mod.rs";
const FRONTEND_CRATES: &[&str] = &[
    "crates/pith-elaborator",
    "crates/pith-hir",
    "crates/pith-loader",
    "crates/pith-syntax",
];

pub(crate) fn run(root: &Path) -> Report {
    let mut report = Report::new(
        "elaborator",
        "the elaborator digest matches ELABORATOR_SEMANTIC_VERSION",
    );
    let recorded = match read_recorded(root) {
        Ok(recorded) => recorded,
        Err(diagnostic) => {
            report.push(diagnostic);
            return report;
        }
    };
    let version = match declared_version(root) {
        Ok(version) => version,
        Err(diagnostic) => {
            report.push(diagnostic);
            return report;
        }
    };
    let digest = match source_digest(root) {
        Ok(digest) => digest,
        Err(diagnostic) => {
            report.push(diagnostic);
            return report;
        }
    };

    if recorded.version != version {
        report.push(Diagnostic::file(
            DIGEST_FILE,
            format!(
                "records version {} while the sources declare {version}; rerun \
                 `cargo run -p xtask -- record-elaborator`",
                recorded.version
            ),
        ));
    }
    if recorded.digest != digest {
        report.push(Diagnostic::file(
            DIGEST_FILE,
            format!(
                "the elaborator changed under ELABORATOR_SEMANTIC_VERSION {version}; bump the \
                 constant if the change moves what an elaboration means, then rerun `cargo run -p \
                 xtask -- record-elaborator`"
            ),
        ));
    }
    report
}

pub(crate) fn record(root: &Path) -> Report {
    let mut report = Report::new("elaborator", "recorded the elaborator digest");
    let version = match declared_version(root) {
        Ok(version) => version,
        Err(diagnostic) => {
            report.push(diagnostic);
            return report;
        }
    };
    let digest = match source_digest(root) {
        Ok(digest) => digest,
        Err(diagnostic) => {
            report.push(diagnostic);
            return report;
        }
    };
    let path = root.join(DIGEST_FILE);
    if let Err(error) = std::fs::write(&path, format!("{version} {digest}\n")) {
        report.push(Diagnostic::file(
            DIGEST_FILE,
            format!("could not write the digest: {error}"),
        ));
    }
    report
}

struct Recorded {
    version: u32,
    digest: String,
}

fn read_recorded(root: &Path) -> Result<Recorded, Diagnostic> {
    let contents = std::fs::read_to_string(root.join(DIGEST_FILE)).map_err(|error| {
        Diagnostic::file(
            DIGEST_FILE,
            format!("could not read the recorded digest: {error}"),
        )
    })?;
    let mut fields = contents.split_whitespace();
    let version = fields
        .next()
        .and_then(|field| field.parse::<u32>().ok())
        .ok_or_else(|| {
            Diagnostic::file(
                DIGEST_FILE,
                "expected `<version> <digest>`, found no version",
            )
        })?;
    let digest = fields.next().map(|field| field.to_owned()).ok_or_else(|| {
        Diagnostic::file(
            DIGEST_FILE,
            "expected `<version> <digest>`, found no digest",
        )
    })?;
    Ok(Recorded { version, digest })
}

fn declared_version(root: &Path) -> Result<u32, Diagnostic> {
    let source = std::fs::read_to_string(root.join(VERSION_FILE)).map_err(|error| {
        Diagnostic::file(
            VERSION_FILE,
            format!("could not read the semantic version constant: {error}"),
        )
    })?;
    source
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("pub const ELABORATOR_SEMANTIC_VERSION: u32 =")
        })
        .and_then(|rest| rest.trim().trim_end_matches(';').parse::<u32>().ok())
        .ok_or_else(|| {
            Diagnostic::file(
                VERSION_FILE,
                "could not find `pub const ELABORATOR_SEMANTIC_VERSION: u32 = <n>`",
            )
        })
}

fn source_digest(root: &Path) -> Result<String, Diagnostic> {
    let mut paths = Vec::new();
    for frontend_crate in FRONTEND_CRATES {
        let directory = root.join(frontend_crate);
        collect_files(&directory.join("src"), &mut paths)?;
        paths.push(directory.join("Cargo.toml"));
    }
    paths.sort();

    let mut hasher = Hasher::new();
    for path in paths {
        let contents = std::fs::read(&path).map_err(|error| {
            Diagnostic::file(
                path.strip_prefix(root)
                    .map_or(Path::new("crates/pith-loader"), Path::new),
                format!("could not read source: {error}"),
            )
        })?;
        let relative = path.strip_prefix(root).map_or_else(
            |_| path.to_string_lossy().into_owned(),
            |relative| relative.to_string_lossy().into_owned(),
        );
        hasher.update(relative.as_bytes());
        hasher.update(&(contents.len() as u64).to_le_bytes());
        hasher.update(&contents);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn collect_files(directory: &Path, paths: &mut Vec<std::path::PathBuf>) -> Result<(), Diagnostic> {
    let entries = std::fs::read_dir(directory).map_err(|error| {
        Diagnostic::file(directory, format!("could not read the directory: {error}"))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            Diagnostic::file(
                directory,
                format!("could not read a directory entry: {error}"),
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, paths)?;
        } else {
            paths.push(path);
        }
    }
    Ok(())
}
