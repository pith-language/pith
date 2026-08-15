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
use pith_ids::ContentId;

use crate::codec::{
    FIELD_DOMAIN, FIELD_FEATURES, FIELD_PACKAGE, FIELD_VERSION, canonical_list, field_of,
    record_type, record_value, text_list, text_of, value_content_id,
};
use crate::constraint::{Range, range_type};
use crate::diag;
use crate::identity::{DomainIdentity, PackageIdentity};
use crate::lock::{Origin, origin_type};
use crate::source::{SourceBinding, source_type};

const PROVENANCE: &str = "provenance";
const ORIGIN: &str = "origin";
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
///
/// The origin is where the candidate was read: the registry identity an
/// index names itself by, the forge a reference resolved against, the path
/// a vendor tree sits at. It is the entry's evidence and cannot be derived
/// from the provenance: a registry archive's digest names its content, not
/// which registry served the claim (0044).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub identity: PackageIdentity,
    pub version: Box<str>,
    pub features: Box<[Box<str>]>,
    pub provenance: SourceBinding,
    pub origin: Origin,
    pub requires: Box<[Requirement]>,
}

/// The requirement record type: `{domain, package, range, features}`.
#[must_use]
pub fn requirement_type() -> Type {
    record_type([
        (FIELD_DOMAIN, Type::Text),
        (FIELD_PACKAGE, Type::Text),
        (crate::constraint::RANGE_FIELD, range_type()),
        (FIELD_FEATURES, Type::List(Box::new(Type::Text))),
    ])
}

/// The candidate record type: `{domain, package, version, features,
/// provenance, origin, requires}`.
#[must_use]
pub fn candidate_type() -> Type {
    record_type([
        (FIELD_DOMAIN, Type::Text),
        (FIELD_PACKAGE, Type::Text),
        (FIELD_VERSION, Type::Text),
        (FIELD_FEATURES, Type::List(Box::new(Type::Text))),
        (PROVENANCE, source_type()),
        (ORIGIN, origin_type()),
        (REQUIRES, Type::List(Box::new(requirement_type()))),
    ])
}

/// The candidate-universe type: a canonically sorted list of candidates.
#[must_use]
pub fn universe_type() -> Type {
    Type::List(Box::new(candidate_type()))
}

impl Candidate {
    #[must_use]
    pub fn to_value(&self) -> Value {
        record_value([
            (
                FIELD_DOMAIN,
                Value::Text(self.identity.domain().as_str().into()),
            ),
            (FIELD_PACKAGE, Value::Text(self.identity.name().into())),
            (FIELD_VERSION, Value::Text(self.version.clone())),
            (
                FIELD_FEATURES,
                Value::List(
                    self.features
                        .iter()
                        .map(|feature| Value::Text(feature.clone()))
                        .collect(),
                ),
            ),
            (PROVENANCE, self.provenance.to_value()),
            (ORIGIN, self.origin.to_value()),
            (
                REQUIRES,
                Value::List(self.requires.iter().map(requirement_value).collect()),
            ),
        ])
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
        let domain = field_of(fields, FIELD_DOMAIN)
            .map(|payload| text_of(payload, FIELD_DOMAIN))
            .transpose()?;
        let package = field_of(fields, FIELD_PACKAGE)
            .map(|payload| text_of(payload, FIELD_PACKAGE))
            .transpose()?;
        let version = field_of(fields, FIELD_VERSION)
            .map(|payload| text_of(payload, FIELD_VERSION))
            .transpose()?;
        let features = match field_of(fields, FIELD_FEATURES) {
            Some(payload) => text_list(payload, FIELD_FEATURES)?,
            None => Vec::new(),
        };
        let provenance = field_of(fields, PROVENANCE)
            .map(SourceBinding::from_value)
            .transpose()?;
        let origin = match field_of(fields, ORIGIN) {
            Some(payload) => Origin::from_value(payload)?,
            None => return Err(diag(format!("the candidate record carried no {ORIGIN}"))),
        };
        let mut requires = Vec::new();
        if let Some(Value::List(entries)) = field_of(fields, REQUIRES) {
            for entry in entries.iter() {
                requires.push(read_requirement(entry)?);
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
            origin,
            requires: requires.into(),
        })
    }
}

fn requirement_value(requirement: &Requirement) -> Value {
    record_value([
        (
            FIELD_DOMAIN,
            Value::Text(requirement.subject.domain().as_str().into()),
        ),
        (
            FIELD_PACKAGE,
            Value::Text(requirement.subject.name().into()),
        ),
        (crate::constraint::RANGE_FIELD, requirement.range.to_value()),
        (
            FIELD_FEATURES,
            Value::List(
                requirement
                    .features
                    .iter()
                    .map(|feature| Value::Text(feature.clone()))
                    .collect(),
            ),
        ),
    ])
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
    let domain = field_of(fields, FIELD_DOMAIN)
        .map(|payload| text_of(payload, FIELD_DOMAIN))
        .transpose()?;
    let package = field_of(fields, FIELD_PACKAGE)
        .map(|payload| text_of(payload, FIELD_PACKAGE))
        .transpose()?;
    let range = field_of(fields, crate::constraint::RANGE_FIELD)
        .map(Range::from_value)
        .transpose()?;
    let features = match field_of(fields, FIELD_FEATURES) {
        Some(payload) => text_list(payload, FIELD_FEATURES)?,
        None => Vec::new(),
    };
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

/// The candidate universe: every candidate a resolution may choose among,
/// with the provenance of each. The solver groups this slice in a host map
/// keyed by identity, while the value spelling is a canonically sorted list
/// whose sortedness is what the digest needs (0040's refusal of `Map`).
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
        canonical_list(self.candidates.iter().map(Candidate::to_value))
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
        value_content_id(UNIVERSE_DOMAIN, &self.to_value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint::Bound;
    use crate::lock::Origin;
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
            origin: Origin::LocalPath("vendor/zlib".into()),
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
            origin: Origin::LocalPath("vendor/openssl".into()),
            requires: Box::new([]),
        };
        let universe = CandidateUniverse::new(vec![zlib("1.3", Box::new([])), openssl]);
        let Value::List(elements) = universe.to_value() else {
            unreachable!("a universe value is a list");
        };
        assert_eq!(elements.len(), 2);
    }
}
