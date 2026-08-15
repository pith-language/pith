//! Binary reuse as an admitted substitution (decision 0042): a binary
//! offered for a binding the lock produced through a real resolution is
//! admitted when every clause holds and replaces the build, a refused offer
//! leaves the build running with the refusal named, the rendered lock is
//! byte-identical before and after a served substitution, and the engine's
//! own reuse — 0031 and 0033's path — and a substitution remain different
//! answers to different questions, held apart on what the engine recorded
//! rather than on a difference of literals.

use std::sync::{Arc, Mutex};

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
use pith_core::{
    ActionComputationKey, Pure, PureComputationKey, Request, Rule, RuleIdentity, RuleRevision,
    Value,
};
use pith_diag::{PithResult, Span};
use pith_engine::state::{
    CompletedAttempt, DurableAttempt, DurableAttemptId, DurableComputation, EngineStateError,
    EngineStateStore, EngineStateVersions, InvalidationExplanation, MemoryEngineStateStore,
    StoppedAttempt,
};
use pith_engine::{
    Engine, EvaluationSource, ExecutionPlatform, PureRule, PureRuleFrame, PureStep, Resumption,
};
use pith_ids::ContentId;
use pith_store::MemoryContentStore;

const BINARY: &[u8] = b"zlib-1.3.so";

/// The run's toolchain as one value: the same value the admission test reads
/// and the build requests carry, so the fixture cannot spell the leg and the
/// build differently.
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

/// A compile rule that stands in for xylem's, so the build a refusal leaves
/// running can run through the engine and be recorded as an attempt. The
/// object it completes is fixture content; the tests below read the engine's
/// attempt records, not the object.
struct FixtureCompile {
    output: Value,
}

impl PureRule for FixtureCompile {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(FixtureCompileFrame {
            output: Some(self.output.clone()),
        })
    }
}

struct FixtureCompileFrame {
    output: Option<Value>,
}

impl PureRuleFrame for FixtureCompileFrame {
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
    let identity = RuleIdentity::of_module_declaration("phloem-fixture", "compile");
    Rule::<Pure>::new(
        RuleRevision::of_manifest(identity, b"phloem-fixture-compile-v1"),
        "compile",
        xylem::types::compile_interface(),
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
        FixtureCompile {
            output: xylem::types::object(ContentId::of_blob(b"zlib-fixture.o")),
        },
    );
    engine
}

/// One durable substrate behind the engine, the arrangement `written_lock`
/// tests through: the attempt history a substitution is held against is the
/// engine's own record, read directly rather than inferred.
#[derive(Clone, Default)]
struct SharedState(Arc<Mutex<MemoryEngineStateStore>>);

impl SharedState {
    fn read<T>(
        &self,
        read: impl FnOnce(&MemoryEngineStateStore) -> Result<T, EngineStateError>,
    ) -> Result<T, EngineStateError> {
        match self.0.lock() {
            Ok(store) => read(&store),
            Err(_) => Err(lock_poisoned()),
        }
    }

    fn write<T>(
        &self,
        write: impl FnOnce(&mut MemoryEngineStateStore) -> Result<T, EngineStateError>,
    ) -> Result<T, EngineStateError> {
        match self.0.lock() {
            Ok(mut store) => write(&mut store),
            Err(_) => Err(lock_poisoned()),
        }
    }
}

fn lock_poisoned() -> EngineStateError {
    EngineStateError::Adapter {
        message: "fixture: shared engine state lock was poisoned".into(),
    }
}

impl EngineStateStore for SharedState {
    fn versions(&self) -> EngineStateVersions {
        match self.0.lock() {
            Ok(store) => store.versions(),
            Err(_) => unreachable!("the shared engine state lock was poisoned"),
        }
    }

    fn create_pending_attempt(
        &self,
        computation: DurableComputation,
    ) -> Result<DurableAttemptId, EngineStateError> {
        self.write(|store| store.create_pending_attempt(computation))
    }

    fn publish_complete(
        &self,
        attempt: DurableAttemptId,
        completion: CompletedAttempt,
    ) -> Result<(), EngineStateError> {
        self.write(|store| store.publish_complete(attempt, completion))
    }

    fn publish_failed(
        &self,
        attempt: DurableAttemptId,
        failure: StoppedAttempt,
    ) -> Result<(), EngineStateError> {
        self.write(|store| store.publish_failed(attempt, failure))
    }

    fn publish_cancelled(
        &self,
        attempt: DurableAttemptId,
        cancellation: StoppedAttempt,
    ) -> Result<(), EngineStateError> {
        self.write(|store| store.publish_cancelled(attempt, cancellation))
    }

    fn attempt(
        &self,
        attempt: DurableAttemptId,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.read(|store| store.attempt(attempt))
    }

    fn attempt_history(
        &self,
        computation: PureComputationKey,
    ) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.read(|store| store.attempt_history(computation))
    }

    fn latest_completed_reusable_attempt(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.read(|store| store.latest_completed_reusable_attempt(computation))
    }

    fn latest_completed_reusable_action_attempt(
        &self,
        computation: ActionComputationKey,
    ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
        self.read(|store| store.latest_completed_reusable_action_attempt(computation))
    }

    fn explain_invalidation(
        &self,
        computation: PureComputationKey,
    ) -> Result<Option<InvalidationExplanation>, EngineStateError> {
        self.read(|store| store.explain_invalidation(computation))
    }

    fn pending_attempts(&self) -> Result<Box<[Arc<DurableAttempt>]>, EngineStateError> {
        self.read(|store| store.pending_attempts())
    }
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

/// The lock the request resolves to, with the failures propagated so each
/// test unwraps on its own terms, inside the code the crate's clippy
/// configuration allows to.
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
        inputs: Box::new([ContentId::of_blob(b"zlib.c")]),
        options: Box::new([]),
    }
}

#[test]
fn an_admitted_binary_substitutes_for_the_build_of_a_locked_binding() {
    let mut engine = engine_with(&SharedState::default());
    let lock = locked_zlib(&mut engine).unwrap();
    let entry = lock.entries.first().unwrap();
    let before = phloem::lockfile::render(&lock);
    let (toolchain, platform) = (toolchain(), platform());
    let origins = origins();

    let admission = admission_over(entry, &platform, &origins, &toolchain);
    let realization = realize(&admission, Some((&offer_over(entry, BINARY), BINARY)));
    let Realization::Substituted(admitted) = &realization else {
        unreachable!("an offer passing every clause replaces the build: {realization:?}");
    };
    assert_eq!(admitted.built_from, entry.source);
    assert_eq!(admitted.toolchain, toolchain);
    assert_eq!(admitted.measured, ContentId::of_blob(BINARY));
    assert_eq!(
        admitted.authorized_by,
        Origin::Forge("builds.pith-lang.org".into())
    );

    // The substitution's whole effect on the graph: the requests are simply
    // not made, and the lock — which records source only — is untouched.
    assert!(
        realization_requests(&realization, toolchain.clone(), &description()).is_empty(),
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
    let mut engine = engine_with(&SharedState::default());
    let lock = locked_zlib(&mut engine).unwrap();
    let entry = lock.entries.first().unwrap();
    let toolchain = toolchain();

    let tampered: &[u8] = b"zlib-1.3-tampered.so";
    let offer = offer_over(entry, BINARY);
    let (platform, origins) = (platform(), origins());
    let admission = admission_over(entry, &platform, &origins, &toolchain);
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
        realization_requests(&realization, toolchain.clone(), &description()).len(),
        1,
        "the build runs in the refused offer's place, one request per input"
    );
}

#[test]
fn the_engines_reuse_and_a_substitution_are_different_answers() {
    // 0031 and 0033's reuse is the engine's word about a computation it ran,
    // served under a computation key; a substitution is phloem's word about a
    // computation nobody ran here. The difference is held on what the engine
    // recorded: the reused resolution has an attempt under its key, the
    // substituted build has none under the key the build's own request
    // derives, and the build a refusal leaves running gains exactly that
    // attempt.
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

    // The compile a build of this binding would issue, keyed under the rule
    // that serves it. The toolchain in the request is the same value the
    // admission leg read, which is what ties the leg to the build it guards.
    let compile_rule = fixture_compile_rule();
    let input = *description()
        .inputs
        .first()
        .expect("the fixture prescribes one input");
    let would_run = xylem::types::compile_request(toolchain.clone(), input);
    let compile_key = PureComputationKey::new(&compile_rule, &would_run);

    let (platform, origins) = (platform(), origins());
    let admission = admission_over(entry, &platform, &origins, &toolchain);
    let substituted = realize(&admission, Some((&offer_over(entry, BINARY), BINARY)));
    assert!(matches!(substituted, Realization::Substituted(_)));
    assert!(
        realization_requests(&substituted, toolchain.clone(), &description()).is_empty(),
        "the substituted build issues no request"
    );
    assert!(
        state
            .read(|store| store.attempt_history(compile_key))
            .unwrap()
            .is_empty(),
        "a served substitution publishes no attempt: the computation never ran"
    );

    let tampered: &[u8] = b"zlib-1.3-tampered.so";
    let refused = realize(&admission, Some((&offer_over(entry, BINARY), tampered)));
    let requests = realization_requests(&refused, toolchain.clone(), &description());
    assert_eq!(requests.len(), 1, "the refused offer's build is requested");
    let request = requests
        .first()
        .expect("the refused offer's build is one request");
    let computed = engine.evaluate_pure(request).unwrap();
    assert_eq!(
        computed.source,
        EvaluationSource::Computed,
        "the build the refusal left running computes"
    );
    assert_eq!(
        state
            .read(|store| store.attempt_history(compile_key))
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
    let first = realize(&admission, Some((&offer_over(entry, BINARY), BINARY)));
    let second = realize(&admission, Some((&offer_over(entry, BINARY), BINARY)));
    assert_eq!(first, second);
    assert!(matches!(
        first,
        Realization::Built {
            refused: Some(Refusal::Unauthorized { .. })
        }
    ));

    let origins = origins();
    let widened = admission_over(entry, &platform, &origins, &toolchain);
    assert!(matches!(
        realize(&widened, Some((&offer_over(entry, BINARY), BINARY))),
        Realization::Substituted(_)
    ));
}
