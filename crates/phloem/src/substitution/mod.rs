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
//! [`pith_engine::ExecutionPlatform`] 0031's admission test reads, and the toolchain as
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

mod admission;
mod model;
mod serving;

pub use self::admission::{Refusal, admit};
pub use self::model::{
    Admission, Admitted, AdmittedOrigins, BinaryOffer, SUBSTITUTION, substitution_type,
};
pub use self::serving::{Serving, serve, serving_request};

#[cfg(test)]
mod tests {
    use pith_core::Value;
    use pith_engine::ExecutionPlatform;
    use pith_ids::ContentId;

    use super::*;
    use crate::build::{PackageBuild, SourceFile, SourceTree};
    use crate::description::Description;
    use crate::identity::{DomainIdentity, PackageIdentity, PackageVersion};
    use crate::lock::{LockEntry, Origin};
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
