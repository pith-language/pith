use std::path::{Path, PathBuf};

#[derive(clap::Args)]
pub struct EntryTarget {
    /// The entry declared by the root module.
    #[arg(value_name = "ENTRY")]
    pub entry: String,

    /// The root module. Defaults to module.pi, the M-14 workspace root name.
    #[arg(long, value_name = "PATH", default_value = "module.pi")]
    pub module: PathBuf,
}

impl EntryTarget {
    /// The module path and entry name the query surface resolves together.
    pub fn parts(&self) -> (&Path, &str) {
        (self.module.as_path(), self.entry.as_str())
    }
}
