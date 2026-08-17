use std::path::{Path, PathBuf};

use phloem::witness::{Checkpoint, MerkleTree};

pub fn write_file(path: PathBuf, bytes: &[u8]) -> pith_diag::PithResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            super::diagnostic_support::fixture_error(format!(
                "creating {} failed: {error}",
                parent.display()
            ))
        })?;
    }
    std::fs::write(&path, bytes).map_err(|error| {
        super::diagnostic_support::fixture_error(format!(
            "writing {} failed: {error}",
            path.display()
        ))
    })
}

pub fn append_index_line(index: &mut Vec<(String, String)>, name: &str, line: &str) {
    match index
        .iter_mut()
        .find(|(existing_name, _)| existing_name == name)
    {
        Some((_, lines)) => lines.push_str(&format!("{line}\n")),
        None => index.push((name.into(), format!("{line}\n"))),
    }
}

pub fn write_transparency_log(
    root: &Path,
    origin: &str,
    leaves: &[String],
) -> pith_diag::PithResult<Checkpoint> {
    let log = root.join("log");
    write_file(
        log.join("leaves"),
        format!("{}\n", leaves.join("\n")).as_bytes(),
    )?;
    let tree = MerkleTree::new(leaves.iter().map(String::as_str).map(str::as_bytes))?;
    let checkpoint = Checkpoint {
        origin: origin.into(),
        size: tree.size(),
        root: tree.root(),
    };
    write_file(log.join("checkpoint"), checkpoint.render().as_bytes())?;
    Ok(checkpoint)
}
