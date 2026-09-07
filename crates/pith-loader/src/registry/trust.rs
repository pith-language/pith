//! The verified surface: what a client keeps after reading an index.
//!
//! The walk that runs the checks lives in [`super::reading`]; [`read`]
//! is the only constructor of [`Verified`], so acquisition cannot reach
//! a line that did not pass.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use pith_diag::{Diag, Severity};
use pith_hir::{ManifestVersion, ModuleSubject, RootKey};

use super::keys::Public;
use super::line::{Release, Withdrawal};
use super::reading::Reading;

/// The index state a consumer previously admitted: each subject's lines,
/// canonically rendered, and each domain's key-set generation. The next
/// read must still carry every recorded line, unchanged, before any new
/// ones.
#[derive(Clone, Debug, Default)]
pub struct Record {
    pub(crate) subjects: BTreeMap<ModuleSubject, Box<[Box<str>]>>,
    pub(crate) domains: BTreeMap<Box<str>, u64>,
}

impl Record {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            subjects: BTreeMap::new(),
            domains: BTreeMap::new(),
        }
    }
}

/// An index that passed the four checks.
#[derive(Debug)]
pub struct Verified {
    root: Public,
    subjects: BTreeMap<ModuleSubject, Subject>,
}

impl Verified {
    /// The root key every key set was verified against — the trust claim
    /// two routes to one registry must agree on.
    #[must_use]
    pub const fn root(&self) -> &Public {
        &self.root
    }

    #[must_use]
    pub fn subject(&self, subject: &ModuleSubject) -> Option<&Subject> {
        self.subjects.get(subject)
    }
}

/// A domain's admitted keys, at the generation the consumer read.
#[derive(Debug)]
pub(crate) struct DomainKeys {
    pub(crate) generation: u64,
    pub(crate) keys: BTreeSet<Public>,
}

/// One subject's lines as the client admitted them.
#[derive(Debug)]
pub struct Subject {
    releases: BTreeMap<Box<str>, Release>,
    withdrawals: Box<[Withdrawal]>,
}

impl Subject {
    /// The admitted releases, in canonical version order.
    pub fn releases(&self) -> impl Iterator<Item = &Release> {
        self.releases.values()
    }

    /// The withdrawal standing against `version`, if any.
    #[must_use]
    pub fn withdrawal_for(&self, version: &ManifestVersion) -> Option<&Withdrawal> {
        self.withdrawals
            .iter()
            .find(|withdrawal| withdrawal.version() == version)
    }
}

/// Reads and verifies the index at `directory` under `pinned`, against the
/// state `prior` admits.
///
/// # Errors
/// Returns every refusal the index earned, each attached to the file and
/// line that caused it.
pub fn read(
    directory: &Path,
    pinned: &RootKey,
    prior: &Record,
) -> Result<(Verified, Record), Box<[Diag]>> {
    let mut reading = Reading::new(directory, prior);
    let Some(root) = reading.root(pinned) else {
        return Err(reading.into_diagnostics());
    };
    let domains = reading.domains(&root);
    let subjects = reading.subjects(&domains);
    if reading
        .diagnostics()
        .iter()
        .any(|diag| diag.severity == Severity::Error)
    {
        return Err(reading.into_diagnostics());
    }
    let record = Record {
        subjects: subjects
            .iter()
            .map(|(subject, admitted)| {
                (
                    subject.clone(),
                    admitted
                        .rendered
                        .iter()
                        .map(|line| Box::from(line.render().as_str()))
                        .collect::<Vec<_>>()
                        .into(),
                )
            })
            .collect(),
        domains: domains
            .iter()
            .map(|(domain, keys)| (domain.clone(), keys.generation))
            .collect(),
    };
    Ok((
        Verified {
            root,
            subjects: subjects
                .into_iter()
                .map(|(subject, admitted)| {
                    (
                        subject,
                        Subject {
                            releases: admitted.releases,
                            withdrawals: admitted.withdrawals.into(),
                        },
                    )
                })
                .collect(),
        },
        record,
    ))
}
