//! The key-set checks: every domain key set is signed by the pinned
//! root, and no generation moved backwards.

use std::collections::BTreeMap;

use pith_diag::{ByteOffset, Span};
use pith_hir::FrontendCode;

use super::super::DOMAINS_DIRECTORY;
use super::super::keys::Public;
use super::super::line::KeySet;
use super::super::trust::DomainKeys;
use super::Reading;
use crate::workspace::graph::{at, sourceless};

impl Reading<'_> {
    /// The key-set checks over every domain directory.
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
            let key_set = match KeySet::parse(source.source_text(), ByteOffset(0)) {
                Ok(key_set) => key_set,
                Err(malformed) => {
                    self.diagnostics.push(at(
                        FrontendCode::UnreadableSource,
                        malformed.span,
                        &source,
                        malformed.message,
                    ));
                    continue;
                }
            };
            if !root.verify(key_set.render_unsigned().as_bytes(), key_set.signature()) {
                self.diagnostics.push(at(
                    FrontendCode::UnsignedKeySet,
                    Span::new(ByteOffset(0), ByteOffset(0)),
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
                self.diagnostics.push(at(
                    FrontendCode::UnreadableSource,
                    Span::new(ByteOffset(0), ByteOffset(0)),
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
                self.diagnostics.push(at(
                    FrontendCode::KeySetRollback,
                    Span::new(ByteOffset(0), ByteOffset(0)),
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
}
