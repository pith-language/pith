//! The subject-line checks: every line is signed by a key its domain
//! enrolled, no version appears twice, and every line the consumer
//! previously admitted is still there, unchanged, before any new one.

use std::collections::BTreeMap;
use std::sync::Arc;

use pith_diag::{SourceFile, Span};
use pith_hir::{FrontendCode, ModuleSubject};

use super::super::INDEX_DIRECTORY;
use super::super::keys::{Detached, Public};
use super::super::line::Line;
use super::super::trust::DomainKeys;
use super::{Admitted, Reading, key_list, segment};
use crate::workspace::graph::{at, sourceless};

/// A line's assertion: who signed it, the canonical bytes covered, and
/// the signature over them.
type Assertion<'a> = (&'a Public, &'a str, &'a Detached);

impl Reading<'_> {
    /// The subject-line checks, over every domain and subject the index
    /// carries.
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
                    self.diagnostics.push(at(
                        FrontendCode::UnreadableSource,
                        malformed.span,
                        source,
                        malformed.message,
                    ));
                    continue;
                }
            };
            // Previously admitted lines answer to the append-only check,
            // not the current key set: rotation retires a key for what it
            // may sign next, not for what it signed while enrolled.
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
                        self.diagnostics.push(at(
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
            self.diagnostics.push(at(
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
            self.diagnostics.push(at(
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
            self.diagnostics.push(at(
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

    /// The append-only check: the lines the consumer previously admitted
    /// for this subject are a prefix of the lines it just read, because
    /// new entries append and nothing else.
    fn append_only(&mut self, subject: &ModuleSubject, source: &Arc<SourceFile>, current: &[Line]) {
        let Some(prior) = self.prior.subjects.get(subject) else {
            return;
        };
        for (index, admitted) in prior.iter().enumerate() {
            let still_there = current
                .get(index)
                .is_some_and(|line| line.render() == admitted.as_ref());
            if !still_there {
                self.diagnostics.push(at(
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
}
