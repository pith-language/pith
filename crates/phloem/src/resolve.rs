//! Resolution: the solver as a host rule in the graph (decision 0040).
//!
//! The solver request names four values — the constraint set, the candidate
//! universe, the preference list, the search budget in deterministic units —
//! and the answer names two: the choice, one candidate per constrained
//! subject, and the explanation. Because the inputs are values they
//! participate in the computation key, so a changed universe or constraint
//! is a different computation and the reusable index never serves a stale
//! resolution.
//!
//! The body is a host rule on 0038's declared tier: the search is not
//! structurally recursive, so it has no represented spelling under 0018, and
//! it owes the engine only the determinism contract — the answer is a pure
//! function of the four inputs. The search's interior (backtracking, host
//! maps keyed by subject) is invisible to the engine, which is the same
//! epistemic position it holds toward every host rule; the value spellings
//! stay canonically sorted lists, whose sortedness is what the digest and
//! the diff need.
//!
//! The algorithm here is deliberately the simplest one that satisfies the
//! protocol: a depth-first walk over subjects in canonical order, candidates
//! in preference order, refusing whenever a declared ordering fails to
//! separate the candidates it must choose among. 0040's unresolved section
//! leaves the algorithm open on the evidence that real-scale candidate
//! universes do not exist yet; this is not that evidence.

use std::collections::BTreeMap;

use pith_core::{
    Interface, Pure, RecordField, Request, Rule, RuleIdentity, RuleRevision, SumConstructor, Type,
    Value,
};
use pith_diag::{PithResult, Span};
use pith_engine::{PureRule, PureRuleFrame, PureStep, Resumption};
use pith_ids::ContentId;

use crate::constraint::{Constraint, constraint_set_type};
use crate::identity::{DomainIdentity, PackageIdentity, VersionScheme};
use crate::preference::{Preference, PreferenceList, preference_list_type, preference_list_value};
use crate::universe::{Candidate, CandidateUniverse, candidate_type, universe_type};

/// The declared resolution sum's name.
pub const RESOLUTION: &str = "phloem.Resolution";

const SOLVED: &str = "Solved";
const UNSATISFIABLE: &str = "Unsatisfiable";
const UNDERDETERMINED: &str = "Underdetermined";
const BUDGET_EXHAUSTED: &str = "BudgetExhausted";

const CHOICE: &str = "choice";
const TRAIL: &str = "trail";
const UNIVERSE: &str = "universe";
const DERIVATION: &str = "derivation";
const TIED: &str = "tied";
const ORDERINGS: &str = "orderings";
const BUDGET: &str = "budget";
const DECISIONS: &str = "decisions";
const CONSIDERED: &str = "considered";
const DECIDED_BY: &str = "decided_by";
const CANDIDATES: &str = "candidates";
const CONSTRAINTS: &str = "constraints";

/// The solver request: the four declared inputs, as typed data.
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

/// The derivation a failure carries: the subject that could not be
/// satisfied, the constraints in force over it — with the attribution of
/// each, root or chosen candidate — and the candidate versions that were
/// available when it emptied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Derivation {
    pub subject: PackageIdentity,
    pub constraints: Box<[Constraint]>,
    pub candidates: Box<[Box<str>]>,
}

/// One entry of a success's decision trail: which subject, how many
/// candidates satisfied the constraints in force, and which declared
/// ordering chose the winner — `sole-candidate` when no ordering was needed
/// because only one candidate satisfied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrailEntry {
    pub subject: PackageIdentity,
    pub considered: usize,
    pub decided_by: Box<str>,
}

/// The solver answer: the choice plus the explanation, as one value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    Solved {
        choice: Box<[Candidate]>,
        trail: Box<[TrailEntry]>,
        /// The digest of the candidate universe the choice resolved against,
        /// so a recorded answer names the universe it came from.
        universe: ContentId,
    },
    /// No solution exists, and here is the derivation. Distinct from every
    /// other constructor: this is a fact about the problem.
    Unsatisfiable { derivation: Derivation },
    /// The preference list failed to separate candidates it had to choose
    /// among. A refusal on 0015's terms: no declared fact distinguishes the
    /// tied candidates, and picking among them would record a choice nothing
    /// explains. Distinct from `Unsatisfiable`: nothing here says no solution
    /// exists.
    Underdetermined {
        subject: PackageIdentity,
        tied: Box<[Candidate]>,
        orderings: PreferenceList,
    },
    /// The search budget ran out. A fact about the run, never about the
    /// problem: it does not say no solution exists (0040, on the ground 0022
    /// drew between a failure and an interruption).
    BudgetExhausted { budget: u64, decisions: u64 },
}

/// The trail-entry record type: `{domain, package, considered, decided_by}`.
#[must_use]
pub fn trail_entry_type() -> Type {
    let record = Type::record([
        RecordField {
            name: crate::constraint::DOMAIN.into(),
            payload: Type::Text,
        },
        RecordField {
            name: crate::constraint::PACKAGE.into(),
            payload: Type::Text,
        },
        RecordField {
            name: CONSIDERED.into(),
            payload: Type::Int,
        },
        RecordField {
            name: DECIDED_BY.into(),
            payload: Type::Text,
        },
    ]);
    record.unwrap_or_else(|error| unreachable!("{error}"))
}

/// The derivation record type: `{domain, package, constraints, candidates}`.
#[must_use]
pub fn derivation_type() -> Type {
    let record = Type::record([
        RecordField {
            name: crate::constraint::DOMAIN.into(),
            payload: Type::Text,
        },
        RecordField {
            name: crate::constraint::PACKAGE.into(),
            payload: Type::Text,
        },
        RecordField {
            name: CONSTRAINTS.into(),
            payload: constraint_set_type(),
        },
        RecordField {
            name: CANDIDATES.into(),
            payload: Type::List(Box::new(Type::Text)),
        },
    ]);
    record.unwrap_or_else(|error| unreachable!("{error}"))
}

/// The declared resolution sum type: `Solved({choice, trail, universe})`,
/// `Unsatisfiable({derivation})`, `Underdetermined({domain, package, tied,
/// orderings})`, `BudgetExhausted({budget, decisions})`.
#[must_use]
pub fn resolution_type() -> Type {
    let solved = record_type([
        (CHOICE, Type::List(Box::new(candidate_type()))),
        (TRAIL, Type::List(Box::new(trail_entry_type()))),
        (UNIVERSE, Type::Blob),
    ]);
    let unsatisfiable = record_type([(DERIVATION, derivation_type())]);
    let underdetermined = record_type([
        (crate::constraint::DOMAIN, Type::Text),
        (crate::constraint::PACKAGE, Type::Text),
        (TIED, Type::List(Box::new(candidate_type()))),
        (ORDERINGS, preference_list_type()),
    ]);
    let budget_exhausted = record_type([(BUDGET, Type::Int), (DECISIONS, Type::Int)]);
    let sum = Type::sum(
        RESOLUTION,
        [
            SumConstructor {
                name: SOLVED.into(),
                payload: Some(solved),
            },
            SumConstructor {
                name: UNSATISFIABLE.into(),
                payload: Some(unsatisfiable),
            },
            SumConstructor {
                name: UNDERDETERMINED.into(),
                payload: Some(underdetermined),
            },
            SumConstructor {
                name: BUDGET_EXHAUSTED.into(),
                payload: Some(budget_exhausted),
            },
        ],
    );
    sum.unwrap_or_else(|error| unreachable!("{error}"))
}

/// The resolution interface: `(constraint set, candidate universe,
/// preference list, budget) -> phloem.Resolution`, selected by 0015 like any
/// other interface.
#[must_use]
pub fn resolve_interface() -> Interface {
    Interface {
        inputs: Box::new([
            constraint_set_type(),
            universe_type(),
            preference_list_type(),
            Type::Int,
        ]),
        output: resolution_type(),
    }
}

fn record_type<const N: usize>(fields: [(&str, Type); N]) -> Type {
    let record = Type::record(fields.map(|(name, payload)| RecordField {
        name: name.into(),
        payload,
    }));
    record.unwrap_or_else(|error| unreachable!("{error}"))
}

fn record_value<const N: usize>(fields: [(&str, Value); N]) -> Value {
    let record = Value::record(fields.map(|(name, payload)| RecordField {
        name: name.into(),
        payload,
    }));
    record.unwrap_or_else(|error| unreachable!("{error}"))
}

fn identity_fields(subject: &PackageIdentity) -> [(&'static str, Value); 2] {
    [
        (
            crate::constraint::DOMAIN,
            Value::Text(subject.domain().as_str().into()),
        ),
        (crate::constraint::PACKAGE, text_value(subject.name())),
    ]
}

fn text_value(content: &str) -> Value {
    Value::Text(content.into())
}

fn subject_of(fields: &[RecordField<Value>]) -> Option<PackageIdentity> {
    let mut domain = None;
    let mut package = None;
    for field in fields.iter() {
        match field.name.as_ref() {
            crate::constraint::DOMAIN => {
                if let Value::Text(text) = &field.payload {
                    domain = Some(text.clone());
                }
            }
            crate::constraint::PACKAGE => {
                if let Value::Text(text) = &field.payload {
                    package = Some(text.clone());
                }
            }
            _ => {}
        }
    }
    Some(PackageIdentity::declare(
        DomainIdentity::new(domain?),
        package?,
    ))
}

fn sum_value(constructor: &str, payload: Value) -> Value {
    Value::Sum {
        type_name: RESOLUTION.into(),
        constructor: constructor.into(),
        payload: Some(Box::new(payload)),
    }
}

impl Resolution {
    #[must_use]
    pub fn to_value(&self) -> Value {
        match self {
            Self::Solved {
                choice,
                trail,
                universe,
            } => sum_value(
                SOLVED,
                record_value([
                    (
                        CHOICE,
                        Value::List(choice.iter().map(|c| c.to_value()).collect()),
                    ),
                    (
                        TRAIL,
                        Value::List(
                            trail
                                .iter()
                                .map(|entry| {
                                    let [domain, package] = identity_fields(&entry.subject);
                                    record_value([
                                        domain,
                                        package,
                                        (CONSIDERED, Value::Int(entry.considered as i64)),
                                        (DECIDED_BY, text_value(&entry.decided_by)),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                    (UNIVERSE, Value::Blob(*universe)),
                ]),
            ),
            Self::Unsatisfiable { derivation } => sum_value(
                UNSATISFIABLE,
                record_value([(
                    DERIVATION,
                    record_value([
                        (
                            crate::constraint::DOMAIN,
                            Value::Text(derivation.subject.domain().as_str().into()),
                        ),
                        (
                            crate::constraint::PACKAGE,
                            text_value(derivation.subject.name()),
                        ),
                        (
                            CONSTRAINTS,
                            crate::constraint::constraint_set_value(&derivation.constraints),
                        ),
                        (
                            CANDIDATES,
                            Value::List(
                                derivation
                                    .candidates
                                    .iter()
                                    .map(|version| text_value(version))
                                    .collect(),
                            ),
                        ),
                    ]),
                )]),
            ),
            Self::Underdetermined {
                subject,
                tied,
                orderings,
            } => sum_value(
                UNDERDETERMINED,
                record_value([
                    (
                        crate::constraint::DOMAIN,
                        Value::Text(subject.domain().as_str().into()),
                    ),
                    (crate::constraint::PACKAGE, text_value(subject.name())),
                    (
                        TIED,
                        Value::List(tied.iter().map(|c| c.to_value()).collect()),
                    ),
                    (ORDERINGS, preference_list_value(orderings)),
                ]),
            ),
            Self::BudgetExhausted { budget, decisions } => sum_value(
                BUDGET_EXHAUSTED,
                record_value([
                    (BUDGET, Value::Int(*budget as i64)),
                    (DECISIONS, Value::Int(*decisions as i64)),
                ]),
            ),
        }
    }

    /// Read a resolution from a value.
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming what was found when the value
    /// is not a resolution of the declared sum.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        if !value.is_type(&resolution_type()) {
            return Err(crate::diag(format!(
                "expected a value of the {RESOLUTION} sum, found {}",
                value.describe()
            )));
        }
        let Value::Sum {
            constructor,
            payload,
            ..
        } = value
        else {
            return Err(crate::diag(format!(
                "expected a value of the {RESOLUTION} sum, found {}",
                value.describe()
            )));
        };
        let Some(payload) = payload.as_deref() else {
            return Err(crate::diag(format!(
                "the {RESOLUTION} constructor `{constructor}` carried no payload"
            )));
        };
        match constructor.as_ref() {
            SOLVED => {
                let fields = record_fields(payload)?;
                let choice = candidate_list(fields, CHOICE)?;
                let universe = blob_of(fields, UNIVERSE)?;
                let mut trail = Vec::new();
                if let Some(Value::List(entries)) = field_of(fields, TRAIL) {
                    for entry in entries.iter() {
                        let entry_fields = record_fields(entry)?;
                        trail.push(TrailEntry {
                            subject: subject_of(entry_fields).ok_or_else(missing_subject)?,
                            considered: int_of(entry_fields, CONSIDERED)? as usize,
                            decided_by: text_of(entry_fields, DECIDED_BY)?,
                        });
                    }
                }
                Ok(Self::Solved {
                    choice: choice.into(),
                    trail: trail.into(),
                    universe,
                })
            }
            UNSATISFIABLE => {
                let outer = record_fields(payload)?;
                let Some(Value::Record(derivation)) = field_of(outer, DERIVATION) else {
                    return Err(crate::diag(format!(
                        "the {DERIVATION} field carried no derivation record"
                    )));
                };
                let mut constraints = Vec::new();
                if let Some(Value::List(entries)) = field_of(derivation, CONSTRAINTS) {
                    for entry in entries.iter() {
                        constraints.push(Constraint::from_value(entry)?);
                    }
                }
                let mut candidates = Vec::new();
                if let Some(Value::List(entries)) = field_of(derivation, CANDIDATES) {
                    for entry in entries.iter() {
                        candidates.push(match entry {
                            Value::Text(text) => text.clone(),
                            other => {
                                return Err(crate::diag(format!(
                                    "the derivation's candidate list carried {} rather than a text",
                                    other.describe()
                                )));
                            }
                        });
                    }
                }
                Ok(Self::Unsatisfiable {
                    derivation: Derivation {
                        subject: subject_of(derivation).ok_or_else(missing_subject)?,
                        constraints: constraints.into(),
                        candidates: candidates.into(),
                    },
                })
            }
            UNDERDETERMINED => {
                let fields = record_fields(payload)?;
                let mut tied = Vec::new();
                if let Some(Value::List(entries)) = field_of(fields, TIED) {
                    for entry in entries.iter() {
                        tied.push(Candidate::from_value(entry)?);
                    }
                }
                let mut orderings = Vec::new();
                if let Some(Value::List(entries)) = field_of(fields, ORDERINGS) {
                    for entry in entries.iter() {
                        orderings.push(Preference::from_value(entry)?);
                    }
                }
                Ok(Self::Underdetermined {
                    subject: subject_of(fields).ok_or_else(missing_subject)?,
                    tied: tied.into(),
                    orderings: PreferenceList(orderings.into()),
                })
            }
            BUDGET_EXHAUSTED => {
                let fields = record_fields(payload)?;
                Ok(Self::BudgetExhausted {
                    budget: int_of(fields, BUDGET)?,
                    decisions: int_of(fields, DECISIONS)?,
                })
            }
            other => Err(crate::diag(format!(
                "the {RESOLUTION} sum carried an unknown constructor `{other}`"
            ))),
        }
    }
}

fn record_fields(value: &Value) -> PithResult<&[RecordField<Value>]> {
    match value {
        Value::Record(fields) => Ok(fields),
        _ => Err(crate::diag(format!(
            "expected a record payload, found {}",
            value.describe()
        ))),
    }
}

fn field_of<'a>(fields: &'a [RecordField<Value>], name: &str) -> Option<&'a Value> {
    fields
        .iter()
        .find(|field| field.name.as_ref() == name)
        .map(|field| &field.payload)
}

fn candidate_list(fields: &[RecordField<Value>], name: &str) -> PithResult<Vec<Candidate>> {
    let Some(Value::List(entries)) = field_of(fields, name) else {
        return Err(crate::diag(format!("the {name} field carried no list")));
    };
    let mut candidates = Vec::with_capacity(entries.len());
    for entry in entries.iter() {
        candidates.push(Candidate::from_value(entry)?);
    }
    Ok(candidates)
}

fn blob_of(fields: &[RecordField<Value>], name: &str) -> PithResult<ContentId> {
    match field_of(fields, name) {
        Some(Value::Blob(id)) => Ok(*id),
        _ => Err(crate::diag(format!("the {name} field carried no blob"))),
    }
}

fn int_of(fields: &[RecordField<Value>], name: &str) -> PithResult<u64> {
    match field_of(fields, name) {
        Some(Value::Int(n)) => u64::try_from(*n)
            .map_err(|_| crate::diag(format!("the {name} field carried a negative integer"))),
        _ => Err(crate::diag(format!("the {name} field carried no integer"))),
    }
}

fn missing_subject() -> pith_diag::DiagnosticSink {
    crate::diag("the record carried no domain and package naming a subject")
}

fn text_of(fields: &[RecordField<Value>], name: &str) -> PithResult<Box<str>> {
    match field_of(fields, name) {
        Some(Value::Text(text)) => Ok(text.clone()),
        _ => Err(crate::diag(format!("the {name} field carried no text"))),
    }
}

/// The solver. A pure function of the four inputs: every ordering it uses is
/// either canonical (subjects in identity order, candidates in value order
/// before preferences are applied) or declared (the domain's version
/// ordering, the preference list), so the same request always produces the
/// same answer.
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
        let in_force = self.constraints.get(&subject).cloned().unwrap_or_default();
        let satisfying: Vec<&'a Candidate> = available
            .iter()
            .copied()
            .filter(|candidate| {
                in_force.iter().all(|constraint| {
                    constraint.admits(self.scheme, &candidate.version, &candidate.features)
                })
            })
            .collect();
        if satisfying.is_empty() {
            self.record_dead_end(
                &subject,
                available.iter().map(|c| c.version.clone()).collect(),
            );
            return Step::DeadEnd;
        }
        // Candidates best-first under the declared orderings; the sort is
        // stable over the universe's canonical value order, so candidates the
        // list does not separate stay adjacent as one group and are refused
        // below rather than picked apart by iteration order.
        let mut ordered = satisfying.clone();
        ordered.sort_by(|left, right| {
            self.preferences
                .compare(self.scheme, &right.version, &left.version)
        });
        let Some(best) = ordered.first() else {
            unreachable!("a satisfying candidate exists");
        };
        let decided_by: Box<str> = if ordered.len() == 1 {
            Box::from("sole-candidate")
        } else {
            ordered
                .iter()
                .skip(1)
                .find_map(|other| {
                    self.preferences
                        .separator(self.scheme, &best.version, &other.version)
                        .map(Preference::name)
                })
                .map_or_else(|| Box::from("sole-candidate"), Box::from)
        };
        // Candidates in tie groups, best group first. A group of more than
        // one candidate is underdetermination: no declared ordering
        // separates its members, and trying them in order would be picking by
        // search order, so the group is reached only to be refused.
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
        for (head, group) in groups {
            if group.len() > 1 {
                return Step::Underdetermined {
                    subject: subject.clone(),
                    tied: group.iter().map(|c| (*c).clone()).collect(),
                };
            }
            let candidate = head;
            self.decisions = self.decisions.saturating_add(1);
            if self.decisions > self.budget {
                return Step::BudgetExhausted;
            }
            let added: Vec<Constraint> = candidate
                .requires
                .iter()
                .map(|requirement| Constraint {
                    subject: requirement.subject.clone(),
                    range: requirement.range.clone(),
                    features: requirement.features.clone(),
                    attribution: attribution_of(candidate),
                })
                .collect();
            for constraint in added.iter() {
                self.constraints
                    .entry(constraint.subject.clone())
                    .or_default()
                    .push(constraint.clone());
            }
            // A requirement may land on a subject already assigned higher in
            // the walk; a choice that violates it is a dead end exactly like
            // one that empties a later subject, and the derivation names the
            // assigned subject with the violated requirement in force.
            let violation = added.iter().find_map(|constraint| {
                assigned.iter().find_map(|chosen| {
                    (chosen.identity == constraint.subject
                        && !constraint.admits(self.scheme, &chosen.version, &chosen.features))
                    .then(|| constraint.subject.clone())
                })
            });
            if let Some(subject) = violation {
                let rejected: Box<[Box<str>]> = assigned
                    .iter()
                    .find(|chosen| chosen.identity == subject)
                    .map_or_else(Vec::new, |chosen| vec![chosen.version.clone()])
                    .into_boxed_slice();
                self.record_dead_end(&subject, rejected);
                for constraint in added.iter() {
                    if let Some(entries) = self.constraints.get_mut(&constraint.subject) {
                        entries.pop();
                    }
                }
                return Step::DeadEnd;
            }
            self.trail.push((
                subject.clone(),
                Trail {
                    considered: satisfying.len(),
                    decided_by: decided_by.clone(),
                },
            ));
            self.depth = self.depth.saturating_add(1);
            assigned.push(candidate);
            let outcome = self.run(assigned);
            match outcome {
                Step::Done => return Step::Done,
                Step::DeadEnd => {}
                Step::Underdetermined { .. } | Step::BudgetExhausted => return outcome,
            }
            assigned.pop();
            self.trail.pop();
            self.depth = self.depth.saturating_sub(1);
            for constraint in added.iter() {
                if let Some(entries) = self.constraints.get_mut(&constraint.subject) {
                    entries.pop();
                }
            }
        }
        // Every candidate for this subject was tried and dead-ended below;
        // the deepest dead end recorded on the way down is the derivation.
        Step::DeadEnd
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

fn attribution_of(candidate: &Candidate) -> Box<str> {
    format!(
        "candidate {}/{} {}",
        candidate.identity.domain().as_str(),
        candidate.identity.name(),
        candidate.version
    )
    .into()
}

/// The solver as a host rule body: one step, completing with the answer
/// value. The engine never sees the search.
pub struct ResolveSolver {
    scheme: Box<dyn VersionScheme + Send + Sync>,
}

impl ResolveSolver {
    #[must_use]
    pub fn new(scheme: impl VersionScheme + Send + Sync + 'static) -> Self {
        Self {
            scheme: Box::new(scheme),
        }
    }

    /// The rule, ready to register against the resolution interface.
    #[must_use]
    pub fn rule(&self) -> Rule<Pure> {
        let identity = RuleIdentity::of_module_declaration("phloem", "resolve");
        Rule::<Pure>::new(
            RuleRevision::of_manifest(identity, b"phloem-resolve-v1"),
            "resolve",
            resolve_interface(),
            Span::none(),
        )
    }
}

impl PureRule for ResolveSolver {
    fn start(&self, inputs: &[Value]) -> Box<dyn pith_engine::PureRuleFrame> {
        Box::new(ResolveFrame {
            answer: Some(solve_from_values(&*self.scheme, inputs)),
        })
    }
}

struct ResolveFrame {
    answer: Option<PithResult<Value>>,
}

impl PureRuleFrame for ResolveFrame {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        match self.answer.take() {
            Some(Ok(value)) => Ok(PureStep::Complete(value)),
            Some(Err(diagnostics)) => Err(diagnostics),
            None => Err(crate::diag("the resolve step ran twice")),
        }
    }
}

/// Decode the four request inputs and run the solver.
fn solve_from_values(scheme: &dyn VersionScheme, inputs: &[Value]) -> PithResult<Value> {
    let [constraints, universe, preferences, budget] = inputs else {
        return Err(crate::diag(format!(
            "a resolve request supplies four inputs; found {}",
            inputs.len()
        )));
    };
    let Value::List(entries) = constraints else {
        return Err(crate::diag(
            "the first resolve input is not a constraint set",
        ));
    };
    let mut parsed = Vec::with_capacity(entries.len());
    for entry in entries.iter() {
        parsed.push(Constraint::from_value(entry)?);
    }
    let preferences = crate::preference::preference_list_from_value(preferences)?;
    let Value::Int(budget) = budget else {
        return Err(crate::diag(
            "the fourth resolve input is not a budget integer",
        ));
    };
    let budget = u64::try_from(*budget)
        .map_err(|_| crate::diag("the resolve budget must not be negative"))?;
    let request = SolveRequest {
        constraints: parsed.into(),
        universe: CandidateUniverse::from_value(universe)?,
        preferences,
        budget,
    };
    Ok(resolve(scheme, &request).to_value())
}

/// A resolve request value against the resolution interface.
#[must_use]
pub fn resolve_request(
    constraints: &Value,
    universe: &Value,
    preferences: &Value,
    budget: u64,
) -> Request<Pure> {
    Request::<Pure>::new(
        "resolve",
        resolve_interface(),
        [
            constraints.clone(),
            universe.clone(),
            preferences.clone(),
            Value::Int(budget as i64),
        ],
        Span::none(),
    )
}

/// The constraint and preference types the interface names, for callers
/// building requests without importing each module.
#[must_use]
pub fn resolve_input_types() -> Box<[Type]> {
    resolve_interface().inputs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint::{Bound, Range};
    use crate::identity::{DomainIdentity, NumericSegments};
    use crate::preference::Preference;
    use crate::source::SourceBinding;
    use crate::universe::Requirement;
    use pith_ids::ContentId;

    fn identity(name: &str) -> PackageIdentity {
        PackageIdentity::declare(DomainIdentity::new("pithpkgs"), name)
    }

    pub(crate) fn candidate(name: &str, version: &str) -> Candidate {
        Candidate {
            identity: identity(name),
            version: version.into(),
            features: Box::new([]),
            provenance: SourceBinding::Archive {
                archive: ContentId::of_blob(name.as_bytes()),
            },
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

    #[test]
    fn a_resolution_round_trips_through_its_value() {
        let solve = request(
            &[root_constraint(
                "zlib",
                Range::AtLeast(Bound::new("1.0", true)),
            )],
            &[candidate("zlib", "1.3")],
            &[Preference::Newest],
            100,
        );
        let resolution = resolve(&NumericSegments, &solve);
        let value = resolution.to_value();
        assert!(value.is_type(&resolution_type()));
        assert_eq!(Resolution::from_value(&value).unwrap(), resolution);
    }
}
