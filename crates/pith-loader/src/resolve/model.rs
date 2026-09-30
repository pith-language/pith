//! The module resolver's vocabulary: constraints over module subjects,
//! candidates from an acquired universe, and the answer a solve returns.
//! Module vocabulary throughout: typed segment versions, no feature
//! coordinates, one selected version per subject.

use pith_hir::{ModuleSubject, ModuleVersion, VersionRange};
use pith_ids::{ContentId, DigestDomain};

/// One hard constraint: which versions of one subject a clause admits, and
/// who declared it. The attribution is what a derivation or trail names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleConstraint {
    pub subject: ModuleSubject,
    pub range: VersionRange,
    pub attribution: Box<str>,
}

/// What one candidate requires of its own dependencies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleRequirement {
    pub subject: ModuleSubject,
    pub range: VersionRange,
}

/// One release of one subject, as an acquired universe carries it: the
/// version, what it requires, and where its bytes may be fetched from.
/// The origin is a locator hint; it never enters the candidate's identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleCandidate {
    pub subject: ModuleSubject,
    pub version: ModuleVersion,
    pub requires: Box<[ModuleRequirement]>,
    pub origin: Box<str>,
}

/// The universe a solve runs over, in one canonical order whatever order
/// it was assembled in: subject, then version, then origin, with exact
/// duplicates collapsed. Construction order is caller policy and must not
/// reach the answer or the computation key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleUniverse {
    candidates: Box<[ModuleCandidate]>,
}

impl ModuleUniverse {
    #[must_use]
    pub fn new(candidates: impl IntoIterator<Item = ModuleCandidate>) -> Self {
        let mut candidates = candidates.into_iter().collect::<Vec<_>>();
        candidates.sort_by(candidate_order);
        candidates.dedup();
        Self {
            candidates: candidates.into(),
        }
    }

    #[must_use]
    pub fn candidates(&self) -> &[ModuleCandidate] {
        &self.candidates
    }

    /// The universe's measured identity: what a lock records and what an
    /// invalidation explanation names.
    #[must_use]
    pub fn content_id(&self) -> ContentId {
        const UNIVERSE_DOMAIN: DigestDomain = DigestDomain::new("module-candidate-universe", 1);
        let mut encoded = Vec::new();
        for candidate in &self.candidates {
            encode_candidate(&mut encoded, candidate);
        }
        ContentId::of_domain(UNIVERSE_DOMAIN, &encoded)
    }
}

/// The canonical order of candidates: subject, then version, then origin.
fn candidate_order(left: &ModuleCandidate, right: &ModuleCandidate) -> std::cmp::Ordering {
    left.subject
        .cmp(&right.subject)
        .then(left.version.cmp(&right.version))
        .then_with(|| left.origin.cmp(&right.origin))
}

/// A candidate's identity fields in a stable, self-delimiting spelling,
/// for the universe digest. Requirements and locators are part of the
/// candidate's meaning, so they are part of its bytes.
fn encode_candidate(encoded: &mut Vec<u8>, candidate: &ModuleCandidate) {
    pith_core::manifest::encode_str(encoded, candidate.subject.spelling().as_ref());
    encoded.extend_from_slice(candidate.version.canonical_spelling().as_bytes());
    encoded.push(0);
    encoded.extend_from_slice(candidate.origin.as_bytes());
    encoded.push(0);
    for requirement in &candidate.requires {
        pith_core::manifest::encode_str(encoded, requirement.subject.spelling().as_ref());
        encoded.extend_from_slice(requirement.range.to_string().as_bytes());
        encoded.push(0);
    }
    encoded.push(0);
}

/// The selection the solver made for one subject (the lock-entry shape):
/// the version a person can point at; the origin stays the candidate's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleSelection {
    pub subject: ModuleSubject,
    pub version: ModuleVersion,
}

/// One decided subject on the trail out of a solve: how many candidates
/// were considered, and whose constraint decided among them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrailEntry {
    pub subject: ModuleSubject,
    pub considered: usize,
    pub decided_by: Box<str>,
}

/// Why a constraint set held no solution, at the deepest subject the
/// search reached: the subject, the constraints in force on it, and how
/// many candidates it had to choose among.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Derivation {
    pub subject: ModuleSubject,
    pub constraints: Box<[ModuleConstraint]>,
    pub candidates: usize,
}

/// A solve's answer. Exhaustion is never evidence of unsatisfiability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleResolution {
    Solved {
        selections: Box<[ModuleSelection]>,
        trail: Box<[TrailEntry]>,
        universe: ContentId,
    },
    Unsatisfiable {
        derivation: Derivation,
    },
    Underdetermined {
        subject: ModuleSubject,
        /// The candidates that tie, each its version at its origin.
        tied: Box<[Box<str>]>,
    },
    BudgetExhausted {
        budget: u64,
        decisions: u64,
    },
}

/// Which end of the version ordering a solve prefers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preference {
    Newest,
    Oldest,
}

impl Preference {
    /// The written name, which is also the value spelling.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Newest => "newest",
            Self::Oldest => "oldest",
        }
    }

    /// # Errors
    /// [`UnknownPreference`] when `name` is not a written name.
    pub fn from_name(name: &str) -> Result<Self, UnknownPreference> {
        match name {
            "newest" => Ok(Self::Newest),
            "oldest" => Ok(Self::Oldest),
            other => Err(UnknownPreference { name: other.into() }),
        }
    }
}

/// A preference name no solve can interpret.
#[derive(Debug, PartialEq, Eq)]
pub struct UnknownPreference {
    pub name: Box<str>,
}

impl std::fmt::Display for UnknownPreference {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "`{}` is not a preference; the written names are `newest` and `oldest`",
            self.name
        )
    }
}

impl std::error::Error for UnknownPreference {}
