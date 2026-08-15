//! Binary reuse as an admitted substitution (decision 0042): a binary
//! offered for a binding the lock produced through a real resolution is
//! admitted when every clause holds and replaces the build, a refused offer
//! leaves the build running with the refusal named, the rendered lock is
//! byte-identical before and after a served substitution, and the engine's
//! own reuse — 0031 and 0033's path — and a substitution remain different
//! answers to different questions in one test.

use phloem::constraint::{Bound, Constraint, Range, constraint_set_value};
use phloem::description::Description;
use phloem::document::Lock;
use phloem::identity::{DomainIdentity, NUMERIC_SEGMENTS, PackageIdentity, version_scheme_value};
use phloem::lock::Origin;
use phloem::preference::{Preference, PreferenceList, preference_list_value};
use phloem::resolution::{Resolution, resolve_request};
use phloem::resolve::{ResolveSolver, Schemes};
use phloem::source::SourceBinding;
use phloem::substitution::{
    Admission, AdmittedOrigins, BinaryOffer, Realization, Refusal, realization_requests, realize,
};
use phloem::universe::{Candidate, CandidateUniverse};
use pith_engine::state::MemoryEngineStateStore;
use pith_engine::{Engine, EvaluationSource, ExecutionPlatform};
use pith_ids::ContentId;
use pith_store::MemoryContentStore;

const BINARY: &[u8] = b"zlib-1.3.so";
const TOOLCHAIN: &str = "gcc-13";

fn platform() -> ExecutionPlatform {
    ExecutionPlatform {
        operating_system: "linux".into(),
        architecture: "x86_64".into(),
    }
}

fn identity(name: &str) -> PackageIdentity {
    PackageIdentity::declare(DomainIdentity::new("pithpkgs"), name)
}

fn newest() -> PreferenceList {
    PreferenceList(Box::new([Preference::Newest]))
}

fn engine() -> Engine {
    let mut engine = Engine::with_state_store(
        MemoryContentStore::default(),
        MemoryEngineStateStore::default(),
    );
    let solver = ResolveSolver::new(Schemes::standard());
    engine.register_rule(solver.rule(), solver);
    engine
}

/// The resolve request one zlib candidate answers, so every offer below is
/// offered against a binding a real resolution bound.
fn zlib_request() -> pith_core::Request<pith_core::Pure> {
    let universe = CandidateUniverse::new(vec![Candidate {
        identity: identity("zlib"),
        version: "1.3".into(),
        features: Box::new([]),
        provenance: SourceBinding::Archive {
            archive: ContentId::of_blob(b"zlib-1.3.tar"),
        },
        requires: Box::new([]),
    }]);
    resolve_request(
        &version_scheme_value(NUMERIC_SEGMENTS),
        &constraint_set_value(&[Constraint {
            subject: identity("zlib"),
            range: Range::AtLeast(Bound::new("1.0", true)),
            features: Box::new([]),
            attribution: "root".into(),
        }]),
        &universe.to_value(),
        &preference_list_value(&newest()),
        100,
    )
}

/// The lock the request resolves to, with the failures propagated so each
/// test unwraps on its own terms, inside the code the crate's clippy
/// configuration allows to.
fn locked_zlib(engine: &mut Engine) -> pith_diag::PithResult<Lock> {
    let answer = engine.evaluate_pure(&zlib_request())?;
    Lock::from_resolution(
        NUMERIC_SEGMENTS,
        &newest(),
        &Resolution::from_value(&answer.value)?,
    )
}

fn origins() -> AdmittedOrigins {
    AdmittedOrigins(Box::new([Origin::Forge("builds.pith-lang.org".into())]))
}

fn offer_over(entry: &phloem::lock::LockEntry, bytes: &[u8]) -> BinaryOffer {
    BinaryOffer::new(
        entry.package.clone(),
        entry.features.clone(),
        entry.source,
        platform(),
        TOOLCHAIN,
        ContentId::of_blob(bytes),
        Origin::Forge("builds.pith-lang.org".into()),
    )
}

fn description() -> Description {
    Description {
        name: "zlib".into(),
        source: SourceBinding::Archive {
            archive: ContentId::of_blob(b"zlib-1.3.tar"),
        },
        inputs: Box::new([ContentId::of_blob(b"zlib.c")]),
        options: Box::new([]),
    }
}

#[test]
fn an_admitted_binary_substitutes_for_the_build_of_a_locked_binding() {
    let mut engine = engine();
    let lock = locked_zlib(&mut engine).unwrap();
    let entry = lock.entries.first().unwrap();
    let before = phloem::lockfile::render(&lock);

    let admission = Admission {
        entry,
        platform: &platform(),
        toolchain: TOOLCHAIN,
        origins: &origins(),
    };
    let realization = realize(&admission, Some((&offer_over(entry, BINARY), BINARY)));
    let Realization::Substituted(admitted) = &realization else {
        unreachable!("an offer passing every clause replaces the build: {realization:?}");
    };
    assert_eq!(admitted.built_from, entry.source);
    assert_eq!(admitted.measured, ContentId::of_blob(BINARY));
    assert_eq!(
        admitted.authorized_by,
        Origin::Forge("builds.pith-lang.org".into())
    );

    // The substitution's whole effect on the graph: the requests are simply
    // not made, and the lock — which records source only — is untouched.
    assert!(
        realization_requests(
            &realization,
            xylem::types::toolchain("/nix/store/cc"),
            &description()
        )
        .is_empty(),
        "the build the binary stands in for is not requested"
    );
    assert_eq!(
        phloem::lockfile::render(&lock),
        before,
        "a served substitution moves nothing in the written lock"
    );
}

#[test]
fn a_refused_offer_builds_in_the_binarys_place_with_the_clause_named() {
    let mut engine = engine();
    let lock = locked_zlib(&mut engine).unwrap();
    let entry = lock.entries.first().unwrap();

    let tampered: &[u8] = b"zlib-1.3-tampered.so";
    let offer = offer_over(entry, BINARY);
    let admission = Admission {
        entry,
        platform: &platform(),
        toolchain: TOOLCHAIN,
        origins: &origins(),
    };
    let realization = realize(&admission, Some((&offer, tampered)));
    let Realization::Built { refused } = &realization else {
        unreachable!("bytes measuring other than the claim refuse: {realization:?}");
    };
    assert_eq!(
        refused,
        &Some(Refusal::Content {
            claimed: ContentId::of_blob(BINARY),
            measured: ContentId::of_blob(tampered),
        }),
        "the refusal names the clause and both digests"
    );
    assert_eq!(
        realization_requests(
            &realization,
            xylem::types::toolchain("/nix/store/cc"),
            &description()
        )
        .len(),
        1,
        "the build runs in the refused offer's place, one request per input"
    );
}

#[test]
fn the_engines_reuse_and_a_substitution_are_different_answers() {
    // 0031 and 0033's reuse is the engine's word about a computation it ran,
    // served under a computation key; a substitution is phloem's word about a
    // computation nobody ran here. One test drives both and holds them apart.
    let mut engine = engine();
    let lock = locked_zlib(&mut engine).unwrap();
    let entry = lock.entries.first().unwrap();

    let again = engine.evaluate_pure(&zlib_request()).unwrap();
    assert_eq!(
        again.source,
        EvaluationSource::Reused,
        "the engine serves its own recorded attempt under its key"
    );

    let admission = Admission {
        entry,
        platform: &platform(),
        toolchain: TOOLCHAIN,
        origins: &origins(),
    };
    let realization = realize(&admission, Some((&offer_over(entry, BINARY), BINARY)));
    let Realization::Substituted(admitted) = &realization else {
        unreachable!("the fixture's offer passes every clause: {realization:?}");
    };
    // The substituted content was never computed by this engine: the only
    // computation the engine recorded is the resolution, and the binary's
    // content identity is not any resolved value.
    assert_ne!(
        ContentId::of_blob(BINARY),
        ContentId::of_blob(b"zlib-1.3.tar"),
        "the fixture's binary is not the bound source, so the two answers name different content"
    );
    assert_eq!(admitted.measured, ContentId::of_blob(BINARY));
}

#[test]
fn a_refused_offer_is_refused_again_because_nothing_remembers_it() {
    // No negative state: the same offer, refused once, is re-tested and
    // refused identically, and an origin the policy later admits is admitted
    // with nothing to clear.
    let mut engine = engine();
    let lock = locked_zlib(&mut engine).unwrap();
    let entry = lock.entries.first().unwrap();
    let narrow = AdmittedOrigins(Box::new([]));
    let admission = Admission {
        entry,
        platform: &platform(),
        toolchain: TOOLCHAIN,
        origins: &narrow,
    };
    let first = realize(&admission, Some((&offer_over(entry, BINARY), BINARY)));
    let second = realize(&admission, Some((&offer_over(entry, BINARY), BINARY)));
    assert_eq!(first, second);
    assert!(matches!(
        first,
        Realization::Built {
            refused: Some(Refusal::Unauthorized { .. })
        }
    ));

    let widened = Admission {
        entry,
        platform: &platform(),
        toolchain: TOOLCHAIN,
        origins: &origins(),
    };
    assert!(matches!(
        realize(&widened, Some((&offer_over(entry, BINARY), BINARY))),
        Realization::Substituted(_)
    ));
}
