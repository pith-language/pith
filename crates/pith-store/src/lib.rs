//! Content-addressed immutable storage.

mod blob;
mod filesystem;
mod memory;
mod store;
mod tree;

pub use blob::Blob;
pub use filesystem::FilesystemContentStore;
pub use memory::MemoryContentStore;
pub use store::{ContentStore, StoreError};
pub use tree::{Tree, TreeEntry, TreeEntryContent};
