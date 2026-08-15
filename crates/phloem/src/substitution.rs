//! Binary reuse as an admitted substitution (decision 0042).
//!
//! A binary offered for coordinates a lock binds replaces the *derivation of
//! a realization*: the build requests a description would have issued, and
//! the content they would have produced. It is not a cache hit. 0031's index
//! answers a request this engine made, under a key this engine derived from
//! it; nothing here has a key, because the computation whose key it would be
//! never ran on this machine. What replaces the key is a test over evidence,
//! and the evidence is what the test carries out with it.
//!
//! Four legs — binding, environment, content, authorization — checked clause
//! by clause in a fixed order, and the first failing clause is the one a
//! refusal names. The coordinates and the feature
//! set come from the lock entry. The source content the publisher claims to
//! have built from comes from the offer and must equal the source the entry
//! binds, which is what makes the offer a claim about a binding rather than
//! about a name. The realization coordinates — the platform under the same
//! [`ExecutionPlatform`] 0031's admission test reads, and the toolchain — come
//! from the run and are compared against the offer's, because a binary built
//! for another environment is a different realization, not a substitution
//! candidate for this one. The binary's own content identity is measured from
//! the bytes read, never taken from the offer. And an authorization covers the
//! substitution: M-4 ships no keys, so it degrades to a local decision naming
//! the origins whose offers this run considers at all, on the position Nix
//! reaches with `require-sigs = false`.
//!
//! A refused offer is not a fault. The build runs, and the refusal travels
//! back as a value naming the clause that turned the offer down, so a caller
//! can report which input disagreed. Nothing remembers a rejection: every
//! input to the test is in the run or in the offer, so re-testing is total
//! and cheap, and a remembered refusal would be state neither a request nor
//! a revision names (0038).

use pith_core::{Pure, Request, Type, Value};
use pith_diag::PithResult;
use pith_engine::ExecutionPlatform;
use pith_ids::ContentId;

use crate::codec::{blob_field, field_of, record_type, record_value, text_field, text_list};
use crate::description::Description;
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
    /// The toolchain identity the binary claims to have been built under, in
    /// the spelling the run's build requests carry. Platform and toolchain
    /// are the request-input half of a realization's identity (0039), so an
    /// offer under another toolchain realizes something this run would not.
    pub toolchain: Box<str>,
    /// The digest the publisher claims for the binary's bytes, which the test
    /// measures rather than believes.
    pub claimed: ContentId,
    pub origin: Origin,
}

impl BinaryOffer {
    #[must_use]
    pub fn new(
        package: PackageVersion,
        features: impl IntoIterator<Item = impl Into<Box<str>>>,
        built_from: ContentId,
        platform: ExecutionPlatform,
        toolchain: impl Into<Box<str>>,
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
            toolchain: toolchain.into(),
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
    pub toolchain: &'a str,
    pub origins: &'a AdmittedOrigins,
}

/// Every input the admission test consulted, carried out of it.
///
/// A substitution rests on exactly these values, so a caller reporting one
/// reports what it rested on, and a test perturbing any one of them sees the
/// outcome become the [`Refusal`] that names it. This is also the provenance
/// record a served substitution is: a value over the fields below, on the
/// terms the lock entry and the description are values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Admitted {
    pub package: PackageVersion,
    pub features: Box<[Box<str>]>,
    /// The source the lock bound, which the offer's claim matched.
    pub built_from: ContentId,
    pub platform: ExecutionPlatform,
    pub toolchain: Box<str>,
    /// The digest computed from the bytes read, not the one the offer
    /// claimed. 0014's distinction: this is the measurement.
    pub measured: ContentId,
    pub authorized_by: Origin,
}

/// The declared substitution record's name.
pub const SUBSTITUTION: &str = "phloem.Substitution";

const DOMAIN: &str = "domain";
const PACKAGE: &str = "package";
const VERSION: &str = "version";
const FEATURES: &str = "features";
const BUILT_FROM: &str = "built-from";
const OPERATING_SYSTEM: &str = "operating-system";
const ARCHITECTURE: &str = "architecture";
const TOOLCHAIN: &str = "toolchain";
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
        (DOMAIN, Type::Text),
        (PACKAGE, Type::Text),
        (VERSION, Type::Text),
        (FEATURES, Type::List(Box::new(Type::Text))),
        (BUILT_FROM, Type::Blob),
        (OPERATING_SYSTEM, Type::Text),
        (ARCHITECTURE, Type::Text),
        (TOOLCHAIN, Type::Text),
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
                DOMAIN,
                Value::Text(self.package.identity().domain().as_str().into()),
            ),
            (PACKAGE, Value::Text(self.package.identity().name().into())),
            (VERSION, Value::Text(self.package.version().into())),
            (
                FEATURES,
                Value::List(
                    self.features
                        .iter()
                        .map(|feature| Value::Text(feature.clone()))
                        .collect(),
                ),
            ),
            (BUILT_FROM, Value::Blob(self.built_from)),
            (
                OPERATING_SYSTEM,
                Value::Text(self.platform.operating_system.clone()),
            ),
            (
                ARCHITECTURE,
                Value::Text(self.platform.architecture.clone()),
            ),
            (TOOLCHAIN, Value::Text(self.toolchain.clone())),
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
        let domain = text_field(fields, DOMAIN)?;
        let package = text_field(fields, PACKAGE)?;
        let version = text_field(fields, VERSION)?;
        let features = match field_of(fields, FEATURES) {
            Some(payload) => text_list(payload, FEATURES)?,
            None => return Err(crate::diag(format!("the record carried no {FEATURES} set"))),
        };
        let built_from = blob_field(fields, BUILT_FROM)?;
        let operating_system = text_field(fields, OPERATING_SYSTEM)?;
        let architecture = text_field(fields, ARCHITECTURE)?;
        let toolchain = text_field(fields, TOOLCHAIN)?;
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

/// Which clause of the admission test turned an offer down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    Coordinates {
        bound: PackageVersion,
        offered: PackageVersion,
    },
    Features {
        bound: Box<[Box<str>]>,
        offered: Box<[Box<str>]>,
    },
    Source {
        bound: ContentId,
        offered: ContentId,
    },
    Platform {
        running: ExecutionPlatform,
        offered: ExecutionPlatform,
    },
    Toolchain {
        running: Box<str>,
        offered: Box<str>,
    },
    Content {
        claimed: ContentId,
        measured: ContentId,
    },
    Unauthorized {
        origin: Origin,
        admitted: Box<[Origin]>,
    },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coordinates { bound, offered } => write!(
                formatter,
                "the lock binds `{}` in `{}` version {}, and the binary is offered for \
                 `{}` in `{}` version {}",
                bound.identity().name(),
                bound.identity().domain().as_str(),
                bound.version(),
                offered.identity().name(),
                offered.identity().domain().as_str(),
                offered.version(),
            ),
            Self::Features { bound, offered } => write!(
                formatter,
                "the lock binds features [{}] and the binary was built with [{}]: \
                 features are coordinates, so these are two realizations",
                bound.join(", "),
                offered.join(", "),
            ),
            Self::Source { bound, offered } => write!(
                formatter,
                "the lock binds this version to source `{}`, and the binary claims to \
                 have been built from `{}`: the offer realizes another binding",
                bound.digest(),
                offered.digest(),
            ),
            Self::Platform { running, offered } => write!(
                formatter,
                "this run realizes on {}/{} and the binary was built for {}/{}",
                running.operating_system,
                running.architecture,
                offered.operating_system,
                offered.architecture,
            ),
            Self::Toolchain { running, offered } => write!(
                formatter,
                "this run realizes under `{running}` and the binary was built under `{offered}`"
            ),
            Self::Content { claimed, measured } => write!(
                formatter,
                "the binary claims content `{}` and its bytes measure `{}`",
                claimed.digest(),
                measured.digest(),
            ),
            Self::Unauthorized { origin, admitted } => write!(
                formatter,
                "no local policy admits substitutions from {origin}; this run admits {}",
                admitted_list(admitted),
            ),
        }
    }
}

fn admitted_list(admitted: &[Origin]) -> String {
    if admitted.is_empty() {
        return "no origin".into();
    }
    admitted
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// What served a locked package version's realization.
///
/// The two constructors are the record 0039 asks for: a build from source and
/// a substituted binary are different provenance claims about one package
/// version, and the package's identity absorbs neither. This is also where a
/// reader tells the two reuse paths apart. 0031 and 0033's reuse is the
/// engine's word about a computation it ran, reported as an
/// `EvaluationSource`; a substitution is phloem's word about a computation
/// nobody ran here, and it never reaches the engine at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Realization {
    /// An admitted binary stands in, and the build does not run.
    Substituted(Admitted),
    /// The build runs. `refused` is present when an offer was made and the
    /// admission test turned it down, which is a record rather than a fault.
    Built { refused: Option<Refusal> },
}

/// Apply the admission test to `offer`, whose bytes are `bytes`.
///
/// # Errors
/// The [`Refusal`] naming the first clause that disagreed. A refusal is a
/// miss, not a diagnostic: the caller builds instead.
pub fn admit(
    admission: &Admission<'_>,
    offer: &BinaryOffer,
    bytes: &[u8],
) -> Result<Admitted, Refusal> {
    let entry = admission.entry;
    if entry.package != offer.package {
        return Err(Refusal::Coordinates {
            bound: entry.package.clone(),
            offered: offer.package.clone(),
        });
    }
    if entry.features != offer.features {
        return Err(Refusal::Features {
            bound: entry.features.clone(),
            offered: offer.features.clone(),
        });
    }
    if entry.source != offer.built_from {
        return Err(Refusal::Source {
            bound: entry.source,
            offered: offer.built_from,
        });
    }
    if admission.platform != &offer.platform {
        return Err(Refusal::Platform {
            running: admission.platform.clone(),
            offered: offer.platform.clone(),
        });
    }
    if admission.toolchain != offer.toolchain.as_ref() {
        return Err(Refusal::Toolchain {
            running: admission.toolchain.into(),
            offered: offer.toolchain.clone(),
        });
    }
    let measured = ContentId::of_blob(bytes);
    if measured != offer.claimed {
        return Err(Refusal::Content {
            claimed: offer.claimed,
            measured,
        });
    }
    let Some(authorized_by) = admission.origins.covering(&offer.origin) else {
        return Err(Refusal::Unauthorized {
            origin: offer.origin.clone(),
            admitted: admission.origins.0.clone(),
        });
    };
    Ok(Admitted {
        package: entry.package.clone(),
        features: entry.features.clone(),
        built_from: entry.source,
        platform: offer.platform.clone(),
        toolchain: offer.toolchain.clone(),
        measured,
        authorized_by: authorized_by.clone(),
    })
}

/// What will realize the locked package version: an admitted binary, or the
/// build. An absent offer builds with nothing to report.
#[must_use]
pub fn realize(admission: &Admission<'_>, offer: Option<(&BinaryOffer, &[u8])>) -> Realization {
    match offer {
        None => Realization::Built { refused: None },
        Some((offer, bytes)) => match admit(admission, offer, bytes) {
            Ok(admitted) => Realization::Substituted(admitted),
            Err(refusal) => Realization::Built {
                refused: Some(refusal),
            },
        },
    }
}

/// The build requests a realization issues: none under a substitution, one
/// per prescribed input under a build.
///
/// This is the substitution's whole observable effect on the graph. Nothing
/// is diverted, redirected, or served under another key; the requests are
/// simply not made, which is what "in place of running the build" means.
#[must_use]
pub fn realization_requests(
    realization: &Realization,
    toolchain: Value,
    description: &Description,
) -> Box<[Request<Pure>]> {
    match realization {
        Realization::Substituted(_) => Box::new([]),
        Realization::Built { .. } => crate::request::compile_requests(toolchain, description),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{DomainIdentity, PackageIdentity};
    use crate::source::SourceBinding;

    const BINARY: &[u8] = b"zlib-1.3.so";
    const TOOLCHAIN: &str = "gcc-13";

    fn platform() -> ExecutionPlatform {
        ExecutionPlatform {
            operating_system: "linux".into(),
            architecture: "x86_64".into(),
        }
    }

    fn source() -> ContentId {
        ContentId::of_blob(b"zlib-1.3.tar")
    }

    fn package() -> PackageVersion {
        PackageVersion::new(
            PackageIdentity::declare(DomainIdentity::new("pithpkgs"), "zlib"),
            "1.3",
        )
    }

    fn entry() -> LockEntry {
        LockEntry::new(
            package(),
            ["shared"],
            source(),
            Origin::Registry("pkgs.pith-lang.org".into()),
        )
    }

    fn builder() -> Origin {
        Origin::Forge("builds.pith-lang.org".into())
    }

    fn origins() -> AdmittedOrigins {
        AdmittedOrigins(Box::new([
            Origin::Registry("mirror.example".into()),
            builder(),
        ]))
    }

    fn offer() -> BinaryOffer {
        BinaryOffer::new(
            package(),
            ["shared"],
            source(),
            platform(),
            TOOLCHAIN,
            ContentId::of_blob(BINARY),
            builder(),
        )
    }

    fn description() -> Description {
        Description {
            name: "zlib".into(),
            source: SourceBinding::Archive { archive: source() },
            inputs: Box::new([
                ContentId::of_blob(b"zlib.c"),
                ContentId::of_blob(b"adler32.c"),
            ]),
            options: Box::new([]),
        }
    }

    fn admission<'a>(
        entry: &'a LockEntry,
        platform: &'a ExecutionPlatform,
        origins: &'a AdmittedOrigins,
    ) -> Admission<'a> {
        Admission {
            entry,
            platform,
            toolchain: TOOLCHAIN,
            origins,
        }
    }

    #[test]
    fn an_admitted_binary_carries_every_input_the_test_consulted() {
        // The vacuity guard's positive half: each field of the outcome holds
        // the value the clause that produced it read, and the measured digest
        // is computed from the bytes rather than copied from the claim.
        let (entry, platform, origins) = (entry(), platform(), origins());
        let admitted = admit(&admission(&entry, &platform, &origins), &offer(), BINARY).unwrap();
        assert_eq!(admitted.package, package());
        assert_eq!(admitted.features, Box::from([Box::from("shared")]));
        assert_eq!(admitted.built_from, source());
        assert_eq!(admitted.platform, platform);
        assert_eq!(admitted.toolchain, Box::from(TOOLCHAIN));
        assert_eq!(admitted.measured, ContentId::of_blob(BINARY));
        assert_eq!(admitted.authorized_by, builder());
    }

    #[test]
    fn perturbing_any_one_input_refuses_naming_that_clause() {
        // The vacuity guard's negative half. Every clause is load-bearing:
        // one input moved at a time, the rest held at their admitting values,
        // and the outcome is the refusal that names the moved one.
        let (entry, platform, origins) = (entry(), platform(), origins());
        let elsewhere = ExecutionPlatform {
            operating_system: "darwin".into(),
            architecture: "aarch64".into(),
        };

        let mut wrong_version = offer();
        wrong_version.package = PackageVersion::new(package().identity().clone(), "1.4");
        let mut wrong_features = offer();
        wrong_features.features = Box::new([]);
        let mut wrong_source = offer();
        wrong_source.built_from = ContentId::of_blob(b"zlib-1.3-republished.tar");
        let mut wrong_platform = offer();
        wrong_platform.platform = elsewhere.clone();
        let mut wrong_toolchain = offer();
        wrong_toolchain.toolchain = "clang-18".into();
        let mut unknown_origin = offer();
        unknown_origin.origin = Origin::Registry("attacker.example".into());

        let cases: [(BinaryOffer, &[u8], Refusal); 7] = [
            (
                wrong_version,
                BINARY,
                Refusal::Coordinates {
                    bound: package(),
                    offered: PackageVersion::new(package().identity().clone(), "1.4"),
                },
            ),
            (
                wrong_features,
                BINARY,
                Refusal::Features {
                    bound: Box::new([Box::from("shared")]),
                    offered: Box::new([]),
                },
            ),
            (
                wrong_source,
                BINARY,
                Refusal::Source {
                    bound: source(),
                    offered: ContentId::of_blob(b"zlib-1.3-republished.tar"),
                },
            ),
            (
                wrong_platform,
                BINARY,
                Refusal::Platform {
                    running: platform.clone(),
                    offered: elsewhere,
                },
            ),
            (
                wrong_toolchain,
                BINARY,
                Refusal::Toolchain {
                    running: TOOLCHAIN.into(),
                    offered: "clang-18".into(),
                },
            ),
            (
                offer(),
                b"zlib-1.3-tampered.so",
                Refusal::Content {
                    claimed: ContentId::of_blob(BINARY),
                    measured: ContentId::of_blob(b"zlib-1.3-tampered.so"),
                },
            ),
            (
                unknown_origin,
                BINARY,
                Refusal::Unauthorized {
                    origin: Origin::Registry("attacker.example".into()),
                    admitted: origins.0.clone(),
                },
            ),
        ];
        for (perturbed, bytes, expected) in cases {
            let refusal = admit(&admission(&entry, &platform, &origins), &perturbed, bytes)
                .expect_err("the perturbed input is refused");
            assert_eq!(refusal, expected, "the refusal names the moved input");
        }
    }

    #[test]
    fn a_substitution_issues_no_build_request_and_a_refusal_issues_them_all() {
        let (entry, platform, origins) = (entry(), platform(), origins());
        let admission = admission(&entry, &platform, &origins);
        let toolchain = xylem::types::toolchain("/nix/store/cc");

        let substituted = realize(&admission, Some((&offer(), BINARY)));
        assert!(matches!(substituted, Realization::Substituted(_)));
        assert!(
            realization_requests(&substituted, toolchain.clone(), &description()).is_empty(),
            "the build the binary stands in for is not requested"
        );

        let mut unknown = offer();
        unknown.origin = Origin::Registry("attacker.example".into());
        let refused = realize(&admission, Some((&unknown, BINARY)));
        let Realization::Built {
            refused: Some(refusal),
        } = &refused
        else {
            unreachable!("an unauthorized offer builds and records the refusal");
        };
        assert!(matches!(refusal, Refusal::Unauthorized { .. }));
        assert_eq!(
            realization_requests(&refused, toolchain.clone(), &description()).len(),
            description().inputs.len(),
            "the build runs in the refused offer's place"
        );

        let no_offer = realize(&admission, None);
        assert_eq!(no_offer, Realization::Built { refused: None });
        assert_eq!(
            realization_requests(&no_offer, toolchain, &description()).len(),
            description().inputs.len()
        );
    }

    #[test]
    fn the_test_is_total_so_a_refused_offer_is_refused_again_and_a_fixed_one_is_admitted() {
        // Nothing remembers a rejection: the second run reads the same
        // inputs and reaches the same answer, and an origin the policy
        // later admits is admitted with no state to clear.
        let (entry, platform) = (entry(), platform());
        let narrow = AdmittedOrigins(Box::new([]));
        let first = realize(
            &admission(&entry, &platform, &narrow),
            Some((&offer(), BINARY)),
        );
        let second = realize(
            &admission(&entry, &platform, &narrow),
            Some((&offer(), BINARY)),
        );
        assert_eq!(first, second);
        assert!(matches!(
            first,
            Realization::Built {
                refused: Some(Refusal::Unauthorized { .. })
            }
        ));

        let widened = origins();
        let third = realize(
            &admission(&entry, &platform, &widened),
            Some((&offer(), BINARY)),
        );
        assert!(matches!(third, Realization::Substituted(_)));
    }

    #[test]
    fn a_refusal_names_the_clause_and_both_sides_of_the_disagreement() {
        let (entry, platform, origins) = (entry(), platform(), origins());
        let mut republished = offer();
        republished.built_from = ContentId::of_blob(b"zlib-1.3-republished.tar");
        let refusal = admit(
            &admission(&entry, &platform, &origins),
            &republished,
            BINARY,
        )
        .expect_err("an offer built from other source realizes another binding");
        let message = refusal.to_string();
        assert!(
            message.contains(&source().digest().to_string())
                && message.contains(
                    &ContentId::of_blob(b"zlib-1.3-republished.tar")
                        .digest()
                        .to_string()
                ),
            "the refusal carries both source identities: {message}"
        );
    }

    #[test]
    fn an_offers_feature_set_is_canonically_sorted_the_way_an_entrys_is() {
        // Two spellings of one feature set are one offer, on 0040's terms;
        // the admission test compares sets, so the spelling cannot decide it.
        let reordered = BinaryOffer::new(
            package(),
            ["zlib", "shared"],
            source(),
            platform(),
            TOOLCHAIN,
            ContentId::of_blob(BINARY),
            builder(),
        );
        let canonical = BinaryOffer::new(
            package(),
            ["shared", "zlib"],
            source(),
            platform(),
            TOOLCHAIN,
            ContentId::of_blob(BINARY),
            builder(),
        );
        assert_eq!(reordered, canonical);
    }

    #[test]
    fn a_substitution_record_round_trips_through_its_value() {
        // The provenance claim crosses processes as its value, the piece the
        // lock's refusal of binaries leaves unwitnessed.
        let (entry, platform, origins) = (entry(), platform(), origins());
        let admitted = admit(&admission(&entry, &platform, &origins), &offer(), BINARY).unwrap();
        let value = admitted.to_value();
        assert!(value.is_type(&substitution_type()));
        let decoded = Value::decode_canonical(&value.encode_canonical()).unwrap();
        assert_eq!(decoded, value);
        assert_eq!(Admitted::from_value(&decoded).unwrap(), admitted);
    }

    #[test]
    fn a_value_that_is_not_a_substitution_record_is_refused() {
        let wrong = Value::Text("not a substitution".into());
        assert!(!wrong.is_type(&substitution_type()));
        assert!(Admitted::from_value(&wrong).is_err());
    }
}
