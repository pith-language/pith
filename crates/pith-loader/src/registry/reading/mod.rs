//! The read in progress: one diagnostic per refusal earned. The first
//! check lives here; the key-set checks in [`domains`], the subject-line
//! checks in [`subjects`]; [`super::trust::read`] decides what the
//! diagnostics add up to.

mod domains;
mod subjects;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use pith_diag::{Diag, SourceFile, SourceId};
use pith_hir::{RootKey, SubjectSegment};

use super::keys::Public;
use crate::workspace::graph::sourceless;
use pith_hir::FrontendCode;

use super::ROOT_FILE;

/// One read in progress: the directory, the previously admitted state, the
/// diagnostics earned so far, and the next source identity to hand out.
pub(crate) struct Reading<'a> {
    pub(super) directory: &'a Path,
    pub(super) prior: &'a super::trust::Record,
    pub(super) diagnostics: Vec<Diag>,
    pub(super) next_id: u32,
}

/// A subject's lines, split by kind, with the canonical renders the record
/// keeps.
pub(crate) struct Admitted {
    pub(crate) releases: BTreeMap<Box<str>, super::line::Release>,
    pub(crate) withdrawals: Vec<super::line::Withdrawal>,
    pub(crate) rendered: Vec<super::line::Line>,
}

impl<'a> Reading<'a> {
    pub(crate) fn new(directory: &'a Path, prior: &'a super::trust::Record) -> Self {
        Self {
            directory,
            prior,
            diagnostics: Vec::new(),
            next_id: 0,
        }
    }

    /// The first check: the index names the root key the consumer pinned.
    pub(crate) fn root(&mut self, pinned: &RootKey) -> Option<Public> {
        if std::fs::read_dir(self.directory).is_err() {
            self.diagnostics.push(sourceless(
                FrontendCode::IndexUnreachable,
                format!(
                    "the registry at `{}` cannot be read at all",
                    self.directory.display()
                ),
            ));
            return None;
        }
        let path = self.directory.join(ROOT_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                self.diagnostics.push(sourceless(
                    FrontendCode::UnsignedKeySet,
                    format!(
                        "the registry at `{}` names no root key: {error}",
                        self.directory.display()
                    ),
                ));
                return None;
            }
        };
        let served = text.trim();
        let served = match Public::parse(served) {
            Ok(served) => served,
            Err(error) => {
                self.diagnostics.push(sourceless(
                    FrontendCode::InvalidRootKey,
                    format!("the root file at `{}`: {}", self.directory.display(), error),
                ));
                return None;
            }
        };
        match Public::of_pinned(pinned) {
            Ok(pinned) if pinned == served => Some(served),
            Ok(pinned) => {
                self.diagnostics.push(sourceless(
                    FrontendCode::UnsignedKeySet,
                    format!(
                        "the registry at `{}` serves the root key `{}`, and the configuration \
                         pins `{}`",
                        self.directory.display(),
                        served.spelling(),
                        pinned.spelling()
                    ),
                ));
                None
            }
            Err(error) => {
                self.diagnostics.push(sourceless(
                    FrontendCode::UnsignedKeySet,
                    format!("the pinned root key `{pinned}`: {error}"),
                ));
                None
            }
        }
    }

    pub(crate) fn diagnostics(&self) -> &[Diag] {
        &self.diagnostics
    }

    pub(crate) fn into_diagnostics(self) -> Box<[Diag]> {
        self.diagnostics.into()
    }

    pub(super) fn source(&mut self, path: &Path, text: String) -> Arc<SourceFile> {
        let id = SourceId::from_raw(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        Arc::new(SourceFile::new(id, path.display().to_string(), text))
    }

    /// A directory's child names, sorted: the read order is the byte order,
    /// so two clients walk one registry the same way. `files` selects
    /// regular files against directories, because the index walk descends
    /// into domains and reads subjects.
    pub(super) fn sorted_children(
        &mut self,
        directory: &Path,
        files: bool,
    ) -> Option<Vec<Box<str>>> {
        let entries = std::fs::read_dir(directory).ok()?;
        let mut names = entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                entry
                    .file_name()
                    .into_string()
                    .ok()
                    .filter(|_| entry.file_type().is_ok_and(|kind| kind.is_file() == files))
            })
            .collect::<Vec<_>>();
        names.sort();
        Some(names.into_iter().map(Box::from).collect())
    }
}

pub(super) fn segment(name: &str) -> Option<SubjectSegment> {
    SubjectSegment::parse(name).ok()
}

pub(super) fn key_list(keys: &BTreeSet<Public>) -> String {
    keys.iter()
        .map(|key| key.spelling().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
