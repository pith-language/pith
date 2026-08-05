use crate::{Blob, Tree};
use pith_ids::ContentId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreError {
    message: Box<str>,
}

impl StoreError {
    pub fn new(message: impl Into<Box<str>>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for StoreError {}

pub trait ContentStore {
    /// # Errors
    /// Returns an adapter error when the bytes cannot be stored.
    fn put_blob(&mut self, bytes: &[u8]) -> Result<ContentId, StoreError>;

    /// # Errors
    /// Returns an adapter error when lookup fails. Missing content is `None`.
    fn get_blob(&self, id: ContentId) -> Result<Option<Blob>, StoreError>;

    /// # Errors
    /// Returns an adapter error when the tree cannot be stored.
    fn put_tree(&mut self, tree: Tree) -> Result<ContentId, StoreError>;

    /// # Errors
    /// Returns an adapter error when lookup fails. Missing content is `None`.
    fn get_tree(&self, id: ContentId) -> Result<Option<Tree>, StoreError>;
}
