//! Binary reuse as an admitted substitution (decision 0042).
//!
//! A binary offered for coordinates a lock binds replaces the *build of a
//! realization*: the package-build request the description would have
//! issued, and the content it would have produced. It is not a cache hit.
//! 0031's index answers a request this engine made, under a key this
//! engine derived from it; nothing here has a key, because the computation
//! whose key it would be never ran on this machine. What replaces the key
//! is a test over evidence, and the evidence is what the test carries out
//! with it.
//!
//! Four legs — binding, environment, content, authorization — checked clause
//! by clause in a fixed order, and the first failing clause is the one a
//! refusal names. The coordinates and the feature
//! set come from the lock entry. The source content the publisher claims to
//! have built from comes from the offer and must equal the source the entry
//! binds, which is what makes the offer a claim about a binding rather than
//! about a name. The realization coordinates — the platform under the same
//! [`ExecutionPlatform`] 0031's admission test reads, and the toolchain as
//! the value the run's build requests carry — come from the run and are
//! compared against the offer's, because a binary built for another
//! environment is a different realization, not a substitution candidate for
//! this one. The binary's own content identity is measured from
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
    /// caller hands to [`serving_request`], so the leg cannot be spelled
    /// independently of the build it guards.
    pub toolchain: &'a Value,
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
        running: Value,
        offered: Value,
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
                "this run realizes under {} and the binary was built under {}",
                running.describe(),
                offered.describe(),
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

/// What served a locked package version's binding: an admitted binary in
/// place of the build, or the build itself.
///
/// The two constructors are the record 0039 asks for: a build from source and
/// a substituted binary are different provenance claims about one package
/// version, and the package's identity absorbs neither. This is also where a
/// reader tells the two reuse paths apart. 0031 and 0033's reuse is the
/// engine's word about a computation it ran, reported as an
/// `EvaluationSource`; a substitution is phloem's word about a computation
/// nobody ran here, and it never reaches the engine at all.
///
/// The name is 0045's correction of an earlier one: this value says *how a
/// binding was served*, not what a realization is. A realization is the
/// attempt the engine already holds — the artifact's content identity and
/// the computation that produced it — and nothing in this module
/// constructs one. How-it-was-obtained and what-it-is are different
/// facts, and a name shared by both blurs which one the type carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Serving {
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
    if admission.toolchain != &offer.toolchain {
        return Err(Refusal::Toolchain {
            running: admission.toolchain.clone(),
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
        toolchain: admission.toolchain.clone(),
        measured,
        authorized_by: authorized_by.clone(),
    })
}

/// What served the locked package version: an admitted binary, or the
/// build. An absent offer builds with nothing to report.
#[must_use]
pub fn serve(admission: &Admission<'_>, offer: Option<(&BinaryOffer, &[u8])>) -> Serving {
    match offer {
        None => Serving::Built { refused: None },
        Some((offer, bytes)) => match admit(admission, offer, bytes) {
            Ok(admitted) => Serving::Substituted(admitted),
            Err(refusal) => Serving::Built {
                refused: Some(refusal),
            },
        },
    }
}

/// The build request a serving leaves standing: none under a substitution,
/// the one package-build request under a build. This is the substitution's
/// whole observable effect on the graph. Nothing is diverted, redirected,
/// or served under another key; the request is simply not made, which is
/// what "in place of running the build" means.
#[must_use]
pub fn serving_request(
    serving: &Serving,
    toolchain: Value,
    tree: &crate::build::SourceTree,
    build: &crate::build::PackageBuild,
) -> Option<Request<Pure>> {
    match serving {
        Serving::Substituted(_) => None,
        Serving::Built { .. } => Some(crate::build::build_request(toolchain, tree, build)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{PackageBuild, SourceFile, SourceTree};
    use crate::description::Description;
    use crate::identity::{DomainIdentity, PackageIdentity};
    use crate::source::SourceBinding;

    const BINARY: &[u8] = b"zlib-1.3.so";

    /// The run's toolchain as one value, the same spelling the build requests
    /// carry, so the admission leg and the derived requests cannot drift.
    fn toolchain() -> Value {
        xylem::types::toolchain("/nix/store/gcc-13")
    }

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
            toolchain(),
            ContentId::of_blob(BINARY),
            builder(),
        )
    }

    fn description() -> Description {
        Description {
            name: "zlib".into(),
            source: SourceBinding::Archive { archive: source() },
            build: PackageBuild {
                sources: Box::new(["zlib-1.3/zlib.c".into(), "zlib-1.3/adler32.c".into()]),
            },
        }
    }

    /// The tree the description's build runs over, holding exactly the
    /// paths it prescribes.
    fn tree() -> SourceTree {
        SourceTree {
            files: Box::new([
                SourceFile {
                    path: "zlib-1.3/zlib.c".into(),
                    content: ContentId::of_blob(b"zlib.c"),
                },
                SourceFile {
                    path: "zlib-1.3/adler32.c".into(),
                    content: ContentId::of_blob(b"adler32.c"),
                },
            ]),
        }
    }

    fn admission<'a>(
        entry: &'a LockEntry,
        platform: &'a ExecutionPlatform,
        origins: &'a AdmittedOrigins,
        toolchain: &'a Value,
    ) -> Admission<'a> {
        Admission {
            entry,
            platform,
            toolchain,
            origins,
        }
    }

    #[test]
    fn an_admitted_binary_carries_every_input_the_test_consulted() {
        // The vacuity guard's positive half: each field of the outcome holds
        // the value the clause that produced it read, and the measured digest
        // is computed from the bytes rather than copied from the claim.
        let (entry, platform, origins) = (entry(), platform(), origins());
        let toolchain = toolchain();
        let admitted = admit(
            &admission(&entry, &platform, &origins, &toolchain),
            &offer(),
            BINARY,
        )
        .unwrap();
        assert_eq!(admitted.package, package());
        assert_eq!(admitted.features, Box::from([Box::from("shared")]));
        assert_eq!(admitted.built_from, source());
        assert_eq!(admitted.platform, platform);
        assert_eq!(admitted.toolchain, toolchain);
        assert_eq!(admitted.measured, ContentId::of_blob(BINARY));
        assert_eq!(admitted.authorized_by, builder());
    }

    #[test]
    fn perturbing_any_one_input_refuses_naming_that_clause() {
        // The vacuity guard's negative half. Every clause is load-bearing:
        // one input moved at a time, the rest held at their admitting values,
        // and the outcome is the refusal that names the moved one.
        let (entry, platform, origins) = (entry(), platform(), origins());
        let toolchain = toolchain();
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
        wrong_toolchain.toolchain = xylem::types::toolchain("/nix/store/clang-18");
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
                    running: toolchain.clone(),
                    offered: xylem::types::toolchain("/nix/store/clang-18"),
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
            let refusal = admit(
                &admission(&entry, &platform, &origins, &toolchain),
                &perturbed,
                bytes,
            )
            .expect_err("the perturbed input is refused");
            assert_eq!(refusal, expected, "the refusal names the moved input");
        }
    }

    #[test]
    fn a_substitution_issues_no_build_request_and_a_refusal_issues_the_build() {
        let (entry, platform, origins) = (entry(), platform(), origins());
        let toolchain = toolchain();
        let admission = admission(&entry, &platform, &origins, &toolchain);

        let substituted = serve(&admission, Some((&offer(), BINARY)));
        assert!(matches!(substituted, Serving::Substituted(_)));
        assert!(
            serving_request(
                &substituted,
                toolchain.clone(),
                &tree(),
                &description().build
            )
            .is_none(),
            "the build the binary stands in for is not requested"
        );

        let mut unknown = offer();
        unknown.origin = Origin::Registry("attacker.example".into());
        let refused = serve(&admission, Some((&unknown, BINARY)));
        let Serving::Built {
            refused: Some(refusal),
        } = &refused
        else {
            unreachable!("an unauthorized offer builds and records the refusal");
        };
        assert!(matches!(refusal, Refusal::Unauthorized { .. }));
        assert!(
            serving_request(&refused, toolchain.clone(), &tree(), &description().build).is_some(),
            "the build runs in the refused offer's place"
        );

        let no_offer = serve(&admission, None);
        assert_eq!(no_offer, Serving::Built { refused: None });
        assert!(
            serving_request(&no_offer, toolchain, &tree(), &description().build).is_some(),
            "the build with no offer to test runs the same way"
        );
    }

    #[test]
    fn the_test_is_total_so_a_refused_offer_is_refused_again_and_a_fixed_one_is_admitted() {
        // Nothing remembers a rejection: the second run reads the same
        // inputs and reaches the same answer, and an origin the policy
        // later admits is admitted with no state to clear.
        let (entry, platform) = (entry(), platform());
        let toolchain = toolchain();
        let narrow = AdmittedOrigins(Box::new([]));
        let first = serve(
            &admission(&entry, &platform, &narrow, &toolchain),
            Some((&offer(), BINARY)),
        );
        let second = serve(
            &admission(&entry, &platform, &narrow, &toolchain),
            Some((&offer(), BINARY)),
        );
        assert_eq!(first, second);
        assert!(matches!(
            first,
            Serving::Built {
                refused: Some(Refusal::Unauthorized { .. })
            }
        ));

        let widened = origins();
        let third = serve(
            &admission(&entry, &platform, &widened, &toolchain),
            Some((&offer(), BINARY)),
        );
        assert!(matches!(third, Serving::Substituted(_)));
    }

    #[test]
    fn a_refusal_names_the_clause_and_both_sides_of_the_disagreement() {
        let (entry, platform, origins) = (entry(), platform(), origins());
        let toolchain = toolchain();
        let mut republished = offer();
        republished.built_from = ContentId::of_blob(b"zlib-1.3-republished.tar");
        let refusal = admit(
            &admission(&entry, &platform, &origins, &toolchain),
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
    fn an_offers_feature_set_is_canonically_sorted_the_way_an_entries_is() {
        // Two spellings of one feature set are one offer, on 0040's terms;
        // the admission test compares sets, so the spelling cannot decide it.
        let reordered = BinaryOffer::new(
            package(),
            ["zlib", "shared"],
            source(),
            platform(),
            toolchain(),
            ContentId::of_blob(BINARY),
            builder(),
        );
        let canonical = BinaryOffer::new(
            package(),
            ["shared", "zlib"],
            source(),
            platform(),
            toolchain(),
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
        let toolchain = toolchain();
        let admitted = admit(
            &admission(&entry, &platform, &origins, &toolchain),
            &offer(),
            BINARY,
        )
        .unwrap();
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
