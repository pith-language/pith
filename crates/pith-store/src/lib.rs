//! Content-addressed immutable storage.

mod blob;
mod filesystem;
mod materialize;
mod memory;
mod store;
mod tree;

pub use blob::Blob;
pub use filesystem::FilesystemContentStore;
pub use materialize::materialize_tree;
pub use memory::MemoryContentStore;
pub use store::{ContentStore, StoreError};
pub use tree::{FileContent, Tree, TreeEntry, TreeEntryContent};
