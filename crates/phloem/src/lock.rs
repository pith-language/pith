//! Lock entries and source binding verification.
//!
//! A lock entry binds package coordinates to source content and records the
//! origin as provenance. Origin does not participate in binding identity.

use pith_core::{SumConstructor, Type, Value};
use pith_diag::PithResult;
use pith_ids::ContentId;

use crate::codec::{
    FIELD_DOMAIN, FIELD_FEATURES, FIELD_VERSION, blob_field, field_of, record_type, record_value,
    sum_value, text_field, text_list, value_content_id,
};
use crate::diag;
use crate::identity::PackageVersion;

/// The digest domain for one revision of a lock entry. NUL-terminated so it
/// is self-delimiting against the canonical bytes that follow, mirroring the
/// domain separation `pith-ids` applies to every digest kind it owns.
const LOCK_ENTRY_DOMAIN: &[u8] = b"phloem.lock-entry-v1\0";

/// The declared origin sum's name.
pub const ORIGIN: &str = "phloem.Origin";

const REGISTRY: &str = "Registry";
const FORGE: &str = "Forge";
const LOCAL_PATH: &str = "LocalPath";

/// The origin kinds as the written lock spells them, one per constructor.
pub const KIND_REGISTRY: &str = "registry";
pub const KIND_FORGE: &str = "forge";
pub const KIND_LOCAL_PATH: &str = "local-path";

const NAME: &str = "name";
const SOURCE: &str = "source";
const ORIGIN_FIELD: &str = "origin";

/// The remote or local origin of resolved package content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    Registry(Box<str>),
    Forge(Box<str>),
    LocalPath(Box<str>),
}

impl Origin {
    /// The origin as a value of the declared sum: `Registry(Text)`,
    /// `Forge(Text)`, `LocalPath(Text)`.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let (constructor, location) = match self {
            Self::Registry(location) => (REGISTRY, location),
            Self::Forge(location) => (FORGE, location),
            Self::LocalPath(location) => (LOCAL_PATH, location),
        };
        sum_value(ORIGIN, constructor, Some(Value::Text(location.clone())))
    }

    /// Decodes an origin from `value`.
    ///
    /// # Errors
    /// Returns a diagnostic when `value` is not an origin.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        if !value.is_type(&origin_type()) {
            return Err(diag(format!(
                "expected a value of {ORIGIN}, found {}",
                value.describe()
            )));
        }
        let Value::Sum {
            constructor,
            payload,
            ..
        } = value
        else {
            return Err(diag(format!(
                "expected a value of {ORIGIN}, found {}",
                value.describe()
            )));
        };
        let location = match payload.as_deref() {
            Some(Value::Text(location)) => location.clone(),
            _ => {
                return Err(diag(format!(
                    "the {constructor} constructor of {ORIGIN} carried no location text"
                )));
            }
        };
        match constructor.as_ref() {
            REGISTRY => Ok(Self::Registry(location)),
            FORGE => Ok(Self::Forge(location)),
            LOCAL_PATH => Ok(Self::LocalPath(location)),
            other => Err(diag(format!("{other} is not a constructor of {ORIGIN}"))),
        }
    }
}

impl Origin {
    /// Returns the origin kind used by the written lock.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Registry(_) => KIND_REGISTRY,
            Self::Forge(_) => KIND_FORGE,
            Self::LocalPath(_) => KIND_LOCAL_PATH,
        }
    }

    /// The origin a written kind names at `location`, the inverse of
    /// [`Self::kind`].
    #[must_use]
    pub fn from_kind(kind: &str, location: impl Into<Box<str>>) -> Option<Self> {
        match kind {
            KIND_REGISTRY => Some(Self::Registry(location.into())),
            KIND_FORGE => Some(Self::Forge(location.into())),
            KIND_LOCAL_PATH => Some(Self::LocalPath(location.into())),
            _ => None,
        }
    }

    /// Where the resolution happened, the payload every kind carries.
    #[must_use]
    pub fn location(&self) -> &str {
        match self {
            Self::Registry(location) | Self::Forge(location) | Self::LocalPath(location) => {
                location
            }
        }
    }
}

impl std::fmt::Display for Origin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registry(url) => write!(formatter, "registry {url}"),
            Self::Forge(url) => write!(formatter, "forge {url}"),
            Self::LocalPath(path) => write!(formatter, "local path {path}"),
        }
    }
}

/// The declared origin sum type: three constructors, each carrying its
/// location as text.
#[must_use]
pub fn origin_type() -> Type {
    let sum = Type::sum(
        ORIGIN,
        [
            SumConstructor {
                name: REGISTRY.into(),
                payload: Some(Type::Text),
            },
            SumConstructor {
                name: FORGE.into(),
                payload: Some(Type::Text),
            },
            SumConstructor {
                name: LOCAL_PATH.into(),
                payload: Some(Type::Text),
            },
        ],
    );
    match sum {
        Ok(sum) => sum,
        // Constructor names are distinct literals.
        Err(error) => unreachable!("{error}"),
    }
}

/// Returns the declared lock-entry record type.
#[must_use]
pub fn entry_type() -> Type {
    record_type([
        (FIELD_DOMAIN, Type::Text),
        (NAME, Type::Text),
        (FIELD_VERSION, Type::Text),
        (FIELD_FEATURES, Type::List(Box::new(Type::Text))),
        (SOURCE, Type::Blob),
        (ORIGIN_FIELD, origin_type()),
    ])
}

/// Package coordinates bound to source content.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Binding {
    pub package: PackageVersion,
    pub features: Box<[Box<str>]>,
    pub source: ContentId,
}

/// One lock entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockEntry {
    pub package: PackageVersion,
    /// Canonically sorted feature coordinates.
    pub features: Box<[Box<str>]>,
    pub source: ContentId,
    pub origin: Origin,
}

impl LockEntry {
    #[must_use]
    pub fn new(
        package: PackageVersion,
        features: impl IntoIterator<Item = impl Into<Box<str>>>,
        source: ContentId,
        origin: Origin,
    ) -> Self {
        let mut features: Vec<Box<str>> = features.into_iter().map(Into::into).collect();
        features.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        Self {
            package,
            features: features.into(),
            source,
            origin,
        }
    }

    /// The binding this entry records, without the origin evidence.
    #[must_use]
    pub fn binding(&self) -> Binding {
        Binding {
            package: self.package.clone(),
            features: self.features.clone(),
            source: self.source,
        }
    }

    /// The entry as a value of the declared record type.
    #[must_use]
    pub fn to_value(&self) -> Value {
        record_value([
            (
                FIELD_DOMAIN,
                Value::Text(self.package.identity().domain().as_str().into()),
            ),
            (NAME, Value::Text(self.package.identity().name().into())),
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
            (SOURCE, Value::Blob(self.source)),
            (ORIGIN_FIELD, self.origin.to_value()),
        ])
    }

    /// Decodes a lock entry from `value`.
    ///
    /// # Errors
    /// Returns a diagnostic when `value` is not a lock entry.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        if !value.is_type(&entry_type()) {
            return Err(diag(format!(
                "expected a value of the lock entry type, found {}",
                value.describe()
            )));
        }
        let Value::Record(fields) = value else {
            return Err(diag(format!(
                "expected a value of the lock entry type, found {}",
                value.describe()
            )));
        };
        let domain = text_field(fields, FIELD_DOMAIN)?;
        let name = text_field(fields, NAME)?;
        let version = text_field(fields, FIELD_VERSION)?;
        let features = match field_of(fields, FIELD_FEATURES) {
            Some(payload) => text_list(payload, FIELD_FEATURES)?,
            None => return Err(diag(format!("the record carried no {FIELD_FEATURES} set"))),
        };
        let source = blob_field(fields, SOURCE)?;
        let origin = match field_of(fields, ORIGIN_FIELD) {
            Some(payload) => Origin::from_value(payload)?,
            None => return Err(diag(format!("the record carried no {ORIGIN_FIELD}"))),
        };
        Ok(Self {
            package: PackageVersion::new(
                crate::identity::PackageIdentity::declare(
                    crate::identity::DomainIdentity::new(domain),
                    name,
                ),
                version,
            ),
            features: features.into(),
            source,
            origin,
        })
    }

    /// The entry's own content identity: a digest over its canonical
    /// encoding, identifying one revision of the entry. The digest moves
    /// when the origin evidence moves, which is the entry-as-record's honest
    /// behavior; the binding, which the entry records, does not.
    #[must_use]
    pub fn content_id(&self) -> ContentId {
        value_content_id(LOCK_ENTRY_DOMAIN, &self.to_value())
    }

    /// Checks freshly resolved content against the bound source identity.
    ///
    /// # Errors
    /// Returns a diagnostic when `resolved` differs from the bound source.
    pub fn verify_resolution(&self, resolved: ContentId) -> PithResult<()> {
        if resolved == self.source {
            return Ok(());
        }
        Err(diag(format!(
            "the lock binds `{}` in `{}` version {} to content `{}`, but resolution \
             from {} produced content `{}`: the domain's entry changed underneath \
             the coordinates, which is drift to report or a new resolution to \
             record, never the same resolution",
            self.package.identity().name(),
            self.package.identity().domain().as_str(),
            self.package.version(),
            self.source.digest(),
            self.origin,
            resolved.digest(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{DomainIdentity, PackageIdentity};

    fn package(version: &str) -> PackageVersion {
        PackageVersion::new(
            PackageIdentity::declare(DomainIdentity::new("pithpkgs"), "zlib"),
            version,
        )
    }

    fn source() -> ContentId {
        ContentId::of_blob(b"zlib-1.3.tar")
    }

    fn entry() -> LockEntry {
        LockEntry::new(
            package("1.3"),
            [] as [Box<str>; 0],
            source(),
            Origin::Registry("pkgs.pith-lang.org".into()),
        )
    }

    #[test]
    fn the_origin_is_evidence_not_identity() {
        let from_registry = LockEntry::new(
            package("1.3"),
            [] as [Box<str>; 0],
            source(),
            Origin::Registry("pkgs.pith-lang.org".into()),
        );
        let from_mirror = LockEntry::new(
            package("1.3"),
            [] as [Box<str>; 0],
            source(),
            Origin::Registry("mirror.example".into()),
        );
        assert_ne!(from_registry, from_mirror);
        assert_eq!(from_registry.binding(), from_mirror.binding());
    }

    #[test]
    fn a_version_bump_is_a_different_binding_over_one_package() {
        let before = LockEntry::new(
            package("1.3"),
            [] as [Box<str>; 0],
            source(),
            Origin::Registry("pkgs.pith-lang.org".into()),
        );
        let after = LockEntry::new(
            package("1.3.1"),
            [] as [Box<str>; 0],
            source(),
            Origin::Registry("pkgs.pith-lang.org".into()),
        );
        assert_ne!(before.binding(), after.binding());
        assert_eq!(
            before.binding().package.identity(),
            after.binding().package.identity()
        );
    }

    #[test]
    fn the_feature_set_is_a_sorted_coordinate_of_the_binding() {
        let plain = entry();
        let shared = LockEntry::new(
            package("1.3"),
            ["shared"],
            source(),
            Origin::Registry("pkgs.pith-lang.org".into()),
        );
        assert_ne!(plain.binding(), shared.binding());
        let reordered = LockEntry::new(
            package("1.3"),
            ["zlib", "shared"],
            source(),
            Origin::Registry("pkgs.pith-lang.org".into()),
        );
        let canonical = LockEntry::new(
            package("1.3"),
            ["shared", "zlib"],
            source(),
            Origin::Registry("pkgs.pith-lang.org".into()),
        );
        assert_eq!(reordered, canonical);
        assert_eq!(reordered.content_id(), canonical.content_id());
    }

    #[test]
    fn an_origins_written_kind_reads_back_through_from_kind() {
        let origins = [
            (Origin::Registry("pkgs.pith-lang.org".into()), KIND_REGISTRY),
            (Origin::Forge("git.pith-lang.org".into()), KIND_FORGE),
            (Origin::LocalPath("vendor/zlib".into()), KIND_LOCAL_PATH),
        ];
        for (origin, kind) in origins {
            assert_eq!(origin.kind(), kind);
            assert_eq!(
                Origin::from_kind(origin.kind(), origin.location()),
                Some(origin.clone())
            );
        }
        assert_eq!(Origin::from_kind("mirror", "somewhere"), None);
        assert_eq!(Origin::Registry("r".into()).location(), "r");
    }

    #[test]
    fn matching_content_verifies_and_drifting_content_is_reported() {
        let entry = entry();
        assert!(entry.verify_resolution(source()).is_ok());

        let drifted = ContentId::of_blob(b"zlib-1.3-republished");
        let error = entry
            .verify_resolution(drifted)
            .expect_err("different content under the same coordinates is drift");
        let message = error
            .iter()
            .next()
            .map(|diagnostic| diagnostic.message.0.to_string())
            .unwrap_or_default();
        assert!(
            message.contains("zlib") && message.contains("drift"),
            "the diagnostic should name the package and the drift: {message}"
        );
        assert!(
            message.contains(&source().digest().to_string())
                && message.contains(&drifted.digest().to_string()),
            "the diagnostic should carry both content identities: {message}"
        );
    }

    #[test]
    fn an_entry_round_trips_through_the_canonical_codec() {
        let value = entry().to_value();
        assert!(value.is_type(&entry_type()));
        let decoded = Value::decode_canonical(&value.encode_canonical()).unwrap();
        assert_eq!(decoded, value);
        assert_eq!(LockEntry::from_value(&decoded).unwrap(), entry());
    }

    #[test]
    fn an_entry_read_back_binds_what_was_written() {
        let written = entry();
        let decoded = Value::decode_canonical(&written.to_value().encode_canonical()).unwrap();
        let read_back = LockEntry::from_value(&decoded).unwrap();
        assert_eq!(read_back.binding(), written.binding());
    }

    #[test]
    fn an_origin_change_moves_the_entry_revision_not_the_binding() {
        let from_registry = entry();
        let from_forge = LockEntry::new(
            package("1.3"),
            [] as [Box<str>; 0],
            source(),
            Origin::Forge("git.pith-lang.org".into()),
        );
        assert_ne!(
            from_registry.content_id(),
            from_forge.content_id(),
            "the entry records its evidence, so a re-resolution through another \
             origin is a new entry revision"
        );
        assert_eq!(from_registry.binding(), from_forge.binding());
    }

    #[test]
    fn an_entry_of_another_domain_does_not_collide_with_this_one() {
        let foreign = LockEntry::new(
            PackageVersion::new(
                PackageIdentity::declare(DomainIdentity::new("deb"), "zlib"),
                "1.3",
            ),
            [] as [Box<str>; 0],
            source(),
            Origin::Registry("deb.debian.org".into()),
        );
        assert_ne!(entry().to_value(), foreign.to_value());
        assert_ne!(entry().content_id(), foreign.content_id());
    }
}
