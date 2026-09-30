use std::path::{Path, PathBuf};

#[derive(clap::Args)]
pub struct EntryTarget {
    /// The entry declared by the root module.
    #[arg(value_name = "ENTRY")]
    pub entry: String,

    /// The root project file. Defaults to pith.pi, the conventional
    /// project name.
    #[arg(long, value_name = "PATH", default_value = "pith.pi")]
    pub module: PathBuf,
}

impl EntryTarget {
    /// The module path and entry name the query surface resolves together.
    pub fn parts(&self) -> (&Path, &str) {
        (self.module.as_path(), self.entry.as_str())
    }
}
