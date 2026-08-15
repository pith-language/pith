//! The development environment as a value over the lock (decision 0043):
//! an environment declared as a value resolves through the engine and
//! locks through 0041's lock, materializing one declaration twice produces
//! the same rendered record and the same content identity, a changed
//! declaration moves the environment with the moved input named, the lock's
//! placement is derived from the declaration with one lock per
//! environment, resolving touches no path until the caller writes, and a
//! served substitution persists in the environment's record while the
//! rendered lock stays byte-identical.

use phloem::constraint::{Bound, Constraint, Range};
use phloem::document::LockChange;
use phloem::environment::{self, Environment, EnvironmentChange, EnvironmentDocument, Offer};
use phloem::identity::{DomainIdentity, NUMERIC_SEGMENTS, PackageIdentity};
use phloem::lock::Origin;
use phloem::preference::{Preference, PreferenceList};
use phloem::resolve::{ResolveSolver, Schemes};
use phloem::source::SourceBinding;
use phloem::substitution::{AdmittedOrigins, BinaryOffer};
use phloem::universe::{Candidate, CandidateUniverse};
use pith_engine::state::MemoryEngineStateStore;
use pith_engine::{Engine, ExecutionPlatform};
use pith_ids::ContentId;
use pith_store::MemoryContentStore;
use tempfile::TempDir;

const BINARY: &[u8] = b"zlib-1.3.so";

fn identity(name: &str) -> PackageIdentity {
    PackageIdentity::declare(DomainIdentity::new("pithpkgs"), name)
}

fn candidate(name: &str, version: &str, content: &[u8]) -> Candidate {
    Candidate {
        identity: identity(name),
        version: version.into(),
        features: Box::new([]),
        provenance: SourceBinding::Archive {
            archive: ContentId::of_blob(content),
        },
        origin: Origin::Registry("pkgs.pith-lang.org".into()),
        requires: Box::new([]),
    }
}

fn universe() -> CandidateUniverse {
    CandidateUniverse::new(vec![candidate("zlib", "1.3", b"zlib-1.3")])
}

fn platform() -> ExecutionPlatform {
    ExecutionPlatform {
        operating_system: "linux".into(),
        architecture: "x86_64".into(),
    }
}

fn toolchain() -> pith_core::Value {
    xylem::types::toolchain("/nix/store/cc")
}

fn origins() -> AdmittedOrigins {
    AdmittedOrigins(Box::new([Origin::Forge("builds.pith-lang.org".into())]))
}

fn declaration() -> Environment {
    Environment {
        name: environment::DEFAULT_ENVIRONMENT.into(),
        constraints: Box::new([Constraint {
            subject: identity("zlib"),
            range: Range::AtLeast(Bound::new("1.0", true)),
            features: Box::new([]),
            attribution: "root".into(),
        }]),
        platform: platform(),
        toolchain: toolchain(),
        origins: origins(),
    }
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

/// The lock a declaration resolves to, with the failure propagated so each
/// test unwraps on its own terms, inside the code the crate's clippy
/// configuration allows to.
fn resolve(
    declaration: &Environment,
    universe: &CandidateUniverse,
) -> pith_diag::PithResult<phloem::document::Lock> {
    let realized = EnvironmentDocument::resolve(
        declaration,
        &mut engine(),
        universe,
        NUMERIC_SEGMENTS,
        &PreferenceList(Box::new([Preference::Newest])),
        100,
        &[],
    )?;
    Ok(realized.document.lock)
}

fn offer_over(
    entry: &phloem::lock::LockEntry,
    bytes: &[u8],
    toolchain: &pith_core::Value,
) -> BinaryOffer {
    BinaryOffer::new(
        entry.package.clone(),
        entry.features.clone(),
        entry.source,
        platform(),
        toolchain.clone(),
        ContentId::of_blob(bytes),
        Origin::Forge("builds.pith-lang.org".into()),
    )
}

#[test]
fn the_offer_that_serves_is_a_function_of_the_offer_set_not_the_slice_order() {
    // Three offers claim zlib's identity: one for a version the lock does
    // not bind, and two that both admit, from two origins the policy
    // admits. The wrong version sits first in one slice and last in the
    // other; which offer serves must not move with the slice.
    let mut declared = declaration();
    declared.origins = AdmittedOrigins(Box::new([
        Origin::Forge("builds.pith-lang.org".into()),
        Origin::Registry("mirror.example".into()),
    ]));
    let locked = resolve(&declared, &universe()).unwrap();
    let entry = locked.entries.first().unwrap();

    let mut wrong_version = offer_over(entry, BINARY, &toolchain());
    wrong_version.package = phloem::identity::PackageVersion::new(identity("zlib"), "1.4");
    let forge = offer_over(entry, BINARY, &toolchain());
    let mut mirror = offer_over(entry, b"zlib-1.3-mirror.so", &toolchain());
    mirror.origin = Origin::Registry("mirror.example".into());

    let realize_with = |offers: &[Offer<'_>]| {
        EnvironmentDocument::resolve(
            &declared,
            &mut engine(),
            &universe(),
            NUMERIC_SEGMENTS,
            &PreferenceList(Box::new([Preference::Newest])),
            100,
            offers,
        )
        .unwrap()
        .document
    };
    let wrong_first = [
        Offer {
            offer: &wrong_version,
            bytes: BINARY,
        },
        Offer {
            offer: &forge,
            bytes: BINARY,
        },
        Offer {
            offer: &mirror,
            bytes: b"zlib-1.3-mirror.so",
        },
    ];
    let wrong_last = [
        Offer {
            offer: &mirror,
            bytes: b"zlib-1.3-mirror.so",
        },
        Offer {
            offer: &forge,
            bytes: BINARY,
        },
        Offer {
            offer: &wrong_version,
            bytes: BINARY,
        },
    ];
    let one = realize_with(&wrong_first);
    let two = realize_with(&wrong_last);
    assert_eq!(
        one.substitutions.len(),
        1,
        "the wrong-version offer does not swallow the entry it claims"
    );
    assert_eq!(
        two.substitutions.len(),
        1,
        "and it does not matter where in the slice it sits"
    );
    assert_eq!(
        one.substitutions.first().unwrap().measured,
        two.substitutions.first().unwrap().measured,
        "the same offer serves under either assembly order"
    );
}

#[test]
fn an_environment_declared_as_a_value_resolves_and_locks_through_the_engine() {
    let declared = declaration();
    assert_eq!(
        Environment::from_value(&declared.to_value()).unwrap(),
        declared
    );

    let document = EnvironmentDocument::resolve(
        &declared,
        &mut engine(),
        &universe(),
        NUMERIC_SEGMENTS,
        &PreferenceList(Box::new([Preference::Newest])),
        100,
        &[],
    )
    .unwrap()
    .document;
    assert_eq!(document.name, declared.name);
    assert_eq!(document.platform, declared.platform);
    assert_eq!(document.toolchain, declared.toolchain);
    // The lock is held unchanged: it is the same document the resolution
    // would have written, entries and all.
    assert_eq!(document.lock, resolve(&declared, &universe()).unwrap());
    let entry = document.lock.entries.first().unwrap();
    assert_eq!(entry.package.identity(), &identity("zlib"));
    assert_eq!(entry.package.version(), "1.3");
    assert_eq!(entry.source, ContentId::of_blob(b"zlib-1.3"));
    assert!(document.substitutions.is_empty());
}

#[test]
fn materializing_one_declaration_twice_produces_the_same_environment() {
    // The sameness is asserted over the rendered record and the content
    // identity — observable projections of the document — never over the
    // declaration both came from, which would hold under any
    // implementation.
    let first = EnvironmentDocument::resolve(
        &declaration(),
        &mut engine(),
        &universe(),
        NUMERIC_SEGMENTS,
        &PreferenceList(Box::new([Preference::Newest])),
        100,
        &[],
    )
    .unwrap()
    .document;
    let second = EnvironmentDocument::resolve(
        &declaration(),
        &mut engine(),
        &universe(),
        NUMERIC_SEGMENTS,
        &PreferenceList(Box::new([Preference::Newest])),
        100,
        &[],
    )
    .unwrap()
    .document;
    assert_eq!(environment::render(&first), environment::render(&second));
    assert_eq!(first.content_id(), second.content_id());
    assert_eq!(first.lock, second.lock);
}

#[test]
fn a_changed_declaration_moves_the_environment_and_the_difference_names_the_input() {
    let base = resolve(&declaration(), &universe()).unwrap();

    let moved_toolchain = {
        let mut moved = declaration();
        moved.toolchain = xylem::types::toolchain("/nix/store/clang-18");
        resolve(&moved, &universe()).unwrap()
    };
    let moved_toolchain_document = EnvironmentDocument {
        name: environment::DEFAULT_ENVIRONMENT.into(),
        lock: base.clone(),
        platform: platform(),
        toolchain: xylem::types::toolchain("/nix/store/clang-18"),
        substitutions: Box::new([]),
    };
    let base_document = EnvironmentDocument {
        name: environment::DEFAULT_ENVIRONMENT.into(),
        lock: base.clone(),
        platform: platform(),
        toolchain: toolchain(),
        substitutions: Box::new([]),
    };
    assert_eq!(
        environment::diff(&base_document, &moved_toolchain_document),
        Box::from([EnvironmentChange::Toolchain {
            from: toolchain(),
            to: xylem::types::toolchain("/nix/store/clang-18"),
        }]),
        "a moved toolchain is named by the environment diff"
    );
    assert_eq!(
        moved_toolchain, base,
        "the lock binds source only, so a moved toolchain does not move it"
    );
    let moved_toolchain_document_with_lock = EnvironmentDocument {
        name: environment::DEFAULT_ENVIRONMENT.into(),
        lock: moved_toolchain,
        platform: platform(),
        toolchain: xylem::types::toolchain("/nix/store/clang-18"),
        substitutions: Box::new([]),
    };
    assert_ne!(
        moved_toolchain_document_with_lock.content_id(),
        base_document.content_id(),
        "the moved toolchain moved the environment over the same lock"
    );

    let moved_platform = EnvironmentDocument {
        name: environment::DEFAULT_ENVIRONMENT.into(),
        lock: base.clone(),
        platform: ExecutionPlatform {
            operating_system: "darwin".into(),
            architecture: "aarch64".into(),
        },
        toolchain: toolchain(),
        substitutions: Box::new([]),
    };
    assert_eq!(
        environment::diff(&base_document, &moved_platform),
        Box::from([EnvironmentChange::Platform {
            from: platform(),
            to: ExecutionPlatform {
                operating_system: "darwin".into(),
                architecture: "aarch64".into(),
            },
        }])
    );

    // A moved universe moves the environment through the lock, and the
    // lock's own diff names it: two platforms share one selection, a moved
    // candidate does not.
    let moved_universe = CandidateUniverse::new(vec![candidate("zlib", "1.3.1", b"zlib-1.3.1")]);
    let moved = resolve(&declaration(), &moved_universe).unwrap();
    let changes = environment::diff(
        &base_document,
        &EnvironmentDocument {
            name: environment::DEFAULT_ENVIRONMENT.into(),
            lock: moved,
            platform: platform(),
            toolchain: toolchain(),
            substitutions: Box::new([]),
        },
    );
    assert!(
        changes.iter().any(|change| matches!(
            change,
            EnvironmentChange::Lock(LockChange::Universe(..))
        ) || matches!(
            change,
            EnvironmentChange::Lock(LockChange::Upgraded { .. })
        )),
        "the moved universe arrives as the lock diff's named change: {changes:?}"
    );
}

#[test]
fn one_lock_per_environment_and_the_placement_is_derived() {
    let scratch = TempDir::new().unwrap();
    let default = resolve(&declaration(), &universe()).unwrap();
    let mut cross = declaration();
    cross.name = "cross".into();
    let cross_lock = resolve(&cross, &universe()).unwrap();

    let default_path =
        environment::lock_path(scratch.path(), environment::DEFAULT_ENVIRONMENT).unwrap();
    let cross_path = environment::lock_path(scratch.path(), "cross").unwrap();
    assert_eq!(
        default_path,
        scratch.path().join("pith.lock"),
        "the default environment's lock is the repository's lock"
    );
    assert_eq!(cross_path, scratch.path().join("cross.pith.lock"));

    phloem::lockpublish::write(&default, &default_path).unwrap();
    phloem::lockpublish::write(&cross_lock, &cross_path).unwrap();
    assert_eq!(phloem::lockpublish::read(&default_path).unwrap(), default);
    assert_eq!(phloem::lockpublish::read(&cross_path).unwrap(), cross_lock);
    let written: Vec<std::ffi::OsString> = std::fs::read_dir(scratch.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(
        written,
        vec![
            std::ffi::OsString::from("cross.pith.lock"),
            std::ffi::OsString::from("pith.lock"),
        ],
        "two environments hold two locks"
    );
}

#[test]
fn resolving_an_environment_touches_no_path_until_the_caller_writes() {
    let scratch = TempDir::new().unwrap();
    let document = EnvironmentDocument::resolve(
        &declaration(),
        &mut engine(),
        &universe(),
        NUMERIC_SEGMENTS,
        &PreferenceList(Box::new([Preference::Newest])),
        100,
        &[],
    )
    .unwrap()
    .document;
    let rendered = environment::render(&document);
    assert!(
        std::fs::read_dir(scratch.path()).unwrap().next().is_none(),
        "computing the environment — resolving, locking, realizing, rendering — \
         wrote nothing"
    );
    let lock = environment::lock_path(scratch.path(), environment::DEFAULT_ENVIRONMENT).unwrap();
    let record =
        environment::record_path(scratch.path(), environment::DEFAULT_ENVIRONMENT).unwrap();
    phloem::lockpublish::write(&document.lock, &lock).unwrap();
    std::fs::write(&record, rendered).unwrap();
    assert!(
        lock.exists() && record.exists(),
        "the caller's writes created both"
    );
}

#[test]
fn a_served_substitution_persists_in_the_record_and_not_in_the_lock() {
    let built = EnvironmentDocument::resolve(
        &declaration(),
        &mut engine(),
        &universe(),
        NUMERIC_SEGMENTS,
        &PreferenceList(Box::new([Preference::Newest])),
        100,
        &[],
    )
    .unwrap()
    .document;
    let entry = built.lock.entries.first().unwrap();
    let offer = offer_over(entry, BINARY, &built.toolchain);
    let offers = [Offer {
        offer: &offer,
        bytes: BINARY,
    }];

    let substituted = EnvironmentDocument::resolve(
        &declaration(),
        &mut engine(),
        &universe(),
        NUMERIC_SEGMENTS,
        &PreferenceList(Box::new([Preference::Newest])),
        100,
        &offers,
    )
    .unwrap();
    assert!(
        substituted.refusals.is_empty(),
        "an offer that served leaves nothing to explain"
    );
    let substituted = substituted.document;
    assert_eq!(substituted.substitutions.len(), 1);
    let record = substituted.substitutions.first().unwrap();
    assert_eq!(record.built_from, entry.source);
    assert_eq!(record.measured, ContentId::of_blob(BINARY));
    let rendered = environment::render(&substituted);
    assert!(
        rendered.contains("substitute pithpkgs zlib 1.3"),
        "the record names the served substitution: {rendered}"
    );
    assert_eq!(
        phloem::lockfile::render(&substituted.lock),
        phloem::lockfile::render(&built.lock),
        "the lock records source only and does not move"
    );

    // A tampered offer serves nothing, and its refusal arrives beside the
    // document with both sides of the comparison. The build it leaves
    // running is 0042's business; the document is the one an absent offer
    // produces.
    let tampered: &[u8] = b"zlib-1.3-tampered.so";
    let refused = EnvironmentDocument::resolve(
        &declaration(),
        &mut engine(),
        &universe(),
        NUMERIC_SEGMENTS,
        &PreferenceList(Box::new([Preference::Newest])),
        100,
        &[Offer {
            offer: &offer,
            bytes: tampered,
        }],
    )
    .unwrap();
    assert!(
        refused.document.substitutions.is_empty(),
        "bytes measuring other than the claim serve nothing"
    );
    assert_eq!(refused.document.lock, built.lock);
    assert_eq!(
        refused.document.content_id(),
        built.content_id(),
        "a refusal returns beside the document and does not move its identity"
    );
    let explanation = refused.refusals.first().expect("the refusal arrives");
    assert_eq!(explanation.package, entry.package);
    let message = format!("{}", explanation.refusal);
    assert!(
        message.contains(&ContentId::of_blob(BINARY).digest().to_string())
            && message.contains(&ContentId::of_blob(tampered).digest().to_string()),
        "the refusal names the claimed and the measured content: {message}"
    );
}

#[test]
fn the_declaration_and_the_record_round_trip_through_their_values() {
    let declared = declaration();
    let decoded =
        pith_core::Value::decode_canonical(&declared.to_value().encode_canonical()).unwrap();
    assert_eq!(Environment::from_value(&decoded).unwrap(), declared);

    let mut document = EnvironmentDocument::resolve(
        &declared,
        &mut engine(),
        &universe(),
        NUMERIC_SEGMENTS,
        &PreferenceList(Box::new([Preference::Newest])),
        100,
        &[],
    )
    .unwrap()
    .document;
    let entry = document.lock.entries.first().unwrap();
    let offer = offer_over(entry, BINARY, &document.toolchain);
    document = EnvironmentDocument::resolve(
        &declared,
        &mut engine(),
        &universe(),
        NUMERIC_SEGMENTS,
        &PreferenceList(Box::new([Preference::Newest])),
        100,
        &[Offer {
            offer: &offer,
            bytes: BINARY,
        }],
    )
    .unwrap()
    .document;
    let value = document.to_value();
    assert!(value.is_type(&phloem::environment::environment_document_type()));
    let decoded = pith_core::Value::decode_canonical(&value.encode_canonical()).unwrap();
    assert_eq!(decoded, value);
    assert_eq!(EnvironmentDocument::from_value(&decoded).unwrap(), document);
}
