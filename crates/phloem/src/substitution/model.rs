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

/// A binary offered for a locked package binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BinaryOffer {
    pub package: PackageVersion,
    /// Canonically sorted feature coordinates.
    pub features: Box<[Box<str>]>,
    /// The source content the publisher claims to have built.
    pub built_from: ContentId,
    pub platform: ExecutionPlatform,
    /// The toolchain value used to build the binary.
    pub toolchain: Value,
    /// The publisher's claimed identity for the binary bytes.
    pub claimed: ContentId,
    pub origin: Origin,
}

impl BinaryOffer {
    /// Returns a length-delimited canonical key over every offer field.
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

/// Origins whose binary offers may be admitted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdmittedOrigins(pub Box<[Origin]>);

impl AdmittedOrigins {
    /// Returns the admitted origin equal to `offered`.
    #[must_use]
    pub fn covering(&self, offered: &Origin) -> Option<&Origin> {
        self.0.iter().find(|admitted| *admitted == offered)
    }
}

/// Run-specific inputs to binary admission.
#[derive(Clone, Copy, Debug)]
pub struct Admission<'a> {
    pub entry: &'a LockEntry,
    pub platform: &'a ExecutionPlatform,
    /// The toolchain value used by this run's build requests.
    pub toolchain: &'a Value,
    pub origins: &'a AdmittedOrigins,
}

/// The evidence retained for an admitted substitution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Admitted {
    pub package: PackageVersion,
    pub features: Box<[Box<str>]>,
    /// The source content bound by the lock.
    pub built_from: ContentId,
    pub platform: ExecutionPlatform,
    /// The admitted toolchain value.
    pub toolchain: Value,
    /// The identity measured from the binary bytes.
    pub measured: ContentId,
    pub authorized_by: Origin,
}

/// The declared substitution record's name.
pub const SUBSTITUTION: &str = "phloem.Substitution";

const BUILT_FROM: &str = "built-from";
const BINARY: &str = "binary";
const AUTHORIZED_BY: &str = "authorized-by";

/// Returns the declared substitution record type.
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
    /// Encodes the admitted substitution as its declared record type.
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

    /// Decodes an admitted substitution from `value`.
    ///
    /// # Errors
    /// Returns a diagnostic when `value` is not a substitution record.
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
