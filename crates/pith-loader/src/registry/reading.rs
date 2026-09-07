//! The read in progress: every file, every check, one diagnostic per
//! refusal earned.
//!
//! The walker holds no judgment of its own — each method is one of the
//! four checks or the plumbing around one, and [`super::trust::read`]
//! decides what the diagnostics add up to.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use pith_diag::{Diag, Severity, SourceFile, SourceId, Span};
use pith_hir::{FrontendCode, ModuleSubject, RootKey, SubjectSegment};

use super::keys::{Detached, Public};
use super::line::{KeySet, Line, Release, Withdrawal};
use super::trust::{DomainKeys, Record};
use super::{DOMAINS_DIRECTORY, INDEX_DIRECTORY, ROOT_FILE};

/// A line's assertion: who signed it, the canonical bytes covered, and
/// the signature over them.
type Assertion<'a> = (&'a Public, &'a str, &'a Detached);

/// One read in progress: the directory, the previously admitted state, the
/// diagnostics earned so far, and the next source identity to hand out.
pub(crate) struct Reading<'a> {
    directory: &'a Path,
    prior: &'a Record,
    diagnostics: Vec<Diag>,
    next_id: u32,
}

/// A subject's lines, split by kind, with the canonical renders the record
/// keeps.
pub(crate) struct Admitted {
    pub(crate) releases: BTreeMap<Box<str>, Release>,
    pub(crate) withdrawals: Vec<Withdrawal>,
    pub(crate) rendered: Vec<Line>,
}

impl<'a> Reading<'a> {
    pub(crate) fn new(directory: &'a Path, prior: &'a Record) -> Self {
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

    /// The second and third checks live here: every domain key set is
    /// signed by the root, and no generation moved backwards.
    pub(crate) fn domains(&mut self, root: &Public) -> BTreeMap<Box<str>, DomainKeys> {
        let mut domains = BTreeMap::new();
        let Some(children) = self.sorted_children(&self.directory.join(DOMAINS_DIRECTORY), true)
        else {
            return domains;
        };
        for domain in children {
            let path = self.directory.join(DOMAINS_DIRECTORY).join(domain.as_ref());
            let text = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) => {
                    self.diagnostics.push(sourceless(
                        FrontendCode::UnreadableSource,
                        format!("cannot read the key set `{}`: {error}", path.display()),
                    ));
                    continue;
                }
            };
            let source = self.source(&path, text);
            let key_set = match KeySet::parse(source.source_text(), pith_diag::ByteOffset(0)) {
                Ok(key_set) => key_set,
                Err(malformed) => {
                    self.diagnostics.push(at_source(
                        FrontendCode::UnreadableSource,
                        malformed.span,
                        &source,
                        malformed.message,
                    ));
                    continue;
                }
            };
            if !root.verify(key_set.render_unsigned().as_bytes(), key_set.signature()) {
                self.diagnostics.push(at_source(
                    FrontendCode::UnsignedKeySet,
                    Span::new(pith_diag::ByteOffset(0), pith_diag::ByteOffset(0)),
                    &source,
                    format!(
                        "the `{domain}` key set is not signed by the pinned root key `{}`",
                        root.spelling()
                    ),
                ));
                continue;
            }
            let threshold = key_set.threshold();
            let key_count = key_set.keys().len();
            if threshold == 0 || usize::try_from(threshold).unwrap_or(usize::MAX) > key_count {
                self.diagnostics.push(at_source(
                    FrontendCode::UnreadableSource,
                    Span::new(pith_diag::ByteOffset(0), pith_diag::ByteOffset(0)),
                    &source,
                    format!(
                        "the `{domain}` key set carries a threshold of {threshold} over \
                         {key_count} keys, which no release can meet"
                    ),
                ));
                continue;
            }
            if let Some(&admitted) = self.prior.domains.get(domain.as_ref())
                && key_set.generation() < admitted
            {
                self.diagnostics.push(at_source(
                    FrontendCode::KeySetRollback,
                    Span::new(pith_diag::ByteOffset(0), pith_diag::ByteOffset(0)),
                    &source,
                    format!(
                        "the `{domain}` key set is generation {}, and the consumer \
                         admitted generation {admitted}: a generation moved backwards",
                        key_set.generation()
                    ),
                ));
                continue;
            }
            domains.insert(
                domain,
                DomainKeys {
                    generation: key_set.generation(),
                    keys: key_set.keys().clone(),
                },
            );
        }
        domains
    }

    /// The remaining checks: every line is signed by a key its domain
    /// enrolled, no version appears twice, and every line the consumer
    /// previously admitted is still there, unchanged, before any new one.
    pub(crate) fn subjects(
        &mut self,
        domains: &BTreeMap<Box<str>, DomainKeys>,
    ) -> BTreeMap<ModuleSubject, Admitted> {
        let mut subjects = BTreeMap::new();
        let Some(domains_on_disk) =
            self.sorted_children(&self.directory.join(INDEX_DIRECTORY), false)
        else {
            return subjects;
        };
        for domain in domains_on_disk {
            let Some(domain_segment) = segment(&domain) else {
                self.diagnostics.push(sourceless(
                    FrontendCode::UnreadableSource,
                    format!(
                        "`{domain}` names a directory under `{INDEX_DIRECTORY}` that is not a \
                         subject domain"
                    ),
                ));
                continue;
            };
            let Some(names) = self.sorted_children(
                &self.directory.join(INDEX_DIRECTORY).join(domain.as_ref()),
                true,
            ) else {
                continue;
            };
            for name in names {
                let Some(name_segment) = segment(&name) else {
                    self.diagnostics.push(sourceless(
                        FrontendCode::UnreadableSource,
                        format!("`{name}` is not a subject name"),
                    ));
                    continue;
                };
                let subject = ModuleSubject::new(domain_segment.clone(), name_segment.clone());
                let path = self
                    .directory
                    .join(INDEX_DIRECTORY)
                    .join(domain.as_ref())
                    .join(name.as_ref());
                let text = match std::fs::read_to_string(&path) {
                    Ok(text) => text,
                    Err(error) => {
                        self.diagnostics.push(sourceless(
                            FrontendCode::UnreadableSource,
                            format!(
                                "cannot read the index of {subject} at `{}`: {error}",
                                path.display()
                            ),
                        ));
                        continue;
                    }
                };
                let source = self.source(&path, text);
                let admitted = self.admitted_lines(&subject, domains, &source);
                subjects.insert(subject, admitted);
            }
        }
        subjects
    }

    fn admitted_lines(
        &mut self,
        subject: &ModuleSubject,
        domains: &BTreeMap<Box<str>, DomainKeys>,
        source: &Arc<SourceFile>,
    ) -> Admitted {
        let mut admitted = Admitted {
            releases: BTreeMap::new(),
            withdrawals: Vec::new(),
            rendered: Vec::new(),
        };
        for line in source.lines() {
            if line.text.trim().is_empty() {
                continue;
            }
            let parsed = match Line::parse(line.text, line.span.start) {
                Ok(parsed) => parsed,
                Err(malformed) => {
                    self.diagnostics.push(at_source(
                        FrontendCode::UnreadableSource,
                        malformed.span,
                        source,
                        malformed.message,
                    ));
                    continue;
                }
            };
            // A line the consumer already admitted answers to the
            // append-only check, not to the current key set: it was
            // admitted under a set the consumer recorded, and rotation
            // retires a key for what it may sign next, not for what it
            // signed while enrolled. A line new to this read answers to
            // the current set.
            let fresh = !self.prior.subjects.get(subject).is_some_and(|admitted| {
                admitted
                    .iter()
                    .any(|prior| prior.as_ref() == parsed.render().as_str())
            });
            match &parsed {
                Line::Release(release) => {
                    if fresh {
                        let unsigned = release.render_unsigned();
                        self.check_line(
                            subject,
                            domains,
                            source,
                            line.span,
                            (release.signer(), unsigned.as_str(), release.signature()),
                        );
                    }
                    let spelling = release.version().canonical_spelling();
                    if admitted.releases.contains_key(&spelling) {
                        self.diagnostics.push(at_source(
                            FrontendCode::DuplicateVersion,
                            line.span,
                            source,
                            format!(
                                "the index carries two entries for {subject} {spelling}: \
                                 versions that compare equal are one version"
                            ),
                        ));
                        continue;
                    }
                    admitted.releases.insert(spelling, release.clone());
                }
                Line::Withdrawal(withdrawal) => {
                    if fresh {
                        let unsigned = withdrawal.render_unsigned();
                        self.check_line(
                            subject,
                            domains,
                            source,
                            line.span,
                            (
                                withdrawal.signer(),
                                unsigned.as_str(),
                                withdrawal.signature(),
                            ),
                        );
                    }
                    admitted.withdrawals.push(withdrawal.clone());
                }
            }
            admitted.rendered.push(parsed);
        }
        self.append_only(subject, source, &admitted.rendered);
        admitted
    }

    /// A line's signature check: the named signer is enrolled in the
    /// domain's key set, and the signature covers the line's canonical
    /// bytes under it.
    fn check_line(
        &mut self,
        subject: &ModuleSubject,
        domains: &BTreeMap<Box<str>, DomainKeys>,
        source: &Arc<SourceFile>,
        span: Span,
        assertion: Assertion<'_>,
    ) {
        let (signer, unsigned, signature) = assertion;
        let Some(keys) = domains.get(subject.domain().as_str()) else {
            self.diagnostics.push(at_source(
                FrontendCode::UnsignedKeySet,
                span,
                source,
                format!(
                    "the index carries a line for {} under no `{}` key set",
                    subject,
                    subject.domain().as_str()
                ),
            ));
            return;
        };
        if !keys.keys.contains(signer) {
            self.diagnostics.push(at_source(
                FrontendCode::UnenrolledKey,
                span,
                source,
                format!(
                    "the line for {subject} names its signer `{}`, and the `{}` key set \
                     carries: {} — a key absent from the set",
                    signer.spelling(),
                    subject.domain().as_str(),
                    key_list(&keys.keys)
                ),
            ));
            return;
        }
        if !signer.verify(unsigned.as_bytes(), signature) {
            self.diagnostics.push(at_source(
                FrontendCode::UnenrolledKey,
                span,
                source,
                format!(
                    "the signature over the line for {subject} does not verify under its \
                     named signer `{}`",
                    signer.spelling()
                ),
            ));
        }
    }

    /// The fourth check: the lines the consumer previously admitted for
    /// this subject are a prefix of the lines it just read, because new
    /// entries append and nothing else.
    fn append_only(&mut self, subject: &ModuleSubject, source: &Arc<SourceFile>, current: &[Line]) {
        let Some(prior) = self.prior.subjects.get(subject) else {
            return;
        };
        for (index, admitted) in prior.iter().enumerate() {
            let still_there = current
                .get(index)
                .is_some_and(|line| line.render() == admitted.as_ref());
            if !still_there {
                self.diagnostics.push(at_source(
                    FrontendCode::IndexFork,
                    Span::new(pith_diag::ByteOffset(0), pith_diag::ByteOffset(0)),
                    source,
                    format!(
                        "the index for {subject} previously admitted `{admitted}` as its \
                         line {}, and that line is now altered or absent",
                        index.saturating_add(1)
                    ),
                ));
                return;
            }
        }
    }

    pub(crate) fn diagnostics(&self) -> &[Diag] {
        &self.diagnostics
    }

    pub(crate) fn into_diagnostics(self) -> Box<[Diag]> {
        self.diagnostics.into()
    }

    fn source(&mut self, path: &Path, text: String) -> Arc<SourceFile> {
        let id = SourceId::from_raw(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        Arc::new(SourceFile::new(id, path.display().to_string(), text))
    }

    /// A directory's child names, sorted: the read order is the byte order,
    /// so two clients walk one registry the same way. `files` selects
    /// regular files against directories, because the index walk descends
    /// into domains and reads subjects.
    fn sorted_children(&mut self, directory: &Path, files: bool) -> Option<Vec<Box<str>>> {
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

fn segment(name: &str) -> Option<SubjectSegment> {
    SubjectSegment::parse(name).ok()
}

fn key_list(keys: &BTreeSet<Public>) -> String {
    keys.iter()
        .map(|key| key.spelling().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn sourceless(code: FrontendCode, message: impl Into<Box<str>>) -> Diag {
    Diag::new(Severity::Error, code.stable(), Span::none(), message)
}

fn at_source(
    code: FrontendCode,
    span: Span,
    source: &Arc<SourceFile>,
    message: impl Into<Box<str>>,
) -> Diag {
    Diag::new(Severity::Error, code.stable(), span, message).with_source(Arc::clone(source))
}
