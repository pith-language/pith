//! The module solve: a deterministic backtracking search over an acquired
//! universe, with no I/O (its inputs are values). Subjects are decided in
//! canonical order, candidates in the preference's order; two candidates
//! the preference cannot rank are underdetermined rather than ordered by
//! iteration. Exhausting the budget is its own outcome, not evidence of
//! unsatisfiability.

use std::collections::{BTreeMap, BTreeSet};

use pith_hir::ModuleSubject;

use super::model::{
    Derivation, ModuleCandidate, ModuleConstraint, ModuleResolution, ModuleSelection,
    ModuleUniverse, Preference, TrailEntry,
};

/// One solve request.
pub struct SolveRequest {
    pub constraints: Box<[ModuleConstraint]>,
    pub universe: ModuleUniverse,
    pub preference: Preference,
    pub budget: u64,
}

/// How one branch of the search ended. A dead end carries the depth at
/// which it failed, so a failing search reports the branch that actually
/// failed, not the last one tried.
enum Step {
    Done,
    DeadEnd {
        depth: usize,
        derivation: Derivation,
    },
    Underdetermined {
        subject: ModuleSubject,
        tied: Box<[Box<str>]>,
    },
    Exhausted,
}

/// The search state for one solve.
struct Search<'a> {
    candidates: BTreeMap<ModuleSubject, Vec<&'a ModuleCandidate>>,
    constraints: Vec<ModuleConstraint>,
    preference: Preference,
    remaining: u64,
    decisions: u64,
    trail: Vec<TrailEntry>,
    chosen: BTreeMap<ModuleSubject, &'a ModuleCandidate>,
}

#[must_use]
pub fn resolve(request: &SolveRequest) -> ModuleResolution {
    let universe = request.universe.content_id();
    let mut search = Search {
        candidates: candidates_by_subject(&request.universe),
        constraints: canonical_constraints(&request.constraints),
        preference: request.preference,
        remaining: request.budget,
        decisions: 0,
        trail: Vec::new(),
        chosen: BTreeMap::new(),
    };
    match search.descend() {
        Step::Done => ModuleResolution::Solved {
            selections: search
                .chosen
                .values()
                .map(|candidate| ModuleSelection {
                    subject: candidate.subject.clone(),
                    version: candidate.version.clone(),
                })
                .collect::<Vec<_>>()
                .into(),
            trail: std::mem::take(&mut search.trail).into(),
            universe,
        },
        Step::DeadEnd { derivation, .. } => ModuleResolution::Unsatisfiable { derivation },
        Step::Underdetermined { subject, tied } => {
            ModuleResolution::Underdetermined { subject, tied }
        }
        Step::Exhausted => ModuleResolution::BudgetExhausted {
            budget: request.budget,
            decisions: search.decisions,
        },
    }
}

/// Candidates keyed by subject, each list in the canonical order the
/// universe holds.
fn candidates_by_subject(
    universe: &ModuleUniverse,
) -> BTreeMap<ModuleSubject, Vec<&ModuleCandidate>> {
    let mut by_subject: BTreeMap<ModuleSubject, Vec<&ModuleCandidate>> = BTreeMap::new();
    for candidate in universe.candidates() {
        by_subject
            .entry(candidate.subject.clone())
            .or_default()
            .push(candidate);
    }
    by_subject
}

/// Constraints sorted by subject, range, attribution, with duplicates
/// removed, so construction order never reaches the answer.
fn canonical_constraints(constraints: &[ModuleConstraint]) -> Vec<ModuleConstraint> {
    let mut constraints = constraints.to_vec();
    constraints.sort_by(|left, right| {
        left.subject
            .cmp(&right.subject)
            .then_with(|| left.range.to_string().cmp(&right.range.to_string()))
            .then_with(|| left.attribution.cmp(&right.attribution))
    });
    constraints.dedup();
    constraints
}

impl<'a> Search<'a> {
    /// Decide every constrained subject, or report how the branch failed.
    fn descend(&mut self) -> Step {
        let Some(subject) = self.next_subject() else {
            return Step::Done;
        };
        let available = self.available_of(&subject);
        let Some(admitted) = self.admitted_of(&subject) else {
            return self.dead_end(&subject, available);
        };
        let order = self.ordered(&admitted);
        let mut deepest: Option<(usize, Derivation)> = None;
        for (index, candidate) in order.iter().enumerate() {
            if let Some(step) = self.charge() {
                return step;
            }
            if let Some(step) = self.tie_ahead(&subject, &order, index) {
                return step;
            }
            let pushed = self.commit(subject.clone(), candidate);
            let branch = if self.violates_chosen() {
                self.dead_end(&subject, available)
            } else {
                self.descend()
            };
            match branch {
                Step::Done => return Step::Done,
                Step::Exhausted => {
                    self.uncommit(pushed);
                    return Step::Exhausted;
                }
                Step::Underdetermined { subject, tied } => {
                    self.uncommit(pushed);
                    return Step::Underdetermined { subject, tied };
                }
                Step::DeadEnd { depth, derivation } => {
                    if deepest.as_ref().is_none_or(|(known, _)| depth > *known) {
                        deepest = Some((depth, derivation));
                    }
                    self.uncommit(pushed);
                }
            }
        }
        match deepest {
            // A child's dead end is deeper than this subject's own and
            // names the branch that failed.
            Some((_, derivation)) => Step::DeadEnd {
                depth: self.chosen.len(),
                derivation,
            },
            None => self.dead_end(&subject, available),
        }
    }

    /// The first constrained subject not yet decided, in canonical order.
    fn next_subject(&self) -> Option<ModuleSubject> {
        self.constraints
            .iter()
            .map(|constraint| &constraint.subject)
            .filter(|subject| !self.chosen.contains_key(*subject))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .next()
            .cloned()
    }

    /// How many candidates the universe holds for `subject`.
    fn available_of(&self, subject: &ModuleSubject) -> usize {
        self.candidates.get(subject).map_or(0, Vec::len)
    }

    /// Candidates for `subject` that every constraint on it admits, or
    /// `None` when the universe holds no candidates for it at all: a dead
    /// end here, distinct from an empty admitted set.
    fn admitted_of(&self, subject: &ModuleSubject) -> Option<Vec<&'a ModuleCandidate>> {
        let available = self.candidates.get(subject)?;
        Some(
            available
                .iter()
                .copied()
                .filter(|candidate| {
                    self.constraints
                        .iter()
                        .filter(|constraint| &constraint.subject == subject)
                        .all(|constraint| constraint.range.satisfies(&candidate.version))
                })
                .collect(),
        )
    }

    /// The admitted candidates in the preference's order.
    fn ordered<'candidate>(
        &self,
        admitted: &[&'candidate ModuleCandidate],
    ) -> Vec<&'candidate ModuleCandidate> {
        let mut ordered = admitted.to_vec();
        if matches!(self.preference, Preference::Newest) {
            ordered.reverse();
        }
        ordered
    }

    /// Charge one candidate decision, or report exhaustion.
    fn charge(&mut self) -> Option<Step> {
        if self.remaining == 0 {
            return Some(Step::Exhausted);
        }
        self.remaining = self.remaining.saturating_sub(1);
        self.decisions = self.decisions.saturating_add(1);
        None
    }

    /// Whether the candidate about to be tried ties with its successor
    /// under the preference: two realizations of one version the ordering
    /// cannot rank, refused rather than resolved by iteration order.
    fn tie_ahead(
        &self,
        subject: &ModuleSubject,
        order: &[&ModuleCandidate],
        index: usize,
    ) -> Option<Step> {
        let (this, next) = (order.get(index)?, order.get(index.saturating_add(1))?);
        (this.version == next.version).then(|| Step::Underdetermined {
            subject: subject.clone(),
            tied: Box::new([candidate_identity(this), candidate_identity(next)]),
        })
    }

    /// Commit one candidate: record the trail entry, decide the subject,
    /// and add its requirements as constraints. Returns how many
    /// constraints were added, for the matching uncommit.
    fn commit(&mut self, subject: ModuleSubject, candidate: &'a ModuleCandidate) -> usize {
        let considered = self.candidates.get(&subject).map_or(0, Vec::len);
        let decided_by = self.decided_by(&subject);
        self.trail.push(TrailEntry {
            subject,
            considered,
            decided_by,
        });
        self.chosen.insert(candidate.subject.clone(), candidate);
        let added = candidate.requires.len();
        for requirement in &candidate.requires {
            self.constraints.push(ModuleConstraint {
                subject: requirement.subject.clone(),
                range: requirement.range.clone(),
                attribution: requires_attribution(candidate),
            });
        }
        added
    }

    /// Undo one candidate's decision.
    fn uncommit(&mut self, pushed: usize) {
        for _ in 0..pushed {
            self.constraints.pop();
        }
        if let Some(entry) = self.trail.pop() {
            self.chosen.remove(&entry.subject);
        }
    }

    /// Whether a requirement the branch just added contradicts a subject it
    /// already decided.
    fn violates_chosen(&self) -> bool {
        self.constraints.iter().any(|constraint| {
            self.chosen
                .get(&constraint.subject)
                .is_some_and(|chosen| !constraint.range.satisfies(&chosen.version))
        })
    }

    /// The attribution that decided among a subject's candidates: the
    /// constraint that admits fewest of them, or the alphabetically first
    /// when several tie.
    fn decided_by(&self, subject: &ModuleSubject) -> Box<str> {
        let Some(available) = self.candidates.get(subject) else {
            return Box::from("unconstrained");
        };
        self.constraints
            .iter()
            .filter(|constraint| &constraint.subject == subject)
            .map(|constraint| {
                (
                    constraint.attribution.clone(),
                    available
                        .iter()
                        .filter(|candidate| constraint.range.satisfies(&candidate.version))
                        .count(),
                )
            })
            .min_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)))
            .map_or_else(
                || Box::from("unconstrained"),
                |(attribution, _)| attribution,
            )
    }

    /// A dead end at `subject`: the constraints in force on it, and how
    /// many candidates it had.
    fn dead_end(&self, subject: &ModuleSubject, available: usize) -> Step {
        let constraints = self
            .constraints
            .iter()
            .filter(|constraint| &constraint.subject == subject)
            .cloned()
            .collect::<Vec<_>>();
        Step::DeadEnd {
            depth: self.chosen.len(),
            derivation: Derivation {
                subject: subject.clone(),
                constraints: constraints.into(),
                candidates: available,
            },
        }
    }
}

/// A candidate's identity among tied peers: its version at its origin.
fn candidate_identity(candidate: &ModuleCandidate) -> Box<str> {
    format!(
        "{} @ {}",
        candidate.version.canonical_spelling(),
        candidate.origin
    )
    .into()
}

/// Whose requirement a constraint came from: the declaring candidate, by
/// subject and version.
fn requires_attribution(candidate: &ModuleCandidate) -> Box<str> {
    format!(
        "{} {}",
        candidate.subject,
        candidate.version.canonical_spelling()
    )
    .into()
}
