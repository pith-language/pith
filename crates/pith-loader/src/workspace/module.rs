//! The per-module results of resolution.

use pith_hir::{Manifest, ModuleSubject};
use pith_ids::ContentId;

/// One module of a resolved workspace: what it declared, where its bytes
/// came from, and the source files it owns.
pub struct ResolvedModule {
    pub(super) subject: ModuleSubject,
    /// The store location the module was acquired from, rendered: for
    /// diagnostics only. Routes detect duplicates on it, but a location
    /// never becomes a semantic key: a module's identity is its declared
    /// subject and its module-relative source set.
    pub(super) origin: Box<str>,
    pub(super) manifest: Manifest,
    pub(super) sources: SourceSet,
}

impl ResolvedModule {
    #[must_use]
    pub fn subject(&self) -> &ModuleSubject {
        &self.subject
    }

    /// Where the module's bytes came from, as its store renders the
    /// location. Diagnostics read it; no digest does.
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }

    #[must_use]
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    #[must_use]
    pub fn sources(&self) -> &SourceSet {
        &self.sources
    }

    /// The module's bindings: each `use` clause's alias and the subject it
    /// selected. This is the whole import environment a source file of this
    /// module may name.
    pub fn bindings(&self) -> impl Iterator<Item = (&str, &ModuleSubject)> {
        self.manifest
            .uses()
            .iter()
            .map(|use_| (use_.alias.as_ref(), &use_.subject))
    }
}

/// A module's source set: the regular `.pi` files under its `src/`, in
/// canonical module-relative order.
pub struct SourceSet {
    pub(super) files: Box<[ModuleFile]>,
}

impl SourceSet {
    pub(crate) fn empty() -> Self {
        Self {
            files: Box::new([]),
        }
    }

    /// Sorted into canonical module-relative order whatever order it was
    /// given in: the merged span space of a module is a function of its file
    /// set, never of a directory enumeration. A repeated path is refused.
    ///
    /// # Errors
    /// Returns [`crate::FrontendInputError::DuplicateSourcePath`] for a
    /// repeated path, the same refusal the graph tier's source input makes.
    pub fn new(
        files: impl IntoIterator<Item = ModuleFile>,
    ) -> Result<Self, crate::graph::FrontendInputError> {
        let mut files = files.into_iter().collect::<Vec<_>>();
        files.sort_by(|left, right| left.path().cmp(right.path()));
        for [earlier, later] in files.array_windows() {
            if earlier.path() == later.path() {
                return Err(crate::graph::FrontendInputError::DuplicateSourcePath {
                    path: earlier.path().into(),
                });
            }
        }
        Ok(Self {
            files: files.into(),
        })
    }

    #[must_use]
    pub fn files(&self) -> &[ModuleFile] {
        &self.files
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// One owned source file: its module-relative path, its text, and the
/// content identity of that text.
pub struct ModuleFile {
    path: Box<str>,
    text: Box<str>,
    content_id: ContentId,
}

impl ModuleFile {
    #[must_use]
    pub fn new(path: impl Into<Box<str>>, text: impl Into<Box<str>>) -> Self {
        let text = text.into();
        let content_id = ContentId::of_blob(text.as_bytes());
        Self {
            path: path.into(),
            content_id,
            text,
        }
    }

    /// The module-relative path, in forward slashes.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub const fn content_id(&self) -> ContentId {
        self.content_id
    }
}
