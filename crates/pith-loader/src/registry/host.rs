//! The directory host: the registry side of a directory tree. It writes
//! what publication derives and keeps the trees its revisions name.
//! Nothing here verifies; that is [`super::trust`]'s half, so a host
//! cannot vouch for its own index.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use pith_hir::{ManifestVersion, ModuleSubject};

use super::keys::{Public, Signing};
use super::line::{KeySet, Withdrawal};
use super::publish::Draft;
use super::{DOMAINS_DIRECTORY, INDEX_DIRECTORY, ROOT_FILE, SOURCES_DIRECTORY};
use crate::{MANIFEST_NAME, SOURCE_DIRECTORY};

/// A registry hosted by one directory.
#[derive(Debug)]
pub struct Host {
    directory: PathBuf,
}

impl Host {
    #[must_use]
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Writes the root file naming `root`'s public half; the first call on
    /// an empty directory creates the layout.
    ///
    /// # Errors
    /// The filesystem failure.
    pub fn initialize(&self, root: &Signing) -> io::Result<()> {
        std::fs::create_dir_all(self.directory.join(DOMAINS_DIRECTORY))?;
        std::fs::create_dir_all(self.directory.join(INDEX_DIRECTORY))?;
        std::fs::create_dir_all(self.directory.join(SOURCES_DIRECTORY))?;
        std::fs::write(
            self.directory.join(ROOT_FILE),
            root.public().spelling().as_bytes(),
        )
    }

    /// Writes a domain's key set at `generation`, signed by `root`. A
    /// higher generation rotates the set; the caller owns the number
    /// because only it knows what changed.
    ///
    /// # Errors
    /// The filesystem failure.
    pub fn enroll(
        &self,
        root: &Signing,
        domain: &str,
        keys: &BTreeSet<Public>,
        threshold: u32,
        generation: u64,
    ) -> io::Result<()> {
        let signed = KeySet::signed(generation, threshold, keys.clone(), root).render();
        std::fs::create_dir_all(self.directory.join(DOMAINS_DIRECTORY))?;
        std::fs::write(
            self.directory.join(DOMAINS_DIRECTORY).join(domain),
            signed.as_bytes(),
        )
    }

    /// Stores a draft's tree under its measured revision (addressed by
    /// the revision's own identity) and appends the signed release line
    /// for the host to admit at `admitted`.
    ///
    /// # Errors
    /// The filesystem failure.
    pub fn publish(&self, draft: &Draft, publisher: &Signing, admitted: u64) -> io::Result<()> {
        self.store_tree(draft)?;
        let release = draft.release(publisher, admitted);
        self.append(draft.subject(), &release.render())
    }

    /// Appends a signed withdrawal line standing against `version`.
    ///
    /// # Errors
    /// The filesystem failure.
    pub fn withdraw(
        &self,
        subject: &ModuleSubject,
        version: ManifestVersion,
        reason: &str,
        issuer: &Signing,
    ) -> io::Result<()> {
        let withdrawal = Withdrawal::signed(version, reason.into(), issuer);
        self.append(subject, &withdrawal.render())
    }

    fn store_tree(&self, draft: &Draft) -> io::Result<()> {
        let revision = self
            .directory
            .join(SOURCES_DIRECTORY)
            .join(draft.measured().revision().as_ref());
        if revision.exists() {
            return Ok(());
        }
        std::fs::create_dir_all(revision.join(SOURCE_DIRECTORY))?;
        std::fs::write(revision.join(MANIFEST_NAME), draft.manifest().as_bytes())?;
        for (path, text) in draft.sources() {
            std::fs::create_dir_all(revision.join(parent_of(path)))?;
            std::fs::write(revision.join(path.as_ref()), text.as_bytes())?;
        }
        Ok(())
    }

    fn append(&self, subject: &ModuleSubject, line: &str) -> io::Result<()> {
        let file = self
            .directory
            .join(INDEX_DIRECTORY)
            .join(subject.domain().as_str())
            .join(subject.name().as_str());
        std::fs::create_dir_all(file.parent().unwrap_or(&self.directory))?;
        let mut existing = std::fs::read_to_string(&file).unwrap_or_default();
        if !existing.is_empty() && !existing.ends_with('\n') {
            existing.push('\n');
        }
        existing.push_str(line);
        existing.push('\n');
        std::fs::write(&file, existing.as_bytes())
    }
}

/// The directory part of a module-relative path in forward slashes.
fn parent_of(path: &str) -> PathBuf {
    match path.rsplit_once('/') {
        Some((parent, _)) => PathBuf::from(parent),
        None => PathBuf::new(),
    }
}
