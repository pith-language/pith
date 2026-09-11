//! Binary reuse as an admitted substitution: an offer admitted on every
//! clause replaces the build, a refused offer leaves the build running
//! with the refusal named, a served substitution leaves the rendered lock
//! byte-identical, and the engine's own reuse and a substitution are told
//! apart by what the engine recorded.

#[path = "support/engine_state.rs"]
mod engine_state_support;

use engine_state_support::SharedState;
use phloem::build::{PackageBuild, PackageBuildRule, SourceFile, SourceTree};
use phloem::constraint::{Bound, Constraint, Range, constraint_set_value};
use phloem::description::Description;
use phloem::identity::{DomainIdentity, NUMERIC_SEGMENTS, PackageIdentity, version_scheme_value};
use phloem::lock::Lock;
use phloem::lock::Origin;
use phloem::preference::{Preference, PreferenceList, preference_list_value};
use phloem::resolution::{Resolution, resolve_request};
use phloem::resolve::{ResolveSolver, Schemes};
use phloem::source::SourceBinding;
use phloem::substitution::{
    Admission, AdmittedOrigins, BinaryOffer, Clause, Refusal, Serving, serve, serving_request,
};
use phloem::universe::{Candidate, CandidateUniverse};
use pith_core::{Pure, PureComputationKey, Request, Rule, RuleIdentity, RuleRevision, Value};
use pith_diag::{PithResult, Span};
use pith_engine::{
    Engine, EngineStateReader, EvaluationSource, ExecutionPlatform, PureRule, PureRuleFrame,
    PureStep, Resumption,
};
use pith_ids::ContentId;
use pith_store::MemoryContentStore;

const BINARY: &[u8] = b"zlib-1.3.so";

/// One toolchain value shared by the admission test and the build requests,
/// so the fixture cannot spell them differently.
fn toolchain() -> Value {
    xylem::types::toolchain("/nix/store/cc")
}

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

/// A rule that completes immediately with fixture content, standing in for
/// xylem's compile and link entries. The tests read the engine's attempt
/// records, not the completed value.
struct FixtureEntry {
    output: Value,
}

impl PureRule for FixtureEntry {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(FixtureEntryFrame {
            output: Some(self.output.clone()),
        })
    }
}

struct FixtureEntryFrame {
    output: Option<Value>,
}

impl PureRuleFrame for FixtureEntryFrame {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        match self.output.take() {
            Some(output) => Ok(PureStep::Complete(output)),
            None => Err(failure("the fixture compile step ran twice")),
        }
    }
}

fn failure(message: &str) -> pith_diag::DiagnosticSink {
    let mut sink = pith_diag::DiagnosticSink::new();
    sink.push(pith_diag::Diag::new(
        pith_diag::Severity::Error,
        pith_diag::StableCode(0),
        Span::none(),
        message,
    ));
    sink
}

fn fixture_compile_rule() -> Rule<Pure> {
    fixture_rule("compile", xylem::types::compile_interface())
}

fn fixture_link_rule() -> Rule<Pure> {
    fixture_rule("link", xylem::types::link_interface())
}

fn fixture_rule(name: &str, interface: pith_core::Interface) -> Rule<Pure> {
    let identity = RuleIdentity::of_module_declaration("phloem-fixture", name);
    Rule::<Pure>::new(
        "phloem-fixture",
        RuleRevision::of_manifest(identity, b"phloem-fixture-v1"),
        name,
        interface,
        Span::none(),
    )
}

fn engine_with(state: &SharedState) -> Engine {
    let mut engine = Engine::with_state_store(MemoryContentStore::default(), state.clone());
    let solver = ResolveSolver::new(Schemes::standard());
    engine.register_rule(solver.rule(), solver);
    let compile = fixture_compile_rule();
    engine.register_rule(
        compile,
        FixtureEntry {
            output: xylem::types::object(ContentId::of_blob(b"zlib-fixture.o")),
        },
    );
    let link = fixture_link_rule();
    engine.register_rule(
        link,
        FixtureEntry {
            output: xylem::types::executable(ContentId::of_blob(b"zlib-fixture.exe")),
        },
    );
    engine.register_rule(PackageBuildRule::rule(), PackageBuildRule);
    engine
}

/// The resolve request one zlib candidate answers, so every offer below is
/// offered against a binding a real resolution bound.
fn zlib_request() -> Request<Pure> {
    let universe = CandidateUniverse::new(vec![Candidate {
        identity: identity("zlib"),
        version: "1.3".into(),
        features: Box::new([]),
        provenance: SourceBinding::Archive {
            archive: ContentId::of_blob(b"zlib-1.3.tar"),
        },
        origin: Origin::Registry("pkgs.pith-lang.org".into()),
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

/// The lock the request resolves to, with failures propagated: test bodies
/// may unwrap, helper bodies may not.
fn locked_zlib(engine: &mut Engine) -> PithResult<Lock> {
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
        toolchain(),
        ContentId::of_blob(bytes),
        Origin::Forge("builds.pith-lang.org".into()),
    )
}

fn admission_over<'a>(
    entry: &'a phloem::lock::LockEntry,
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

fn description() -> Description {
    Description {
        name: "zlib".into(),
        source: SourceBinding::Archive {
            archive: ContentId::of_blob(b"zlib-1.3.tar"),
        },
        build: PackageBuild {
            sources: Box::new(["zlib-1.3/zlib.c".into()]),
            includes: Box::new([]),
        },
    }
}

/// The tree the description's build runs over. The fixture compile stands
/// in for xylem's, so the file content is fixture bytes.
fn tree() -> SourceTree {
    SourceTree {
        files: Box::new([SourceFile {
            path: "zlib-1.3/zlib.c".into(),
            content: ContentId::of_blob(b"zlib.c"),
        }]),
    }
}

#[test]
fn an_admitted_binary_substitutes_for_the_build_of_a_locked_binding() {
    let mut engine = engine_with(&SharedState::default());
    let lock = locked_zlib(&mut engine).unwrap();
    let entry = lock.entries.first().unwrap();
    let before = phloem::lock::render(&lock);
    let (toolchain, platform) = (toolchain(), platform());
    let origins = origins();

    let admission = admission_over(entry, &platform, &origins, &toolchain);
    let realization = serve(&admission, Some((&offer_over(entry, BINARY), BINARY)));
    let Serving::Substituted(admitted) = &realization else {
        unreachable!("an offer passing every clause replaces the build: {realization:?}");
    };
    assert_eq!(admitted.built_from, entry.source);
    assert_eq!(admitted.toolchain, toolchain);
    assert_eq!(admitted.measured, ContentId::of_blob(BINARY));
    assert_eq!(
        admitted.authorized_by,
        Origin::Forge("builds.pith-lang.org".into())
    );

    // The substitution's whole effect on the graph: no build request, and
    // the lock, which records source only, is untouched.
    assert!(
        serving_request(
            &realization,
            toolchain.clone(),
            &tree(),
            &description().build
        )
        .is_none(),
        "the build the binary stands in for is not requested"
    );
    assert_eq!(
        phloem::lock::render(&lock),
        before,
        "a served substitution moves nothing in the written lock"
    );
}

#[test]
fn a_refused_offer_builds_in_the_binarys_place_with_the_clause_named() {
    let mut engine = engine_with(&SharedState::default());
    let lock = locked_zlib(&mut engine).unwrap();
    let entry = lock.entries.first().unwrap();
    let toolchain = toolchain();

    let tampered: &[u8] = b"zlib-1.3-tampered.so";
    let offer = offer_over(entry, BINARY);
    let (platform, origins) = (platform(), origins());
    let admission = admission_over(entry, &platform, &origins, &toolchain);
    let realization = serve(&admission, Some((&offer, tampered)));
    let Serving::Built { refused } = &realization else {
        unreachable!("bytes measuring other than the claim refuse: {realization:?}");
    };
    assert_eq!(
        refused,
        &Some(Refusal {
            clause: Clause::Content {
                claimed: ContentId::of_blob(BINARY),
                measured: ContentId::of_blob(tampered),
            }
        }),
        "the refusal names the clause and both digests"
    );
    assert!(
        serving_request(
            &realization,
            toolchain.clone(),
            &tree(),
            &description().build
        )
        .is_some(),
        "the build runs in the refused offer's place, as the one package-build request"
    );
}

#[test]
fn the_engines_reuse_and_a_substitution_are_different_answers() {
    // Engine reuse is the engine's own record of a computation it ran; a
    // substitution stands for a computation nobody ran here. The two are
    // told apart by what the engine recorded: the reused resolution has an
    // attempt under its key, the substituted build has none, and the
    // refused offer's build gains exactly that attempt.
    let state = SharedState::default();
    let mut engine = engine_with(&state);
    let toolchain = toolchain();
    let lock = locked_zlib(&mut engine).unwrap();
    let entry = lock.entries.first().unwrap();

    let again = engine.evaluate_pure(&zlib_request()).unwrap();
    assert_eq!(
        again.source,
        EvaluationSource::Reused,
        "the engine serves its own recorded attempt under its key"
    );

    // The package build this binding would issue, keyed under the rule
    // that serves it. Its toolchain is the same value the admission leg
    // read, which ties the leg to the build it guards.
    let package_rule = PackageBuildRule::rule();
    let would_run = serving_request(
        &Serving::Built { refused: None },
        toolchain.clone(),
        &tree(),
        &description().build,
    )
    .expect("a build is one package-build request");
    let package_key = PureComputationKey::new(&package_rule, &would_run);

    let (platform, origins) = (platform(), origins());
    let admission = admission_over(entry, &platform, &origins, &toolchain);
    let substituted = serve(&admission, Some((&offer_over(entry, BINARY), BINARY)));
    assert!(matches!(substituted, Serving::Substituted(_)));
    assert!(
        serving_request(
            &substituted,
            toolchain.clone(),
            &tree(),
            &description().build
        )
        .is_none(),
        "the substituted build issues no request"
    );
    assert!(
        state
            .read(|store| store.attempt_history(package_key))
            .unwrap()
            .is_empty(),
        "a served substitution publishes no attempt: the computation never ran"
    );

    let tampered: &[u8] = b"zlib-1.3-tampered.so";
    let refused = serve(&admission, Some((&offer_over(entry, BINARY), tampered)));
    let request = serving_request(&refused, toolchain.clone(), &tree(), &description().build)
        .expect("the refused offer's build is requested");
    let computed = engine.evaluate_pure(&request).unwrap();
    assert_eq!(
        computed.source,
        EvaluationSource::Computed,
        "the build the refusal left running computes"
    );
    assert_eq!(
        state
            .read(|store| store.attempt_history(package_key))
            .unwrap()
            .len(),
        1,
        "the running build is recorded as the attempt the substitution never was"
    );
}

#[test]
fn a_refused_offer_is_refused_again_because_nothing_remembers_it() {
    // No negative state: the same offer, refused once, is re-tested and
    // refused identically, and an origin the policy later admits is admitted
    // with nothing to clear.
    let mut engine = engine_with(&SharedState::default());
    let lock = locked_zlib(&mut engine).unwrap();
    let entry = lock.entries.first().unwrap();
    let toolchain = toolchain();
    let (platform, narrow) = (platform(), AdmittedOrigins(Box::new([])));
    let admission = admission_over(entry, &platform, &narrow, &toolchain);
    let first = serve(&admission, Some((&offer_over(entry, BINARY), BINARY)));
    let second = serve(&admission, Some((&offer_over(entry, BINARY), BINARY)));
    assert_eq!(first, second);
    assert!(matches!(
        first,
        Serving::Built {
            refused: Some(Refusal {
                clause: Clause::Unauthorized { .. }
            })
        }
    ));

    let origins = origins();
    let widened = admission_over(entry, &platform, &origins, &toolchain);
    assert!(matches!(
        serve(&widened, Some((&offer_over(entry, BINARY), BINARY))),
        Serving::Substituted(_)
    ));
}
