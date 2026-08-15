//! The search: the provisional half of resolution (decision 0040).
//!
//! 0040's unresolved section leaves the solver algorithm open on the
//! evidence that a candidate universe of real pith-package scale does not
//! exist yet. What is settled is the protocol the search answers to — the
//! request values, the answer values, the determinism contract — so this
//! module is the part a second solver replaces: the types, codec, and host
//! rule wiring around it stay.
//!
//! The algorithm here is deliberately the simplest one that satisfies the
//! protocol: a depth-first walk over subjects in canonical identity order,
//! candidates grouped by the preference list with tie groups refused rather
//! than picked apart by search order, backtracking across groups, the
//! budget charged per candidate tried. Subjects use canonical identity
//! order, while candidates retain their input order until the declared
//! preferences group them. Engine requests supply canonical value order;
//! direct callers supply the host slice order. An unresolved tie is refused,
//! so neither order silently selects within it.

use std::collections::BTreeMap;

use crate::constraint::Constraint;
use crate::identity::{DomainIdentity, PackageIdentity, VersionScheme};
use crate::preference::{Preference, PreferenceList};
use crate::resolution::{Derivation, Resolution, TrailEntry};
use crate::universe::{Candidate, CandidateUniverse};

/// The solver request: the four declared inputs, as typed data, beside the
/// ordering the request names.
#[derive(Clone, Debug)]
pub struct SolveRequest {
    pub constraints: Box<[Constraint]>,
    pub universe: CandidateUniverse,
    pub preferences: PreferenceList,
    /// The search budget, in decisions taken. A deterministic unit, never
    /// wall-clock: an exhausted answer is a function of the inputs like any
    /// other and may be cached (0040).
    pub budget: u64,
}

/// The solver. A pure function of the request under `scheme`: the same
/// request under the same declared ordering always produces the same answer.
#[must_use]
pub fn resolve(scheme: &dyn VersionScheme, request: &SolveRequest) -> Resolution {
    let mut search = Search::new(scheme, request);
    let mut assignment = Vec::new();
    match search.run(&mut assignment) {
        Step::Done => Resolution::Solved {
            choice: assignment
                .iter()
                .map(|candidate| (*candidate).clone())
                .collect(),
            trail: search
                .trail
                .iter()
                .map(|(subject, entry)| TrailEntry {
                    subject: subject.clone(),
                    considered: entry.considered,
                    decided_by: entry.decided_by.clone(),
                })
                .collect(),
            universe: request.universe.content_id(),
        },
        Step::Underdetermined { subject, tied } => Resolution::Underdetermined {
            subject,
            tied,
            orderings: request.preferences.clone(),
        },
        Step::BudgetExhausted => Resolution::BudgetExhausted {
            budget: request.budget,
            decisions: search.decisions,
        },
        Step::DeadEnd => Resolution::Unsatisfiable {
            derivation: search.deepest_dead_end.take().map_or_else(
                || Derivation {
                    subject: PackageIdentity::declare(
                        DomainIdentity::new("phloem"),
                        "unconstrained",
                    ),
                    constraints: Box::new([]),
                    candidates: Box::new([]),
                },
                |(_, derivation)| derivation,
            ),
        },
    }
}

/// What one step of the search concluded.
enum Step {
    Done,
    DeadEnd,
    Underdetermined {
        subject: PackageIdentity,
        tied: Box<[Candidate]>,
    },
    BudgetExhausted,
}

struct Trail {
    considered: usize,
    decided_by: Box<str>,
}

struct Search<'a> {
    scheme: &'a dyn VersionScheme,
    preferences: &'a PreferenceList,
    budget: u64,
    decisions: u64,
    candidates: BTreeMap<PackageIdentity, Vec<&'a Candidate>>,
    constraints: BTreeMap<PackageIdentity, Vec<Constraint>>,
    trail: Vec<(PackageIdentity, Trail)>,
    deepest_dead_end: Option<(usize, Derivation)>,
    depth: usize,
}

impl<'a> Search<'a> {
    fn new(scheme: &'a dyn VersionScheme, request: &'a SolveRequest) -> Self {
        let mut candidates: BTreeMap<PackageIdentity, Vec<&'a Candidate>> = BTreeMap::new();
        for candidate in request.universe.candidates.iter() {
            candidates
                .entry(candidate.identity.clone())
                .or_default()
                .push(candidate);
        }
        let mut constraints: BTreeMap<PackageIdentity, Vec<Constraint>> = BTreeMap::new();
        for constraint in request.constraints.iter() {
            constraints
                .entry(constraint.subject.clone())
                .or_default()
                .push(constraint.clone());
        }
        Self {
            scheme,
            preferences: &request.preferences,
            budget: request.budget,
            decisions: 0,
            candidates,
            constraints,
            trail: Vec::new(),
            deepest_dead_end: None,
            depth: 0,
        }
    }

    /// The next subject to decide: the first in canonical identity order
    /// among those constrained but not yet assigned.
    fn next_subject(&self, assigned: &[&'a Candidate]) -> Option<PackageIdentity> {
        self.constraints
            .keys()
            .find(|subject| {
                !assigned
                    .iter()
                    .any(|candidate| candidate.identity == **subject)
            })
            .cloned()
    }

    fn run(&mut self, assigned: &mut Vec<&'a Candidate>) -> Step {
        let Some(subject) = self.next_subject(assigned) else {
            return Step::Done;
        };
        let Some(available) = self.candidates.get(&subject) else {
            self.record_dead_end(&subject, Box::new([]));
            return Step::DeadEnd;
        };
        let satisfying = self.satisfying(&subject, available);
        if satisfying.is_empty() {
            self.record_dead_end(
                &subject,
                available.iter().map(|c| c.version.clone()).collect(),
            );
            return Step::DeadEnd;
        }
        let ordered = self.ordered_by_preference(satisfying.clone());
        let decided_by = self.deciding_ordering(&ordered);
        // Candidates in tie groups, best group first. A group of more than
        // one candidate is underdetermination: no declared ordering
        // separates its members, and trying them in order would be picking by
        // search order, so the group is reached only to be refused.
        for (candidate, group) in self.tie_groups(ordered) {
            if group.len() > 1 {
                return Step::Underdetermined {
                    subject: subject.clone(),
                    tied: group.iter().map(|c| (*c).clone()).collect(),
                };
            }
            self.decisions = self.decisions.saturating_add(1);
            if self.decisions > self.budget {
                return Step::BudgetExhausted;
            }
            let outcome = self.descend(
                &subject,
                satisfying.len(),
                &decided_by,
                &requirements_of(candidate),
                candidate,
                assigned,
            );
            match outcome {
                Step::Done => return Step::Done,
                Step::DeadEnd => {}
                Step::Underdetermined { .. } | Step::BudgetExhausted => return outcome,
            }
        }
        // Every candidate for this subject was tried and dead-ended below;
        // the deepest dead end recorded on the way down is the derivation.
        Step::DeadEnd
    }

    /// The candidates for `subject` that every constraint in force admits.
    fn satisfying(
        &self,
        subject: &PackageIdentity,
        available: &[&'a Candidate],
    ) -> Vec<&'a Candidate> {
        let in_force = self.constraints.get(subject).cloned().unwrap_or_default();
        available
            .iter()
            .copied()
            .filter(|candidate| {
                in_force.iter().all(|constraint| {
                    constraint.admits(self.scheme, &candidate.version, &candidate.features)
                })
            })
            .collect()
    }

    /// Candidates best-first under the declared orderings. The stable sort
    /// retains the candidate slice's order within a tie; engine requests
    /// obtain that slice from the universe's canonical value. Tied candidates
    /// stay adjacent and are refused rather than picked by iteration order.
    fn ordered_by_preference(&self, satisfying: Vec<&'a Candidate>) -> Vec<&'a Candidate> {
        let mut ordered = satisfying;
        ordered.sort_by(|left, right| {
            self.preferences
                .compare(self.scheme, &right.version, &left.version)
        });
        ordered
    }

    /// Which declared ordering separated the best candidate from the rest,
    /// or `sole-candidate` when none was needed because only one candidate
    /// satisfied or nothing separated them.
    fn deciding_ordering(&self, ordered: &[&'a Candidate]) -> Box<str> {
        let Some(best) = ordered.first() else {
            unreachable!("a satisfying candidate exists");
        };
        ordered
            .iter()
            .skip(1)
            .find_map(|other| {
                self.preferences
                    .separator(self.scheme, &best.version, &other.version)
                    .map(Preference::name)
            })
            .map_or_else(|| Box::from("sole-candidate"), Box::from)
    }

    /// Group best-first candidates by the declared orderings: adjacent
    /// candidates the orderings do not separate form one group.
    fn tie_groups(&self, ordered: Vec<&'a Candidate>) -> Vec<(&'a Candidate, Vec<&'a Candidate>)> {
        let mut groups: Vec<(&'a Candidate, Vec<&'a Candidate>)> = Vec::new();
        for candidate in ordered {
            match groups.last_mut() {
                Some((head, group))
                    if self
                        .preferences
                        .compare(self.scheme, &head.version, &candidate.version)
                        == std::cmp::Ordering::Equal =>
                {
                    group.push(candidate)
                }
                _ => groups.push((candidate, vec![candidate])),
            }
        }
        groups
    }

    /// Try one candidate for `subject`: put its requirements in force,
    /// record the trail entry, and continue the walk underneath it. Every
    /// addition is undone on the way out — here, in one body — unless the
    /// walk underneath solved the request, because the answer is read from
    /// the state a solving frame built.
    fn descend(
        &mut self,
        subject: &PackageIdentity,
        considered: usize,
        decided_by: &str,
        added: &[Constraint],
        candidate: &'a Candidate,
        assigned: &mut Vec<&'a Candidate>,
    ) -> Step {
        self.push_constraints(added);
        // A requirement may land on a subject already assigned higher in
        // the walk; a choice that violates it is a dead end exactly like
        // one that empties a later subject, and the derivation names the
        // assigned subject with the violated requirement in force.
        if let Some(violated) = self.violation(added, assigned) {
            let rejected: Box<[Box<str>]> = assigned
                .iter()
                .find(|chosen| chosen.identity == violated)
                .map_or_else(Vec::new, |chosen| vec![chosen.version.clone()])
                .into_boxed_slice();
            self.record_dead_end(&violated, rejected);
            self.pop_constraints(added);
            return Step::DeadEnd;
        }
        self.trail.push((
            subject.clone(),
            Trail {
                considered,
                decided_by: Box::from(decided_by),
            },
        ));
        self.depth = self.depth.saturating_add(1);
        assigned.push(candidate);
        let outcome = self.run(assigned);
        match outcome {
            Step::Done => outcome,
            Step::DeadEnd | Step::Underdetermined { .. } | Step::BudgetExhausted => {
                assigned.pop();
                self.trail.pop();
                self.depth = self.depth.saturating_sub(1);
                self.pop_constraints(added);
                outcome
            }
        }
    }

    /// Put one frame's added constraints in force.
    fn push_constraints(&mut self, added: &[Constraint]) {
        for constraint in added {
            self.constraints
                .entry(constraint.subject.clone())
                .or_default()
                .push(constraint.clone());
        }
    }

    /// Withdraw one frame's added constraints. Each pop removes the entry
    /// the paired push added; the two are called only as a pair inside
    /// `descend`, which is what keeps them balanced.
    fn pop_constraints(&mut self, added: &[Constraint]) {
        for constraint in added {
            if let Some(entries) = self.constraints.get_mut(&constraint.subject) {
                entries.pop();
            }
        }
    }

    /// The first subject an already-assigned choice violates a requirement
    /// about, if any.
    fn violation(&self, added: &[Constraint], assigned: &[&Candidate]) -> Option<PackageIdentity> {
        added.iter().find_map(|constraint| {
            assigned.iter().find_map(|chosen| {
                (chosen.identity == constraint.subject
                    && !constraint.admits(self.scheme, &chosen.version, &chosen.features))
                .then(|| constraint.subject.clone())
            })
        })
    }

    /// Remember the dead end at the greatest depth reached: the derivation
    /// names the subject that emptied, the constraints in force over it, and
    /// the candidate versions that were available.
    fn record_dead_end(&mut self, subject: &PackageIdentity, candidates: Box<[Box<str>]>) {
        let constraints = self.constraints.get(subject).cloned().unwrap_or_default();
        let derivation = Derivation {
            subject: subject.clone(),
            constraints: constraints.into(),
            candidates,
        };
        if self
            .deepest_dead_end
            .as_ref()
            .is_none_or(|current| self.depth > current.0)
        {
            self.deepest_dead_end = Some((self.depth, derivation));
        }
    }
}

/// The constraints choosing `candidate` adds: its requirements, attributed
/// to its coordinates.
fn requirements_of(candidate: &Candidate) -> Vec<Constraint> {
    candidate
        .requires
        .iter()
        .map(|requirement| Constraint {
            subject: requirement.subject.clone(),
            range: requirement.range.clone(),
            features: requirement.features.clone(),
            attribution: attribution_of(candidate),
        })
        .collect()
}

fn attribution_of(candidate: &Candidate) -> Box<str> {
    format!(
        "candidate {}/{} {}",
        candidate.identity.domain().as_str(),
        candidate.identity.name(),
        candidate.version
    )
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint::{Bound, Range};
    use crate::identity::NumericSegments;
    use crate::source::SourceBinding;
    use crate::universe::Requirement;
    use pith_ids::ContentId;

    fn identity(name: &str) -> PackageIdentity {
        PackageIdentity::declare(DomainIdentity::new("pithpkgs"), name)
    }

    fn candidate(name: &str, version: &str) -> Candidate {
        Candidate {
            identity: identity(name),
            version: version.into(),
            features: Box::new([]),
            provenance: SourceBinding::Archive {
                archive: ContentId::of_blob(name.as_bytes()),
            },
            origin: crate::lock::Origin::Registry("pkgs.pith-lang.org".into()),
            requires: Box::new([]),
        }
    }

    fn root_constraint(name: &str, range: Range) -> Constraint {
        Constraint {
            subject: identity(name),
            range,
            features: Box::new([]),
            attribution: "root".into(),
        }
    }

    fn request(
        constraints: &[Constraint],
        candidates: &[Candidate],
        preferences: &[Preference],
        budget: u64,
    ) -> SolveRequest {
        SolveRequest {
            constraints: constraints.to_vec().into(),
            universe: CandidateUniverse::new(candidates.to_vec()),
            preferences: PreferenceList(preferences.to_vec().into()),
            budget,
        }
    }

    #[test]
    fn the_newest_candidate_wins_under_the_newest_preference() {
        let solve = request(
            &[root_constraint(
                "zlib",
                Range::AtLeast(Bound::new("1.0", true)),
            )],
            &[candidate("zlib", "1.2"), candidate("zlib", "1.3")],
            &[Preference::Newest],
            100,
        );
        let Resolution::Solved { choice, trail, .. } = resolve(&NumericSegments, &solve) else {
            unreachable!("expected a solution");
        };
        assert_eq!(choice.len(), 1);
        assert_eq!(choice.first().unwrap().version, Box::from("1.3"));
        assert_eq!(trail.first().unwrap().decided_by, Box::from("newest"));
        assert_eq!(trail.first().unwrap().considered, 2);
    }

    #[test]
    fn a_preference_picks_among_valid_solutions_and_never_decides_validity() {
        // The same constraint set over the same universe: newest picks 1.3,
        // oldest picks 1.2, and an unsatisfiable range fails under both
        // preference lists — preferences never enter the intersection that
        // defines valid.
        let constraints = &[root_constraint(
            "zlib",
            Range::AtLeast(Bound::new("1.0", true)),
        )];
        let newest = request(
            constraints,
            &[candidate("zlib", "1.2"), candidate("zlib", "1.3")],
            &[Preference::Newest],
            100,
        );
        let oldest = request(
            constraints,
            &[candidate("zlib", "1.2"), candidate("zlib", "1.3")],
            &[Preference::Oldest],
            100,
        );
        let Resolution::Solved {
            choice: newest_choice,
            ..
        } = resolve(&NumericSegments, &newest)
        else {
            unreachable!("newest resolves");
        };
        let Resolution::Solved {
            choice: oldest_choice,
            ..
        } = resolve(&NumericSegments, &oldest)
        else {
            unreachable!("oldest resolves");
        };
        assert_eq!(newest_choice.first().unwrap().version, Box::from("1.3"));
        assert_eq!(oldest_choice.first().unwrap().version, Box::from("1.2"));

        // The range admits nothing the universe offers, and every preference
        // list says the same: an invalid solution stays invalid whichever
        // ordering would have chosen among valid ones.
        let impossible = &[root_constraint(
            "zlib",
            Range::AtLeast(Bound::new("2.0", true)),
        )];
        for preferences in [
            &[Preference::Newest][..],
            &[Preference::Oldest][..],
            &[][..],
        ] {
            let invalid = request(impossible, &[candidate("zlib", "1.3")], preferences, 100);
            assert!(
                matches!(
                    resolve(&NumericSegments, &invalid),
                    Resolution::Unsatisfiable { .. }
                ),
                "a preference cannot make an invalid solution valid"
            );
        }
    }

    #[test]
    fn an_underdetermined_preference_list_refuses_rather_than_picking() {
        // Two candidates at one version, distinct provenance. No declared
        // ordering separates them — newest compares versions, and the
        // versions are one version — so the resolver refuses, naming the
        // tied candidates and the orderings that failed to separate them.
        let mut second = candidate("zlib", "1.3");
        second.provenance = SourceBinding::Path {
            path: "vendor/zlib".into(),
            content: ContentId::of_blob(b"local-zlib"),
        };
        let solve = request(
            &[root_constraint("zlib", Range::Any)],
            &[candidate("zlib", "1.3"), second],
            &[Preference::Newest],
            100,
        );
        let Resolution::Underdetermined {
            tied, orderings, ..
        } = resolve(&NumericSegments, &solve)
        else {
            unreachable!("expected an underdetermination refusal");
        };
        assert_eq!(tied.len(), 2, "the refusal names every tied candidate");
        assert_eq!(orderings, PreferenceList(Box::new([Preference::Newest])));

        // An empty list underdetermines any choice among distinct versions:
        // no declared fact distinguishes the candidates at all.
        let empty = request(
            &[root_constraint("zlib", Range::Any)],
            &[candidate("zlib", "1.2"), candidate("zlib", "1.3")],
            &[],
            100,
        );
        let Resolution::Underdetermined { tied, .. } = resolve(&NumericSegments, &empty) else {
            unreachable!("an empty preference list cannot pick");
        };
        assert_eq!(tied.len(), 2);
    }

    #[test]
    fn an_unsatisfiable_answer_carries_a_derivation_and_a_budget_exhaustion_does_not() {
        // Three versions of `a`, each requiring `b >= 2` while the root
        // requires `b < 2`: no solution exists, and the derivation names the
        // subject that emptied and the constraints in force over it. The
        // same request under a budget of one decision is exhaustion, a fact
        // about the run, and says nothing about solvability.
        let requiring = |version: &str| Candidate {
            requires: Box::new([Requirement {
                subject: identity("b"),
                range: Range::AtLeast(Bound::new("2.0", true)),
                features: Box::new([]),
            }]),
            ..candidate("a", version)
        };
        let constraints = &[
            root_constraint("a", Range::Any),
            root_constraint("b", Range::AtMost(Bound::new("1.9", true))),
        ];
        let candidates = &[
            requiring("1.0"),
            requiring("1.1"),
            requiring("1.2"),
            candidate("b", "1.5"),
        ];
        let generous = request(constraints, candidates, &[Preference::Newest], 100);
        let Resolution::Unsatisfiable { derivation } = resolve(&NumericSegments, &generous) else {
            unreachable!("no solution exists");
        };
        assert_eq!(derivation.subject, identity("b"));
        assert!(
            derivation
                .constraints
                .iter()
                .any(|c| c.attribution.as_ref() == "root"),
            "the derivation names the root constraint: {derivation:?}"
        );
        assert!(
            derivation
                .constraints
                .iter()
                .any(|c| c.attribution.as_ref().starts_with("candidate ")),
            "the derivation names the chosen candidate's requirement: {derivation:?}"
        );

        let starved = request(constraints, candidates, &[Preference::Newest], 1);
        match resolve(&NumericSegments, &starved) {
            Resolution::BudgetExhausted { budget, decisions } => {
                assert_eq!(budget, 1);
                assert!(decisions > budget);
            }
            other => unreachable!("a starved budget is exhaustion, not {other:?}"),
        }
    }

    #[test]
    fn a_constrained_subject_absent_from_the_universe_is_unsatisfiable() {
        let solve = request(
            &[root_constraint(
                "openssl",
                Range::AtLeast(Bound::new("1.0", true)),
            )],
            &[candidate("zlib", "1.3")],
            &[Preference::Newest],
            100,
        );
        let Resolution::Unsatisfiable { derivation } = resolve(&NumericSegments, &solve) else {
            unreachable!("no candidate exists for the subject");
        };
        assert_eq!(derivation.subject, identity("openssl"));
        assert!(derivation.candidates.is_empty());
    }

    #[test]
    fn a_feature_constraint_selects_against_the_newest_preference() {
        // The newest candidate lacks the required feature, so the constraint
        // — not the preference — selects: the choice is the older candidate
        // whose coordinates carry `shared`. This is 0039's handed-over
        // question made concrete: the feature constraint speaks over
        // coordinates, before any realization exists.
        let mut shared = candidate("openssl", "1.1");
        shared.features = Box::new(["shared".into()]);
        let plain = candidate("openssl", "1.2");
        let constraints = &[Constraint {
            subject: identity("openssl"),
            range: Range::AtLeast(Bound::new("1.0", true)),
            features: Box::new(["shared".into()]),
            attribution: "root".into(),
        }];
        let solve = request(constraints, &[shared, plain], &[Preference::Newest], 100);
        let Resolution::Solved { choice, trail, .. } = resolve(&NumericSegments, &solve) else {
            unreachable!("the feature-selecting constraint resolves");
        };
        assert_eq!(choice.first().unwrap().version, Box::from("1.1"));
        assert_eq!(
            trail.first().unwrap().decided_by,
            Box::from("sole-candidate")
        );
    }
}
