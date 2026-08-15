//! The resolution protocol values: the answer as a value, its declared sum,
//! and the interface a resolution is requested against (decision 0040).
//!
//! A solver answer names two things: the choice, one candidate per
//! constrained subject, and the explanation. The four constructors of the
//! `phloem.Resolution` sum keep the distinctions 0040 holds apart — a
//! solution with its decision trail, a proof that no solution exists, a
//! refusal the preference list could not ground, and a budget that ran out,
//! which is a fact about the run and never about the problem.
//!
//! The interface carries the declared version ordering as its first input,
//! nominal over the scheme's declared name, so the computation key covers
//! which ordering a resolution ran under. A solver whose answer depended on
//! an ordering the key did not name would be state neither the request nor
//! the revision covers, the third class 0038 named; naming the scheme in the
//! request closes it on the terms xylem closed the identical toolchain
//! question — the value is the identity, the registered schemes are the
//! lookup table.

use pith_core::{Interface, Pure, RecordField, Request, SumConstructor, Type, Value};
use pith_diag::{PithResult, Span};
use pith_ids::ContentId;

use crate::codec::{
    FIELD_DOMAIN, FIELD_PACKAGE, blob_field, field_of, int_field, record_type, record_value,
    sum_value, text_field,
};
use crate::constraint::{Constraint, constraint_set_type, constraint_set_value};
use crate::identity::{DomainIdentity, PackageIdentity, version_scheme_type};
use crate::preference::{Preference, PreferenceList, preference_list_type, preference_list_value};
use crate::universe::{Candidate, candidate_type, universe_type};

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
    record_type([
        (FIELD_DOMAIN, Type::Text),
        (FIELD_PACKAGE, Type::Text),
        (CONSIDERED, Type::Int),
        (DECIDED_BY, Type::Text),
    ])
}

/// The derivation record type: `{domain, package, constraints, candidates}`.
#[must_use]
pub fn derivation_type() -> Type {
    record_type([
        (FIELD_DOMAIN, Type::Text),
        (FIELD_PACKAGE, Type::Text),
        (CONSTRAINTS, constraint_set_type()),
        (CANDIDATES, Type::List(Box::new(Type::Text))),
    ])
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
        (FIELD_DOMAIN, Type::Text),
        (FIELD_PACKAGE, Type::Text),
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

/// The resolution interface: `(version scheme, constraint set, candidate
/// universe, preference list, budget) -> phloem.Resolution`, selected by 0015
/// like any other interface. The scheme input is the ordering the resolution
/// runs under, named by its declared name so the computation key covers it.
#[must_use]
pub fn resolve_interface() -> Interface {
    Interface {
        inputs: Box::new([
            version_scheme_type(),
            constraint_set_type(),
            universe_type(),
            preference_list_type(),
            Type::Int,
        ]),
        output: resolution_type(),
    }
}

/// A resolve request value against the resolution interface: the declared
/// scheme name and the four protocol inputs.
#[must_use]
pub fn resolve_request(
    scheme: &Value,
    constraints: &Value,
    universe: &Value,
    preferences: &Value,
    budget: u64,
) -> Request<Pure> {
    Request::<Pure>::new(
        "resolve",
        resolve_interface(),
        [
            scheme.clone(),
            constraints.clone(),
            universe.clone(),
            preferences.clone(),
            Value::Int(budget as i64),
        ],
        Span::none(),
    )
}

fn text_value(content: &str) -> Value {
    Value::Text(content.into())
}

fn identity_fields(subject: &PackageIdentity) -> [(&'static str, Value); 2] {
    [
        (FIELD_DOMAIN, Value::Text(subject.domain().as_str().into())),
        (FIELD_PACKAGE, text_value(subject.name())),
    ]
}

fn subject_of(fields: &[RecordField<Value>]) -> Option<PackageIdentity> {
    let mut domain = None;
    let mut package = None;
    for field in fields.iter() {
        match field.name.as_ref() {
            FIELD_DOMAIN => {
                if let Value::Text(text) = &field.payload {
                    domain = Some(text.clone());
                }
            }
            FIELD_PACKAGE => {
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

impl Resolution {
    #[must_use]
    pub fn to_value(&self) -> Value {
        match self {
            Self::Solved {
                choice,
                trail,
                universe,
            } => solved_value(choice, trail, universe),
            Self::Unsatisfiable { derivation } => unsatisfiable_value(derivation),
            Self::Underdetermined {
                subject,
                tied,
                orderings,
            } => underdetermined_value(subject, tied, orderings),
            Self::BudgetExhausted { budget, decisions } => {
                budget_exhausted_value(*budget, *decisions)
            }
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
            SOLVED => solved_from(payload),
            UNSATISFIABLE => unsatisfiable_from(payload),
            UNDERDETERMINED => underdetermined_from(payload),
            BUDGET_EXHAUSTED => budget_exhausted_from(payload),
            other => Err(crate::diag(format!(
                "the {RESOLUTION} sum carried an unknown constructor `{other}`"
            ))),
        }
    }
}

/// The `Solved` constructor's payload: the choice, the decision trail, and
/// the universe digest the choice resolved against.
fn solved_value(choice: &[Candidate], trail: &[TrailEntry], universe: &ContentId) -> Value {
    sum_value(
        RESOLUTION,
        SOLVED,
        Some(record_value([
            (
                CHOICE,
                Value::List(choice.iter().map(|c| c.to_value()).collect()),
            ),
            (
                TRAIL,
                Value::List(trail.iter().map(trail_entry_value).collect()),
            ),
            (UNIVERSE, Value::Blob(*universe)),
        ])),
    )
}

fn solved_from(payload: &Value) -> PithResult<Resolution> {
    let fields = record_fields(payload)?;
    let choice = candidate_list(fields, CHOICE)?;
    let universe = blob_field(fields, UNIVERSE)?;
    let mut trail = Vec::new();
    if let Some(Value::List(entries)) = field_of(fields, TRAIL) {
        for entry in entries.iter() {
            let entry_fields = record_fields(entry)?;
            trail.push(TrailEntry {
                subject: subject_of(entry_fields).ok_or_else(missing_subject)?,
                considered: int_field(entry_fields, CONSIDERED)? as usize,
                decided_by: text_field(entry_fields, DECIDED_BY)?,
            });
        }
    }
    Ok(Resolution::Solved {
        choice: choice.into(),
        trail: trail.into(),
        universe,
    })
}

/// One trail entry as a record: the subject, how many candidates satisfied
/// the constraints in force, and which declared ordering chose the winner.
fn trail_entry_value(entry: &TrailEntry) -> Value {
    let [domain, package] = identity_fields(&entry.subject);
    record_value([
        domain,
        package,
        (CONSIDERED, Value::Int(entry.considered as i64)),
        (DECIDED_BY, text_value(&entry.decided_by)),
    ])
}

/// The `Unsatisfiable` constructor's payload: the derivation naming the
/// subject that could not be satisfied, the constraints in force over it,
/// and the candidate versions that were available.
fn unsatisfiable_value(derivation: &Derivation) -> Value {
    let [domain, package] = identity_fields(&derivation.subject);
    sum_value(
        RESOLUTION,
        UNSATISFIABLE,
        Some(record_value([(
            DERIVATION,
            record_value([
                domain,
                package,
                (CONSTRAINTS, constraint_set_value(&derivation.constraints)),
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
        )])),
    )
}

fn unsatisfiable_from(payload: &Value) -> PithResult<Resolution> {
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
    Ok(Resolution::Unsatisfiable {
        derivation: Derivation {
            subject: subject_of(derivation).ok_or_else(missing_subject)?,
            constraints: constraints.into(),
            candidates: candidates.into(),
        },
    })
}

/// The `Underdetermined` constructor's payload: the subject, the tied
/// candidates no declared ordering separated, and the orderings that failed
/// to separate them.
fn underdetermined_value(
    subject: &PackageIdentity,
    tied: &[Candidate],
    orderings: &PreferenceList,
) -> Value {
    let [domain, package] = identity_fields(subject);
    sum_value(
        RESOLUTION,
        UNDERDETERMINED,
        Some(record_value([
            domain,
            package,
            (
                TIED,
                Value::List(tied.iter().map(|c| c.to_value()).collect()),
            ),
            (ORDERINGS, preference_list_value(orderings)),
        ])),
    )
}

fn underdetermined_from(payload: &Value) -> PithResult<Resolution> {
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
    Ok(Resolution::Underdetermined {
        subject: subject_of(fields).ok_or_else(missing_subject)?,
        tied: tied.into(),
        orderings: PreferenceList(orderings.into()),
    })
}

/// The `BudgetExhausted` constructor's payload: the budget and the decisions
/// taken, the two facts about the run.
fn budget_exhausted_value(budget: u64, decisions: u64) -> Value {
    sum_value(
        RESOLUTION,
        BUDGET_EXHAUSTED,
        Some(record_value([
            (BUDGET, Value::Int(budget as i64)),
            (DECISIONS, Value::Int(decisions as i64)),
        ])),
    )
}

fn budget_exhausted_from(payload: &Value) -> PithResult<Resolution> {
    let fields = record_fields(payload)?;
    Ok(Resolution::BudgetExhausted {
        budget: int_field(fields, BUDGET)?,
        decisions: int_field(fields, DECISIONS)?,
    })
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

fn missing_subject() -> pith_diag::DiagnosticSink {
    crate::diag("the record carried no domain and package naming a subject")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint::Range;
    use crate::identity::version_scheme_value;
    use crate::preference::preference_list_from_value;
    use crate::source::SourceBinding;
    use crate::universe::CandidateUniverse;

    fn identity(name: &str) -> PackageIdentity {
        PackageIdentity::declare(DomainIdentity::new("pithpkgs"), name)
    }

    #[test]
    fn every_constructor_round_trips_through_its_value() {
        let candidate = crate::universe::Candidate {
            identity: identity("zlib"),
            version: "1.3".into(),
            features: Box::new([]),
            provenance: SourceBinding::Path {
                path: "vendor/zlib".into(),
                content: ContentId::of_blob(b"zlib-tree"),
            },
            origin: crate::lock::Origin::LocalPath("vendor/zlib".into()),
            requires: Box::new([]),
        };
        let answers = [
            Resolution::Solved {
                choice: Box::new([candidate.clone()]),
                trail: Box::new([TrailEntry {
                    subject: identity("zlib"),
                    considered: 2,
                    decided_by: "newest".into(),
                }]),
                universe: CandidateUniverse::new(vec![candidate.clone()]).content_id(),
            },
            Resolution::Unsatisfiable {
                derivation: Derivation {
                    subject: identity("openssl"),
                    constraints: Box::new([Constraint {
                        subject: identity("openssl"),
                        range: Range::Any,
                        features: Box::new([]),
                        attribution: "root".into(),
                    }]),
                    candidates: Box::new(["1.1.1".into()]),
                },
            },
            Resolution::Underdetermined {
                subject: identity("zlib"),
                tied: Box::new([candidate]),
                orderings: PreferenceList(Box::new([Preference::Newest])),
            },
            Resolution::BudgetExhausted {
                budget: 1,
                decisions: 2,
            },
        ];
        for answer in &answers {
            let value = answer.to_value();
            assert!(value.is_type(&resolution_type()));
            assert_eq!(Resolution::from_value(&value).unwrap(), *answer);
        }
    }

    #[test]
    fn a_request_names_the_scheme_and_the_four_protocol_inputs() {
        let constraints = constraint_set_value(&[]);
        let universe = CandidateUniverse::default().to_value();
        let preferences = preference_list_value(&PreferenceList(Box::new([])));
        let request = resolve_request(
            &version_scheme_value("numeric-segments"),
            &constraints,
            &universe,
            &preferences,
            7,
        );
        assert_eq!(request.interface, resolve_interface());
        assert!(
            request
                .inputs
                .first()
                .unwrap()
                .is_type(&version_scheme_type()),
            "the scheme input is the interface's first input"
        );
        assert_eq!(request.inputs.len(), 5);
        let _ = preference_list_from_value(request.inputs.get(3).unwrap()).unwrap();
    }
}
