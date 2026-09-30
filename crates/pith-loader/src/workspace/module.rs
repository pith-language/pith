//! The per-module results of resolution.

use pith_hir::{ModuleSubject, Project};
use pith_ids::ContentId;

use super::acquire::PROJECT_NAME;

/// One module of a resolved workspace: what it declared, where its bytes
/// came from, and the files it owns.
pub struct ResolvedModule {
    pub(super) subject: ModuleSubject,
    /// The store location the module was acquired from, rendered: for
    /// diagnostics only. Routes detect duplicates on it, but a location
    /// never becomes a semantic key: a module's identity is its declared
    /// subject and its file set.
    pub(super) origin: Box<str>,
    pub(super) project: Project,
    pub(super) files: ProjectFiles,
    /// Each input's name and the subject it resolved to, in declaration
    /// order. This is the whole import environment a file of this module
    /// may name.
    pub(super) bindings: Box<[(Box<str>, ModuleSubject)]>,
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
    pub fn project(&self) -> &Project {
        &self.project
    }

    #[must_use]
    pub fn files(&self) -> &ProjectFiles {
        &self.files
    }

    /// The module's bindings: each input's name and the subject it
    /// selected.
    pub fn bindings(&self) -> impl Iterator<Item = (&str, &ModuleSubject)> {
        self.bindings
            .iter()
            .map(|(name, subject)| (name.as_ref(), subject))
    }
}

/// A module's file set: its project file plus the includes its header
/// names, includes in canonical order.
pub struct ProjectFiles {
    project: ModuleFile,
    includes: SourceSet,
}

impl ProjectFiles {
    /// The file set a module holds mid-resolution, before acquisition
    /// reads its includes.
    pub(super) fn unacquired() -> Self {
        Self {
            project: ModuleFile::new(PROJECT_NAME, ""),
            includes: SourceSet::empty(),
        }
    }

    /// Assemble a file set from the project file's text and the acquired
    /// includes, which are sorted into canonical order whatever order the
    /// header listed them in, so the merged span space of a module is a
    /// function of its file set, never of its clause order. A repeated
    /// include path is refused.
    ///
    /// # Errors
    /// Returns [`crate::graph::FrontendInputError`] for a repeated path,
    /// the same refusal the graph tier's source input makes.
    pub fn new(
        project_text: impl Into<Box<str>>,
        includes: impl IntoIterator<Item = ModuleFile>,
    ) -> Result<Self, crate::graph::FrontendInputError> {
        Ok(Self {
            project: ModuleFile::new(PROJECT_NAME, project_text),
            includes: SourceSet::new(includes)?,
        })
    }

    #[must_use]
    pub fn project(&self) -> &ModuleFile {
        &self.project
    }

    #[must_use]
    pub fn includes(&self) -> &SourceSet {
        &self.includes
    }
}

/// A module's includes, in canonical module-relative order.
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
    /// Returns [`crate::graph::FrontendInputError::DuplicateSourcePath`] for a
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

/// One owned file: its module-relative path, its text, and the content
/// identity of that text.
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
