//! Lock entries: coordinates bound to content, origin as evidence (decision 0039).
//!
//! A lock entry is a pair plus evidence: the package version (what was
//! chosen), the content identity of the source it resolved to (what the
//! choice means), and the origin it was resolved from (where that
//! happened). The binding is the entry; the origin is not part of either
//! identity. The content side is intrinsic, in Software Heritage's sense —
//! computed from the bytes read, valid wherever those bytes are found
//! again — so content that no longer matches the binding is drift to
//! report, never an identity transition. What makes a binding trustworthy
//! is the artifacts-and-trust question 0039 leaves open; the entry here
//! records the binding and checks it, and witnesses nothing.
//!
//! An entry is a value — a record over the origin sum — because a lock is
//! data that crosses processes, and the record form is the other half of
//! 0039's evidence for landing `Record`: a lock line is named fields the
//! same way a description is.

use pith_core::{RecordField, SumConstructor, Type, Value};
use pith_diag::PithResult;
use pith_ids::{ContentDigest, ContentId};

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

const DOMAIN: &str = "domain";
const NAME: &str = "name";
const VERSION: &str = "version";
const SOURCE: &str = "source";
const ORIGIN_FIELD: &str = "origin";

/// Where a resolution happened: the remote or local source the package was
/// resolved from. Evidence for provenance, not part of the binding — the
/// same content reached through two origins is one resolution recorded
/// twice, not two resolutions.
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
        Value::Sum {
            type_name: ORIGIN.into(),
            constructor: constructor.into(),
            payload: Some(Box::new(Value::Text(location.clone()))),
        }
    }

    /// Read an origin from a value, accepting every declared sum of this
    /// name that contains the selected constructor with a text payload —
    /// `is_type`, not `value_type`, because a sum value cannot recover its
    /// sibling constructors (0026's asymmetry).
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming what was expected and what was
    /// found when the value is not an origin.
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
        // The constructor names are distinct literals, so the duplicate-name
        // rejection is unreachable by construction.
        Err(error) => unreachable!("{error}"),
    }
}

/// The declared lock-entry record type: the full coordinates — domain,
/// name, version — the bound content identity, and the origin evidence. The
/// domain is a field here and is not one on the description, because a lock
/// is read far from the declaration site that carries the domain as its own
/// fact.
#[must_use]
pub fn entry_type() -> Type {
    let record = Type::record([
        RecordField {
            name: DOMAIN.into(),
            payload: Type::Text,
        },
        RecordField {
            name: NAME.into(),
            payload: Type::Text,
        },
        RecordField {
            name: VERSION.into(),
            payload: Type::Text,
        },
        RecordField {
            name: SOURCE.into(),
            payload: Type::Blob,
        },
        RecordField {
            name: ORIGIN_FIELD.into(),
            payload: origin_type(),
        },
    ]);
    match record {
        Ok(record) => record,
        // The field names are distinct literals, so the duplicate-name
        // rejection is unreachable by construction.
        Err(error) => unreachable!("{error}"),
    }
}

/// What a lock entry pins: one package version, one content identity for the
/// source it resolved to. Equality on the binding is the statement "these
/// coordinates resolve to this content," with no origin in it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Binding {
    pub package: PackageVersion,
    pub source: ContentId,
}

/// One lock entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockEntry {
    pub package: PackageVersion,
    pub source: ContentId,
    pub origin: Origin,
}

impl LockEntry {
    #[must_use]
    pub fn new(package: PackageVersion, source: ContentId, origin: Origin) -> Self {
        Self {
            package,
            source,
            origin,
        }
    }

    /// The binding this entry records, without the origin evidence.
    #[must_use]
    pub fn binding(&self) -> Binding {
        Binding {
            package: self.package.clone(),
            source: self.source,
        }
    }

    /// The entry as a value of the declared record type.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let record = Value::record([
            RecordField {
                name: DOMAIN.into(),
                payload: Value::Text(self.package.identity().domain().as_str().into()),
            },
            RecordField {
                name: NAME.into(),
                payload: Value::Text(self.package.identity().name().into()),
            },
            RecordField {
                name: VERSION.into(),
                payload: Value::Text(self.package.version().into()),
            },
            RecordField {
                name: SOURCE.into(),
                payload: Value::Blob(self.source),
            },
            RecordField {
                name: ORIGIN_FIELD.into(),
                payload: self.origin.to_value(),
            },
        ]);
        match record {
            Ok(record) => record,
            // The field names are distinct literals, so the duplicate-name
            // rejection is unreachable by construction.
            Err(error) => unreachable!("{error}"),
        }
    }

    /// Read an entry from a value, checking inhabitation with `is_type`
    /// rather than comparing against `value_type` (0026's asymmetry).
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming the declared type and the
    /// value found when the value is not a lock entry.
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
        let mut domain = None;
        let mut name = None;
        let mut version = None;
        let mut source = None;
        let mut origin = None;
        for field in fields.iter() {
            match field.name.as_ref() {
                DOMAIN => domain = Some(&field.payload),
                NAME => name = Some(&field.payload),
                VERSION => version = Some(&field.payload),
                SOURCE => source = Some(&field.payload),
                ORIGIN_FIELD => origin = Some(&field.payload),
                _ => {}
            }
        }
        let text_of = |field: Option<&Value>, field_name: &str| -> PithResult<Box<str>> {
            match field {
                Some(Value::Text(text)) => Ok(text.clone()),
                _ => Err(diag(format!(
                    "the lock entry record carried no {field_name} text"
                ))),
            }
        };
        let domain = text_of(domain, DOMAIN)?;
        let name = text_of(name, NAME)?;
        let version = text_of(version, VERSION)?;
        let source = match source {
            Some(Value::Blob(id)) => *id,
            _ => return Err(diag(format!("the record carried no {SOURCE} blob"))),
        };
        let origin = match origin {
            Some(value) => Origin::from_value(value)?,
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
        let canonical = self.to_value().encode_canonical();
        let mut domain_prefixed = LOCK_ENTRY_DOMAIN.to_vec();
        domain_prefixed.extend_from_slice(&canonical);
        ContentId::from_digest(ContentDigest::of_bytes(&domain_prefixed))
    }

    /// Check freshly resolved content against the binding. Matching content
    /// confirms the entry; different content under the same coordinates is
    /// drift — the failure a lock binding exists to catch when a domain does
    /// not honor immutability — or a new resolution to record, and in both
    /// cases it is reported rather than absorbed (0039).
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming the coordinates and both
    /// content identities when `resolved` differs from the bound source.
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
            source(),
            Origin::Registry("pkgs.pith-lang.org".into()),
        )
    }

    #[test]
    fn the_origin_is_evidence_not_identity() {
        // The same coordinates resolving to the same content through two
        // origins is one binding recorded twice. An origin that entered the
        // binding would make "what was chosen and what it resolves to"
        // depend on where it was downloaded from.
        let from_registry = LockEntry::new(
            package("1.3"),
            source(),
            Origin::Registry("pkgs.pith-lang.org".into()),
        );
        let from_mirror = LockEntry::new(
            package("1.3"),
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
            source(),
            Origin::Registry("pkgs.pith-lang.org".into()),
        );
        let after = LockEntry::new(
            package("1.3.1"),
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
        // The lock crosses processes as its value, so the binding that comes
        // back from a decode is the claim that matters: same coordinates,
        // same content, whatever process reads the entry.
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
        // The domain is a field of the entry value, so two domains' entries
        // for a same-named package are distinct values and distinct digests.
        let foreign = LockEntry::new(
            PackageVersion::new(
                PackageIdentity::declare(DomainIdentity::new("deb"), "zlib"),
                "1.3",
            ),
            source(),
            Origin::Registry("deb.debian.org".into()),
        );
        assert_ne!(entry().to_value(), foreign.to_value());
        assert_ne!(entry().content_id(), foreign.content_id());
    }
}
