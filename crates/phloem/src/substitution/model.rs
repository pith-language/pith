use pith_core::{Type, Value};
use pith_diag::PithResult;
use pith_engine::ExecutionPlatform;
use pith_ids::ContentId;

use crate::codec::{
    FIELD_ARCHITECTURE, FIELD_DOMAIN, FIELD_FEATURES, FIELD_OPERATING_SYSTEM, FIELD_PACKAGE,
    FIELD_TOOLCHAIN, FIELD_VERSION, blob_field, field_of, record_type, record_value, text_field,
    text_list,
};
use crate::identity::{DomainIdentity, PackageIdentity, PackageVersion};
use crate::lock::{LockEntry, Origin};

/// A binary someone else built, offered for coordinates a lock binds.
///
/// Every field but `origin` is a claim the offer makes about itself, and each
/// is checked against something this run holds. The origin is evidence in the
/// entry's sense (0039) and is what the local authorization ranges over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BinaryOffer {
    pub package: PackageVersion,
    /// The feature coordinate, canonically sorted the way a lock entry's is:
    /// features are coordinates (0040), so an offer built with other features
    /// is an offer for other coordinates.
    pub features: Box<[Box<str>]>,
    /// The source content the publisher claims to have built. The clause that
    /// ties the offer to a binding: a lock binds coordinates to source, and
    /// this is the offer's statement that it realized that binding.
    pub built_from: ContentId,
    pub platform: ExecutionPlatform,
    /// The toolchain the binary claims to have been built under, as the value
    /// the run's build requests carry (xylem's toolchain value). Platform and
    /// toolchain are the request-input half of a realization's identity
    /// (0039), so an offer under another toolchain realizes something this
    /// run would not. The leg compares the values, so a claim spelled any
    /// other way is a claim about another toolchain rather than another
    /// spelling of this one.
    pub toolchain: Value,
    /// The digest the publisher claims for the binary's bytes, which the test
    /// measures rather than believes.
    pub claimed: ContentId,
    pub origin: Origin,
}

impl BinaryOffer {
    /// The canonical order over offers: the claim's every field,
    /// length-prefixed so no field can run into the next. A caller holding
    /// several offers for one identity orders them by this key before
    /// running the admission test, so which offer serves is a function of
    /// the offer set and not of the slice it arrived in.
    #[must_use]
    pub fn canonical_key(&self) -> Vec<u8> {
        fn push(key: &mut Vec<u8>, part: &[u8]) {
            key.extend_from_slice(&(part.len() as u64).to_be_bytes());
            key.extend_from_slice(part);
        }
        let mut key = Vec::new();
        push(
            &mut key,
            self.package.identity().domain().as_str().as_bytes(),
        );
        push(&mut key, self.package.identity().name().as_bytes());
        push(&mut key, self.package.version().as_bytes());
        for feature in self.features.iter() {
            push(&mut key, feature.as_bytes());
        }
        push(&mut key, self.built_from.digest().as_bytes());
        push(&mut key, self.platform.operating_system.as_bytes());
        push(&mut key, self.platform.architecture.as_bytes());
        push(&mut key, &self.toolchain.encode_canonical());
        push(&mut key, self.claimed.digest().as_bytes());
        push(&mut key, self.origin.kind().as_bytes());
        push(&mut key, self.origin.location().as_bytes());
        key
    }

    #[must_use]
    pub fn new(
        package: PackageVersion,
        features: impl IntoIterator<Item = impl Into<Box<str>>>,
        built_from: ContentId,
        platform: ExecutionPlatform,
        toolchain: Value,
        claimed: ContentId,
        origin: Origin,
    ) -> Self {
        let mut features: Vec<Box<str>> = features.into_iter().map(Into::into).collect();
        features.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        Self {
            package,
            features: features.into(),
            built_from,
            platform,
            toolchain,
            claimed,
            origin,
        }
    }
}

/// The authorization M-4 ships in place of a key: the origins whose offers
/// this run will consider at all.
///
/// Nix separates the substituter list from `trusted-public-keys` because
/// where a binary is fetched from and whose signature authorizes it are
/// different questions. With no keys there is one question left, and this is
/// it, stated as a decision rather than left implicit in a URL.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdmittedOrigins(pub Box<[Origin]>);

impl AdmittedOrigins {
    /// The admitted origin `offered` matches, if any.
    #[must_use]
    pub fn covering(&self, offered: &Origin) -> Option<&Origin> {
        self.0.iter().find(|admitted| *admitted == offered)
    }
}

/// What the run brings to the admission test that the offer does not: the
/// binding to substitute for, the realization coordinates it would build
/// under, and the local authorization.
#[derive(Clone, Copy, Debug)]
pub struct Admission<'a> {
    pub entry: &'a LockEntry,
    pub platform: &'a ExecutionPlatform,
    /// The toolchain this run's build requests carry. The same value a
    /// caller hands to [`crate::substitution::serving_request`], so the leg cannot be spelled
    /// independently of the build it guards.
    pub toolchain: &'a Value,
    pub origins: &'a AdmittedOrigins,
}

/// Every input the admission test consulted, carried out of it.
///
/// A substitution rests on exactly these values, so a caller reporting one
/// reports what it rested on, and a test perturbing any one of them sees the
/// outcome become the [`crate::substitution::Refusal`] that names it. This is also the provenance
/// record a served substitution is: a value over the fields below, on the
/// terms the lock entry and the description are values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Admitted {
    pub package: PackageVersion,
    pub features: Box<[Box<str>]>,
    /// The source the lock bound, which the offer's claim matched.
    pub built_from: ContentId,
    pub platform: ExecutionPlatform,
    /// The toolchain the test matched, as the value the run's build requests
    /// carry.
    pub toolchain: Value,
    /// The digest computed from the bytes read, not the one the offer
    /// claimed. 0014's distinction: this is the measurement.
    pub measured: ContentId,
    pub authorized_by: Origin,
}

/// The declared substitution record's name.
pub const SUBSTITUTION: &str = "phloem.Substitution";

const BUILT_FROM: &str = "built-from";
const BINARY: &str = "binary";
const AUTHORIZED_BY: &str = "authorized-by";

/// The declared substitution record type: the binding's full coordinates and
/// bound source, the realization coordinates, the substituted content
/// identity, and the origin whose claim the policy admitted. A substitution
/// crosses processes as this value, the piece the lock's refusal of binaries
/// leaves unwitnessed.
#[must_use]
pub fn substitution_type() -> Type {
    record_type([
        (FIELD_DOMAIN, Type::Text),
        (FIELD_PACKAGE, Type::Text),
        (FIELD_VERSION, Type::Text),
        (FIELD_FEATURES, Type::List(Box::new(Type::Text))),
        (BUILT_FROM, Type::Blob),
        (FIELD_OPERATING_SYSTEM, Type::Text),
        (FIELD_ARCHITECTURE, Type::Text),
        (
            FIELD_TOOLCHAIN,
            Type::Nominal {
                name: xylem::types::TOOLCHAIN.into(),
            },
        ),
        (BINARY, Type::Blob),
        (AUTHORIZED_BY, crate::lock::origin_type()),
    ])
}

impl Admitted {
    /// The record as a value of the declared type.
    #[must_use]
    pub fn to_value(&self) -> Value {
        record_value([
            (
                FIELD_DOMAIN,
                Value::Text(self.package.identity().domain().as_str().into()),
            ),
            (
                FIELD_PACKAGE,
                Value::Text(self.package.identity().name().into()),
            ),
            (FIELD_VERSION, Value::Text(self.package.version().into())),
            (
                FIELD_FEATURES,
                Value::List(
                    self.features
                        .iter()
                        .map(|feature| Value::Text(feature.clone()))
                        .collect(),
                ),
            ),
            (BUILT_FROM, Value::Blob(self.built_from)),
            (
                FIELD_OPERATING_SYSTEM,
                Value::Text(self.platform.operating_system.clone()),
            ),
            (
                FIELD_ARCHITECTURE,
                Value::Text(self.platform.architecture.clone()),
            ),
            (FIELD_TOOLCHAIN, self.toolchain.clone()),
            (BINARY, Value::Blob(self.measured)),
            (AUTHORIZED_BY, self.authorized_by.to_value()),
        ])
    }

    /// Read a substitution record from a value, checking inhabitation with
    /// `is_type` rather than comparing against `value_type` (0026's
    /// asymmetry, inherited by every record whose lists can be empty).
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming the declared type and the value
    /// found when the value is not a substitution record.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        if !value.is_type(&substitution_type()) {
            return Err(crate::diag(format!(
                "expected a value of the {SUBSTITUTION} type, found {}",
                value.describe()
            )));
        }
        let Value::Record(fields) = value else {
            return Err(crate::diag(format!(
                "expected a value of the {SUBSTITUTION} type, found {}",
                value.describe()
            )));
        };
        let domain = text_field(fields, FIELD_DOMAIN)?;
        let package = text_field(fields, FIELD_PACKAGE)?;
        let version = text_field(fields, FIELD_VERSION)?;
        let features = match field_of(fields, FIELD_FEATURES) {
            Some(payload) => text_list(payload, FIELD_FEATURES)?,
            None => {
                return Err(crate::diag(format!(
                    "the record carried no {FIELD_FEATURES} set"
                )));
            }
        };
        let built_from = blob_field(fields, BUILT_FROM)?;
        let operating_system = text_field(fields, FIELD_OPERATING_SYSTEM)?;
        let architecture = text_field(fields, FIELD_ARCHITECTURE)?;
        let toolchain = match field_of(fields, FIELD_TOOLCHAIN) {
            Some(payload) => payload.clone(),
            None => {
                return Err(crate::diag(format!(
                    "the record carried no {FIELD_TOOLCHAIN}"
                )));
            }
        };
        let measured = blob_field(fields, BINARY)?;
        let authorized_by = match field_of(fields, AUTHORIZED_BY) {
            Some(payload) => Origin::from_value(payload)?,
            None => {
                return Err(crate::diag(format!(
                    "the record carried no {AUTHORIZED_BY}"
                )));
            }
        };
        Ok(Self {
            package: PackageVersion::new(
                PackageIdentity::declare(DomainIdentity::new(domain), package),
                version,
            ),
            features: features.into(),
            built_from,
            platform: ExecutionPlatform {
                operating_system,
                architecture,
            },
            toolchain,
            measured,
            authorized_by,
        })
    }
}
