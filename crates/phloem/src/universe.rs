//! The candidate universe: candidates as values carrying the provenance of
//! every candidate (decision 0040).
//!
//! Candidates are 0025-queryable evidence rather than ambient repository
//! state, which is what makes an answer reproducible under the same
//! universe. A candidate carries its coordinates (version and features),
//! the source binding it would resolve from as provenance, and the
//! requirements it declares on other subjects — the requirements are how a
//! choice for one subject constrains the choices for others, which is what
//! makes resolution a search rather than a lookup.

use pith_core::{Type, Value};
use pith_diag::PithResult;
use pith_ids::{ContentDigest, ContentId};

use crate::constraint::{Range, range_type};
use crate::diag;
use crate::identity::{DomainIdentity, PackageIdentity};
use crate::source::{SourceBinding, source_type};

const DOMAIN: &str = "domain";
const PACKAGE: &str = "package";
const VERSION: &str = "version";
const FEATURES: &str = "features";
const PROVENANCE: &str = "provenance";
const REQUIRES: &str = "requires";

/// The digest domain for a candidate universe's content identity.
const UNIVERSE_DOMAIN: &[u8] = b"phloem.candidate-universe-v1\0";

/// A requirement one candidate declares on another subject: a range and a
/// feature set over that subject's coordinates. The requirement is
/// attributed to the candidate carrying it; the attribution a derivation
/// names is the candidate's coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement {
    pub subject: PackageIdentity,
    pub range: Range,
    pub features: Box<[Box<str>]>,
}

/// One candidate: a subject's coordinates, the provenance of those
/// coordinates, and the requirements selecting this candidate imposes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub identity: PackageIdentity,
    pub version: Box<str>,
    pub features: Box<[Box<str>]>,
    pub provenance: SourceBinding,
    pub requires: Box<[Requirement]>,
}

/// The requirement record type: `{domain, package, range, features}`.
#[must_use]
pub fn requirement_type() -> Type {
    let record = pith_core::Type::record([
        pith_core::RecordField {
            name: DOMAIN.into(),
            payload: Type::Text,
        },
        pith_core::RecordField {
            name: PACKAGE.into(),
            payload: Type::Text,
        },
        pith_core::RecordField {
            name: crate::constraint::RANGE_FIELD.into(),
            payload: range_type(),
        },
        pith_core::RecordField {
            name: FEATURES.into(),
            payload: Type::List(Box::new(Type::Text)),
        },
    ]);
    record.unwrap_or_else(|error| unreachable!("{error}"))
}

/// The candidate record type: `{domain, package, version, features,
/// provenance, requires}`.
#[must_use]
pub fn candidate_type() -> Type {
    let record = pith_core::Type::record([
        pith_core::RecordField {
            name: DOMAIN.into(),
            payload: Type::Text,
        },
        pith_core::RecordField {
            name: PACKAGE.into(),
            payload: Type::Text,
        },
        pith_core::RecordField {
            name: VERSION.into(),
            payload: Type::Text,
        },
        pith_core::RecordField {
            name: FEATURES.into(),
            payload: Type::List(Box::new(Type::Text)),
        },
        pith_core::RecordField {
            name: PROVENANCE.into(),
            payload: source_type(),
        },
        pith_core::RecordField {
            name: REQUIRES.into(),
            payload: Type::List(Box::new(requirement_type())),
        },
    ]);
    record.unwrap_or_else(|error| unreachable!("{error}"))
}

/// The candidate-universe type: a canonically sorted list of candidates.
#[must_use]
pub fn universe_type() -> Type {
    Type::List(Box::new(candidate_type()))
}

impl Candidate {
    #[must_use]
    pub fn to_value(&self) -> Value {
        let record = Value::record([
            pith_core::RecordField {
                name: DOMAIN.into(),
                payload: Value::Text(self.identity.domain().as_str().into()),
            },
            pith_core::RecordField {
                name: PACKAGE.into(),
                payload: Value::Text(self.identity.name().into()),
            },
            pith_core::RecordField {
                name: VERSION.into(),
                payload: Value::Text(self.version.clone()),
            },
            pith_core::RecordField {
                name: FEATURES.into(),
                payload: Value::List(
                    self.features
                        .iter()
                        .map(|feature| Value::Text(feature.clone()))
                        .collect(),
                ),
            },
            pith_core::RecordField {
                name: PROVENANCE.into(),
                payload: self.provenance.to_value(),
            },
            pith_core::RecordField {
                name: REQUIRES.into(),
                payload: Value::List(self.requires.iter().map(requirement_value).collect()),
            },
        ]);
        record.unwrap_or_else(|error| unreachable!("{error}"))
    }

    /// Read a candidate from a value.
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming what was found when the value
    /// is not a candidate record.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        if !value.is_type(&candidate_type()) {
            return Err(diag(format!(
                "expected a value of the candidate record type, found {}",
                value.describe()
            )));
        }
        let Value::Record(fields) = value else {
            return Err(diag(format!(
                "expected a value of the candidate record type, found {}",
                value.describe()
            )));
        };
        let mut domain = None;
        let mut package = None;
        let mut version = None;
        let mut features = Vec::new();
        let mut provenance = None;
        let mut requires = Vec::new();
        for field in fields.iter() {
            match field.name.as_ref() {
                DOMAIN => domain = Some(text(&field.payload, DOMAIN)?),
                PACKAGE => package = Some(text(&field.payload, PACKAGE)?),
                VERSION => version = Some(text(&field.payload, VERSION)?),
                FEATURES => {
                    let Value::List(elements) = &field.payload else {
                        return Err(diag(format!(
                            "the {FEATURES} field carried {} rather than a list",
                            field.payload.describe()
                        )));
                    };
                    for element in elements.iter() {
                        features.push(text(element, FEATURES)?);
                    }
                }
                PROVENANCE => provenance = Some(SourceBinding::from_value(&field.payload)?),
                REQUIRES => {
                    let Value::List(elements) = &field.payload else {
                        return Err(diag(format!(
                            "the {REQUIRES} field carried {} rather than a list",
                            field.payload.describe()
                        )));
                    };
                    for element in elements.iter() {
                        requires.push(read_requirement(element)?);
                    }
                }
                _ => {}
            }
        }
        let (Some(domain), Some(package), Some(version), Some(provenance)) =
            (domain, package, version, provenance)
        else {
            return Err(diag(
                "the candidate record was missing a domain, package, version, or provenance field",
            ));
        };
        Ok(Self {
            identity: PackageIdentity::declare(DomainIdentity::new(domain), package),
            version,
            features: features.into(),
            provenance,
            requires: requires.into(),
        })
    }
}

fn requirement_value(requirement: &Requirement) -> Value {
    let record = Value::record([
        pith_core::RecordField {
            name: DOMAIN.into(),
            payload: Value::Text(requirement.subject.domain().as_str().into()),
        },
        pith_core::RecordField {
            name: PACKAGE.into(),
            payload: Value::Text(requirement.subject.name().into()),
        },
        pith_core::RecordField {
            name: crate::constraint::RANGE_FIELD.into(),
            payload: requirement.range.to_value(),
        },
        pith_core::RecordField {
            name: FEATURES.into(),
            payload: Value::List(
                requirement
                    .features
                    .iter()
                    .map(|feature| Value::Text(feature.clone()))
                    .collect(),
            ),
        },
    ]);
    record.unwrap_or_else(|error| unreachable!("{error}"))
}

fn read_requirement(value: &Value) -> PithResult<Requirement> {
    if !value.is_type(&requirement_type()) {
        return Err(diag(format!(
            "expected a value of the requirement record type, found {}",
            value.describe()
        )));
    }
    let Value::Record(fields) = value else {
        return Err(diag(format!(
            "expected a value of the requirement record type, found {}",
            value.describe()
        )));
    };
    let mut domain = None;
    let mut package = None;
    let mut range = None;
    let mut features = Vec::new();
    for field in fields.iter() {
        match field.name.as_ref() {
            DOMAIN => domain = Some(text(&field.payload, DOMAIN)?),
            PACKAGE => package = Some(text(&field.payload, PACKAGE)?),
            crate::constraint::RANGE_FIELD => range = Some(Range::from_value(&field.payload)?),
            FEATURES => {
                let Value::List(elements) = &field.payload else {
                    return Err(diag(format!(
                        "the {FEATURES} field carried {} rather than a list",
                        field.payload.describe()
                    )));
                };
                for element in elements.iter() {
                    features.push(text(element, FEATURES)?);
                }
            }
            _ => {}
        }
    }
    let (Some(domain), Some(package), Some(range)) = (domain, package, range) else {
        return Err(diag(
            "the requirement record was missing a domain, package, or range field",
        ));
    };
    Ok(Requirement {
        subject: PackageIdentity::declare(DomainIdentity::new(domain), package),
        range,
        features: features.into(),
    })
}

fn text(value: &Value, field: &str) -> PithResult<Box<str>> {
    match value {
        Value::Text(text) => Ok(text.clone()),
        _ => Err(diag(format!(
            "the {field} field carried {} rather than a text",
            value.describe()
        ))),
    }
}

/// The candidate universe: every candidate a resolution may choose among,
/// with the provenance of each. Host-side the candidates are a map keyed by
/// identity — the host containers a host rule has (0040's refusal of `Map`)
/// — while the value spelling is a canonically sorted list, whose
/// sortedness is what the digest needs.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CandidateUniverse {
    pub candidates: Box<[Candidate]>,
}

impl CandidateUniverse {
    #[must_use]
    pub fn new(candidates: impl Into<Box<[Candidate]>>) -> Self {
        Self {
            candidates: candidates.into(),
        }
    }

    /// The universe as a canonically sorted value: one spelling for one
    /// universe, so the computation key does not depend on assembly order.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut entries: Vec<(Vec<u8>, Value)> = self
            .candidates
            .iter()
            .map(|c| (c.to_value().encode_canonical(), c.to_value()))
            .collect();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        entries.dedup_by(|front, back| front.0 == back.0);
        Value::List(entries.into_iter().map(|(_, value)| value).collect())
    }

    /// Read a universe from a value.
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming what was found when the value
    /// is not a list of candidate records.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        let Value::List(elements) = value else {
            return Err(diag(format!(
                "expected a candidate universe, found {}",
                value.describe()
            )));
        };
        let mut candidates = Vec::with_capacity(elements.len());
        for element in elements.iter() {
            candidates.push(Candidate::from_value(element)?);
        }
        Ok(Self {
            candidates: candidates.into(),
        })
    }

    /// The universe's content identity: a digest over its canonical value.
    /// A resolution records this beside its choice, so a lock that moves can
    /// name which universe it moved from.
    #[must_use]
    pub fn content_id(&self) -> ContentId {
        let canonical = self.to_value().encode_canonical();
        let mut domain_prefixed = UNIVERSE_DOMAIN.to_vec();
        domain_prefixed.extend_from_slice(&canonical);
        ContentId::from_digest(ContentDigest::of_bytes(&domain_prefixed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint::Bound;
    use crate::source::SourceBinding;
    use pith_ids::ContentId;

    fn identity(name: &str) -> PackageIdentity {
        PackageIdentity::declare(DomainIdentity::new("pithpkgs"), name)
    }

    fn zlib(version: &str, requires: Box<[Requirement]>) -> Candidate {
        Candidate {
            identity: identity("zlib"),
            version: version.into(),
            features: Box::new([]),
            provenance: SourceBinding::Path {
                path: "vendor/zlib".into(),
                content: ContentId::of_blob(b"zlib-tree"),
            },
            requires,
        }
    }

    #[test]
    fn a_candidate_round_trips_through_its_value() {
        let candidate = zlib(
            "1.3",
            Box::new([Requirement {
                subject: identity("openssl"),
                range: Range::AtLeast(Bound::new("1.1", true)),
                features: Box::new(["shared".into()]),
            }]),
        );
        let value = candidate.to_value();
        assert!(value.is_type(&candidate_type()));
        assert_eq!(Candidate::from_value(&value).unwrap(), candidate);
    }

    #[test]
    fn a_universe_has_one_spelling_and_one_digest_regardless_of_assembly_order() {
        let first = zlib("1.3", Box::new([]));
        let mut second = zlib("1.2", Box::new([]));
        second.provenance = SourceBinding::Archive {
            archive: ContentId::of_blob(b"zlib-1.2.tar"),
        };
        let one_way = CandidateUniverse::new(vec![first.clone(), second.clone()]);
        let other_way = CandidateUniverse::new(vec![second, first]);
        assert_eq!(one_way.to_value(), other_way.to_value());
        assert_eq!(one_way.content_id(), other_way.content_id());
    }

    #[test]
    fn a_universe_digest_moves_when_a_candidate_moves() {
        let before = CandidateUniverse::new(vec![zlib("1.3", Box::new([]))]);
        let after = CandidateUniverse::new(vec![zlib("1.3.1", Box::new([]))]);
        assert_ne!(before.content_id(), after.content_id());
        assert_eq!(
            before.content_id(),
            CandidateUniverse::new(vec![zlib("1.3", Box::new([]))]).content_id()
        );
    }

    #[test]
    fn candidates_of_different_subjects_never_collapse_in_the_canonical_spelling() {
        let openssl = Candidate {
            identity: identity("openssl"),
            version: "1.0".into(),
            features: Box::new([]),
            provenance: SourceBinding::Path {
                path: "vendor/openssl".into(),
                content: ContentId::of_blob(b"openssl"),
            },
            requires: Box::new([]),
        };
        let universe = CandidateUniverse::new(vec![zlib("1.3", Box::new([])), openssl]);
        let Value::List(elements) = universe.to_value() else {
            unreachable!("a universe value is a list");
        };
        assert_eq!(elements.len(), 2);
    }
}
