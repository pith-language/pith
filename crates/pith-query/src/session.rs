use std::marker::PhantomData;
use std::path::Path;
use std::sync::OnceLock;

use pith_engine::{Engine, EngineStateReader};
use pith_ids::ContentId;
use pith_output::dto::{StoredContent, StoredContentKind, TreeEntryRepr, TreeListing};
use pith_state_sqlite::{ReadOnlySqliteEngineStateStore, SqliteEngineStateStore, SqliteStateError};
use pith_store::{Blob, ContentStore, FilesystemContentStore, StoreError, Tree, TreeEntryContent};

use crate::content;
use crate::error::QueryError;
use crate::roots::Roots;

mod sealed {
    pub trait Sealed {}
}

pub trait Access: sealed::Sealed {}

pub struct ReadOnly;
pub struct Writable;

impl sealed::Sealed for ReadOnly {}
impl sealed::Sealed for Writable {}
impl Access for ReadOnly {}
impl Access for Writable {}

pub struct Session<A: Access> {
    roots: Roots,
    store: OnceLock<FilesystemContentStore>,
    state: OnceLock<ReadOnlySqliteEngineStateStore>,
    access: PhantomData<fn() -> A>,
}

impl Session<ReadOnly> {
    /// Open the roots for reading.
    ///
    /// # Errors
    /// [`QueryError`] when the content store cannot be opened.
    pub fn open(roots: Roots) -> Result<Self, QueryError> {
        Ok(Self::of_roots(roots))
    }

    /// Consume this session into an engine that can register and query rules,
    /// but cannot evaluate or publish durable attempts.
    ///
    /// The returned type carries only [`EngineStateReader`] authority. Methods
    /// such as `evaluate_pure` and `run` are absent at compile time.
    ///
    /// # Errors
    /// [`QueryError`] when the state database cannot be opened read-only.
    pub fn into_query_engine(self) -> Result<Engine<dyn EngineStateReader>, QueryError> {
        let Self { roots, store, .. } = self;
        let state =
            ReadOnlySqliteEngineStateStore::open_read_only(roots.state()).map_err(state_failure)?;
        let store = match store.into_inner() {
            Some(store) => store,
            None => FilesystemContentStore::open_read_only(roots.store()).map_err(store_failure)?,
        };
        Ok(Engine::with_read_only_state_store(store, state))
    }
}

impl Session<Writable> {
    /// Open the roots for reading and for admitting content.
    ///
    /// # Errors
    /// [`QueryError`] when the content store cannot be opened.
    pub fn open_writable(roots: Roots) -> Result<Self, QueryError> {
        let store = FilesystemContentStore::open(roots.store()).map_err(store_failure)?;
        let session = Self::of_roots(roots);
        let _ = session.store.set(store);
        Ok(session)
    }

    /// Consume this session into the writable engine used for evaluation.
    ///
    /// # Errors
    /// [`QueryError`] when the state database cannot be opened, initialized,
    /// or recovered.
    pub fn into_engine(self) -> Result<Engine, QueryError> {
        let Self { roots, store, .. } = self;
        let state = SqliteEngineStateStore::open(roots.state()).map_err(state_failure)?;
        let store = store
            .into_inner()
            .ok_or_else(|| QueryError::internal("a writable session has no content store"))?;
        Ok(Engine::with_state_store(store, state))
    }

    /// Admit a file or a directory, and name what went in. A directory becomes
    /// a tree; a file becomes a blob.
    ///
    /// # Errors
    /// [`QueryError`] when the path cannot be read or the store refuses it.
    pub fn add(&mut self, path: &Path) -> Result<StoredContent, QueryError> {
        let store = self
            .store
            .get_mut()
            .ok_or_else(|| QueryError::internal("a writable session has no content store"))?;
        content::add(store, path)
    }
}

impl<A: Access> Session<A> {
    fn of_roots(roots: Roots) -> Self {
        Self {
            roots,
            store: OnceLock::new(),
            state: OnceLock::new(),
            access: PhantomData,
        }
    }

    #[must_use]
    pub const fn roots(&self) -> &Roots {
        &self.roots
    }

    pub(crate) fn content_store(&self) -> Result<&FilesystemContentStore, QueryError> {
        if let Some(store) = self.store.get() {
            return Ok(store);
        }
        let store =
            FilesystemContentStore::open_read_only(self.roots.store()).map_err(store_failure)?;
        Ok(self.store.get_or_init(|| store))
    }

    /// The engine-state database, opened read-only on first use.
    ///
    /// Lazy for the same reason the evaluation engine's construction is: the
    /// `store` group reads no engine state, and must not fail because a state
    /// database is absent or unreadable. `graph` and `explain` are the callers
    /// that need it.
    ///
    /// # Errors
    /// [`QueryError`] when the state database cannot be opened: it is absent,
    /// holds no engine state, or records versions this build refuses to read.
    pub fn state(&self) -> Result<&ReadOnlySqliteEngineStateStore, QueryError> {
        if let Some(state) = self.state.get() {
            return Ok(state);
        }
        let state = ReadOnlySqliteEngineStateStore::open_read_only(self.roots.state())
            .map_err(state_failure)?;
        Ok(self.state.get_or_init(|| state))
    }

    /// The bytes of one blob.
    ///
    /// # Errors
    /// [`QueryError`] when the store errors, or a user failure when it holds
    /// no blob under `id`.
    pub fn blob(&self, id: ContentId) -> Result<Blob, QueryError> {
        self.content_store()?
            .get_blob(id)
            .map_err(store_failure)?
            .ok_or_else(|| missing("blob", id))
    }

    /// One tree's entries, in the canonical name order its manifest fixes.
    ///
    /// # Errors
    /// [`QueryError`] when the store errors or holds no tree under `id`.
    pub fn list_tree(&self, id: ContentId) -> Result<TreeListing, QueryError> {
        let tree = self.tree(id)?;
        Ok(TreeListing {
            tree: id.digest().to_string().into(),
            entries: tree.entries().iter().map(entry_projection).collect(),
        })
    }

    /// Render a tree into a new directory.
    ///
    /// # Errors
    /// [`QueryError`] when the store errors, holds no tree under `id`, or the
    /// directory cannot be written.
    pub fn materialize(&self, id: ContentId, into: &Path) -> Result<StoredContent, QueryError> {
        let _ = self.tree(id)?;
        pith_store::materialize_tree(self.content_store()?, id, into).map_err(store_failure)?;
        Ok(StoredContent {
            id: id.digest().to_string().into(),
            kind: StoredContentKind::Tree,
            path: Some(into.display().to_string().into()),
        })
    }

    fn tree(&self, id: ContentId) -> Result<Tree, QueryError> {
        self.content_store()?
            .get_tree(id)
            .map_err(store_failure)?
            .ok_or_else(|| missing("tree", id))
    }
}

fn entry_projection(
    entry: &pith_store::TreeEntry<pith_store::FileContent, ContentId>,
) -> TreeEntryRepr {
    let name = entry.name().into();
    match entry.content() {
        TreeEntryContent::File(file) => TreeEntryRepr::File {
            name,
            content: file.content.digest().to_string().into(),
            executable: file.executable,
        },
        TreeEntryContent::Tree(id) => TreeEntryRepr::Tree {
            name,
            content: id.digest().to_string().into(),
        },
        TreeEntryContent::Symlink { target } => TreeEntryRepr::Symlink {
            name,
            target: String::from_utf8_lossy(target).into_owned().into(),
        },
    }
}

fn missing(what: &str, id: ContentId) -> QueryError {
    QueryError::user(format!("the store holds no {what} under {}", id.digest()))
}

pub(crate) fn store_failure(error: StoreError) -> QueryError {
    QueryError::internal(error.to_string())
}

/// A read the adapter itself refused: corruption, or a lookup the store could
/// not serve. Nothing a caller did wrong, so it lands on the internal side.
pub(crate) fn read_failure(error: pith_engine::state::EngineStateError) -> QueryError {
    QueryError::internal(error.to_string())
}

pub(crate) fn state_failure(error: SqliteStateError) -> QueryError {
    match &error {
        SqliteStateError::Connection(_)
        | SqliteStateError::NothingToRead { .. }
        | SqliteStateError::IncompatibleReadOnly { .. }
        | SqliteStateError::UnusablePath { .. } => QueryError::user(error.to_string()),
        SqliteStateError::Database(_)
        | SqliteStateError::Filesystem { .. }
        | SqliteStateError::NoFreeQuarantinePath { .. }
        | SqliteStateError::UnrepresentableVersion { .. }
        | SqliteStateError::Recovery { .. }
        | SqliteStateError::Poisoned => QueryError::internal(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{ReadOnly, Session};
    use crate::roots::Roots;
    use pith_engine::state::EngineStateReader;
    use pith_state_sqlite::SqliteEngineStateStore;

    #[test]
    fn a_session_opens_without_a_state_database() -> Result<(), Box<dyn std::error::Error>> {
        let home = tempfile::tempdir()?;
        let session = Session::<ReadOnly>::open(Roots::under(home.path()))?;

        assert!(
            session.state().is_err(),
            "a state database appeared from nowhere"
        );
        assert!(
            !session.roots().store().exists(),
            "a read-only state session created the content store"
        );
        Ok(())
    }

    #[test]
    fn the_state_database_opens_read_only_under_the_session()
    -> Result<(), Box<dyn std::error::Error>> {
        let home = tempfile::tempdir()?;
        let roots = Roots::under(home.path());
        let writable = SqliteEngineStateStore::open(roots.state())?;
        drop(writable);

        let session = Session::<ReadOnly>::open(roots)?;
        let pending = session.state()?.pending_attempts()?;
        assert!(pending.is_empty(), "a fresh database holds attempts");
        Ok(())
    }

    #[test]
    fn a_read_only_session_becomes_only_a_query_engine() -> Result<(), Box<dyn std::error::Error>> {
        let home = tempfile::tempdir()?;
        let roots = Roots::under(home.path());
        drop(SqliteEngineStateStore::open(roots.state())?);

        let engine = Session::<ReadOnly>::open(roots)?.into_query_engine()?;
        assert_eq!(engine.query().rules().count(), 0);
        assert_eq!(engine.state_store().pending_attempts()?.len(), 0);
        Ok(())
    }

    #[test]
    fn a_writable_session_becomes_the_evaluation_engine() -> Result<(), Box<dyn std::error::Error>>
    {
        let home = tempfile::tempdir()?;
        let engine =
            Session::<super::Writable>::open_writable(Roots::under(home.path()))?.into_engine()?;

        assert_eq!(engine.state_store().pending_attempts()?.len(), 0);
        Ok(())
    }
}
