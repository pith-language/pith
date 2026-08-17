//! The written lock as an artifact in the graph's terms (decision 0041):
//! a resolution produces a lock document carrying every recorded input, the
//! engine never writes the file, the written form round-trips, two
//! resolutions under the same candidate universe produce the same file
//! bytes, a changed input produces a different file whose diff names the
//! moved input, and a lock read back pins the same selection and reports
//! drift rather than absorbing it.

#[path = "support/engine_state.rs"]
mod engine_state_support;

use engine_state_support::SharedState;
use phloem::constraint::{Bound, Constraint, Range, constraint_set_value};
use phloem::document::{Lock, LockChange, diff};
use phloem::identity::{
    DEBIAN, DomainIdentity, NUMERIC_SEGMENTS, PackageIdentity, version_scheme_value,
};
use phloem::lock::Origin;
use phloem::lockfile;
use phloem::lockpublish;
use phloem::preference::{Preference, PreferenceList, preference_list_value};
use phloem::resolution::{Resolution, resolve_request};
use phloem::resolve::{ResolveSolver, Schemes};
use phloem::source::SourceBinding;
use phloem::universe::{Candidate, CandidateUniverse};
use pith_core::{Pure, Request, Value};
use pith_engine::{Engine, EvaluationSource};
use pith_ids::ContentId;
use pith_store::MemoryContentStore;
use tempfile::TempDir;

fn identity(name: &str) -> PackageIdentity {
    PackageIdentity::declare(DomainIdentity::new("pithpkgs"), name)
}

/// A candidate whose archive content stands in for what a registry served.
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

fn zlib_constraint() -> Constraint {
    Constraint {
        subject: identity("zlib"),
        range: Range::AtLeast(Bound::new("1.0", true)),
        features: Box::new([]),
        attribution: "root".into(),
    }
}

fn newest() -> PreferenceList {
    PreferenceList(Box::new([Preference::Newest]))
}

fn request_over(
    scheme: &str,
    constraints: &Value,
    universe: &Value,
    preferences: &Value,
    budget: u64,
) -> Request<Pure> {
    resolve_request(
        &version_scheme_value(scheme),
        constraints,
        universe,
        preferences,
        budget,
    )
}

fn engine_with(state: &SharedState) -> Engine {
    let mut engine = Engine::with_state_store(MemoryContentStore::default(), state.clone());
    let solver = ResolveSolver::new(Schemes::standard());
    engine.register_rule(solver.rule(), solver);
    engine
}

#[test]
fn a_resolution_produces_a_lock_carrying_every_recorded_input() {
    let mut engine = engine_with(&SharedState::default());
    let universe = CandidateUniverse::new(vec![
        candidate("zlib", "1.2", b"zlib-1.2"),
        candidate("zlib", "1.3", b"zlib-1.3"),
    ]);
    let request = request_over(
        NUMERIC_SEGMENTS,
        &constraint_set_value(&[zlib_constraint()]),
        &universe.to_value(),
        &preference_list_value(&newest()),
        100,
    );
    let answer = engine.evaluate_pure(&request).unwrap();
    let resolution = Resolution::from_value(&answer.value).unwrap();
    let lock = Lock::from_resolution(NUMERIC_SEGMENTS, &newest(), &resolution).unwrap();

    assert_eq!(lock.scheme, Box::from(NUMERIC_SEGMENTS));
    assert_eq!(lock.preferences, newest());
    assert_eq!(lock.universe, universe.content_id());
    assert_eq!(lock.resolver, phloem::resolve::resolver_revision_hex());
    assert_eq!(lock.entries.len(), 1, "one entry per selected subject");
    let entry = lock.entries.first().unwrap();
    assert_eq!(entry.package.version(), "1.3");
    assert_eq!(entry.source, ContentId::of_blob(b"zlib-1.3"));
}

#[test]
fn resolving_through_the_engine_writes_no_file() {
    // The effect boundary: the engine computes a value and touches no path.
    // Until the caller writes, the lock's file does not exist.
    let scratch = TempDir::new().unwrap();
    let path = scratch.path().join("pith.lock");
    let mut engine = engine_with(&SharedState::default());
    let universe = CandidateUniverse::new(vec![candidate("zlib", "1.3", b"zlib-1.3")]);
    let request = request_over(
        NUMERIC_SEGMENTS,
        &constraint_set_value(&[zlib_constraint()]),
        &universe.to_value(),
        &preference_list_value(&newest()),
        100,
    );
    let answer = engine.evaluate_pure(&request).unwrap();
    assert!(!path.exists(), "resolving wrote no file");
    let resolution = Resolution::from_value(&answer.value).unwrap();
    let lock = Lock::from_resolution(NUMERIC_SEGMENTS, &newest(), &resolution).unwrap();
    lockpublish::write(&lock, &path).unwrap();
    assert!(path.exists(), "the caller's write created the file");
}

#[test]
fn the_written_form_round_trips_through_a_real_file() {
    let scratch = TempDir::new().unwrap();
    let path = scratch.path().join("pith.lock");
    let mut engine = engine_with(&SharedState::default());
    let universe = CandidateUniverse::new(vec![candidate("zlib", "1.3", b"zlib-1.3")]);
    let request = request_over(
        NUMERIC_SEGMENTS,
        &constraint_set_value(&[zlib_constraint()]),
        &universe.to_value(),
        &preference_list_value(&newest()),
        100,
    );
    let answer = engine.evaluate_pure(&request).unwrap();
    let resolution = Resolution::from_value(&answer.value).unwrap();
    let written = Lock::from_resolution(NUMERIC_SEGMENTS, &newest(), &resolution).unwrap();

    lockpublish::write(&written, &path).unwrap();
    let read_back = lockpublish::read(&path).unwrap();
    assert_eq!(read_back, written);
    assert_eq!(read_back.content_id(), written.content_id());
    for (back, wrote) in read_back.entries.iter().zip(written.entries.iter()) {
        assert_eq!(back.binding(), wrote.binding());
    }
}

#[test]
fn two_resolutions_under_the_same_universe_write_identical_bytes() {
    let universe = CandidateUniverse::new(vec![
        candidate("zlib", "1.3", b"zlib-1.3"),
        candidate("openssl", "1.1.1", b"openssl-1.1.1"),
    ]);
    let constraints = constraint_set_value(&[zlib_constraint()]);
    let preferences = preference_list_value(&newest());

    let mut first = engine_with(&SharedState::default());
    let one = first
        .evaluate_pure(&request_over(
            NUMERIC_SEGMENTS,
            &constraints,
            &universe.to_value(),
            &preferences,
            100,
        ))
        .unwrap();
    let mut second = engine_with(&SharedState::default());
    let two = second
        .evaluate_pure(&request_over(
            NUMERIC_SEGMENTS,
            &constraints,
            &universe.to_value(),
            &preferences,
            100,
        ))
        .unwrap();

    let first_lock = Lock::from_resolution(
        NUMERIC_SEGMENTS,
        &newest(),
        &Resolution::from_value(&one.value).unwrap(),
    )
    .unwrap();
    let second_lock = Lock::from_resolution(
        NUMERIC_SEGMENTS,
        &newest(),
        &Resolution::from_value(&two.value).unwrap(),
    )
    .unwrap();
    assert_eq!(
        lockfile::render(&first_lock),
        lockfile::render(&second_lock)
    );
    assert_eq!(first_lock.content_id(), second_lock.content_id());

    // The same engine serving the same request from the reusable index
    // produces the same file again.
    let reused = first
        .evaluate_pure(&request_over(
            NUMERIC_SEGMENTS,
            &constraints,
            &universe.to_value(),
            &preferences,
            100,
        ))
        .unwrap();
    assert_eq!(reused.source, EvaluationSource::Reused);
    let reused_lock = Lock::from_resolution(
        NUMERIC_SEGMENTS,
        &newest(),
        &Resolution::from_value(&reused.value).unwrap(),
    )
    .unwrap();
    assert_eq!(
        lockfile::render(&reused_lock),
        lockfile::render(&first_lock)
    );
}

#[test]
fn a_changed_input_moves_the_file_and_the_diff_names_it() {
    let before_universe = CandidateUniverse::new(vec![candidate("zlib", "1.3", b"zlib-1.3")]);
    let after_universe = CandidateUniverse::new(vec![candidate("zlib", "1.3.1", b"zlib-1.3.1")]);
    let constraints = constraint_set_value(&[zlib_constraint()]);

    let mut engine = engine_with(&SharedState::default());
    let mut resolve_under = |universe: &CandidateUniverse, preferences: &Value, scheme: &str| {
        let answer = engine
            .evaluate_pure(&request_over(
                scheme,
                &constraints,
                &universe.to_value(),
                preferences,
                100,
            ))
            .unwrap();
        let list = phloem::preference::preference_list_from_value(preferences).unwrap();
        Lock::from_resolution(
            scheme,
            &list,
            &Resolution::from_value(&answer.value).unwrap(),
        )
        .unwrap()
    };

    let base = resolve_under(
        &before_universe,
        &preference_list_value(&newest()),
        NUMERIC_SEGMENTS,
    );

    let moved_universe = resolve_under(
        &after_universe,
        &preference_list_value(&newest()),
        NUMERIC_SEGMENTS,
    );
    assert_ne!(lockfile::render(&base), lockfile::render(&moved_universe));
    assert_eq!(
        diff(&base, &moved_universe)
            .changes
            .iter()
            .filter(|change| matches!(change, LockChange::Universe(..)))
            .count(),
        1,
        "the diff names the universe as the input that moved"
    );

    let moved_preferences = resolve_under(
        &before_universe,
        &preference_list_value(&PreferenceList(Box::new([Preference::Oldest]))),
        NUMERIC_SEGMENTS,
    );
    assert_eq!(
        diff(&base, &moved_preferences).changes,
        Box::from([LockChange::Preferences])
    );

    let moved_scheme = resolve_under(&before_universe, &preference_list_value(&newest()), DEBIAN);
    assert_eq!(
        diff(&base, &moved_scheme).changes,
        Box::from([LockChange::Scheme])
    );
}

#[test]
fn a_lock_read_back_pins_the_same_selection_and_reports_drift() {
    let universe = CandidateUniverse::new(vec![
        candidate("zlib", "1.2", b"zlib-1.2"),
        candidate("zlib", "1.3", b"zlib-1.3"),
    ]);
    let constraints = constraint_set_value(&[zlib_constraint()]);
    let preferences = preference_list_value(&newest());

    let mut engine = engine_with(&SharedState::default());
    let answer = engine
        .evaluate_pure(&request_over(
            NUMERIC_SEGMENTS,
            &constraints,
            &universe.to_value(),
            &preferences,
            100,
        ))
        .unwrap();
    let lock = Lock::from_resolution(
        NUMERIC_SEGMENTS,
        &newest(),
        &Resolution::from_value(&answer.value).unwrap(),
    )
    .unwrap();

    // The lock as input: its entries are the constraint set of an ordinary
    // resolution, and the pinned re-resolution reproduces the selection.
    let pinned = engine
        .evaluate_pure(&request_over(
            NUMERIC_SEGMENTS,
            &constraint_set_value(&lock.pins()),
            &universe.to_value(),
            &preferences,
            100,
        ))
        .unwrap();
    let Resolution::Solved { choice, .. } = Resolution::from_value(&pinned.value).unwrap() else {
        unreachable!("a pinned resolution under the same universe solves");
    };
    assert_eq!(choice.len(), lock.entries.len());
    for (chosen, entry) in choice.iter().zip(lock.entries.iter()) {
        assert_eq!(chosen.version.as_ref(), entry.package.version());
        assert!(
            entry
                .verify_resolution(ContentId::of_blob(b"zlib-1.3"))
                .is_ok(),
            "the selected content confirms every binding"
        );
    }

    // The same coordinates offered as different content is drift: reported
    // with both content identities, never absorbed.
    let republished =
        CandidateUniverse::new(vec![candidate("zlib", "1.3", b"zlib-1.3-republished")]);
    let drifted = engine
        .evaluate_pure(&request_over(
            NUMERIC_SEGMENTS,
            &constraint_set_value(&lock.pins()),
            &republished.to_value(),
            &preferences,
            100,
        ))
        .unwrap();
    let Resolution::Solved { choice, .. } = Resolution::from_value(&drifted.value).unwrap() else {
        unreachable!("the coordinates still resolve; the content moved");
    };
    let resolved = match &choice.first().unwrap().provenance {
        SourceBinding::Archive { archive } => *archive,
        SourceBinding::Path { content, .. } => *content,
        SourceBinding::Git { .. } | SourceBinding::GitTree { .. } => {
            unreachable!("the fixture binds archives")
        }
    };
    let error = lock
        .entries
        .first()
        .unwrap()
        .verify_resolution(resolved)
        .unwrap_err();
    let message = error
        .iter()
        .next()
        .map(|diagnostic| diagnostic.message.0.to_string())
        .unwrap_or_default();
    assert!(
        message.contains("drift")
            && message.contains(&ContentId::of_blob(b"zlib-1.3").digest().to_string())
            && message.contains(
                &ContentId::of_blob(b"zlib-1.3-republished")
                    .digest()
                    .to_string()
            ),
        "the diagnostic names the drift and both content identities: {message}"
    );
}

#[test]
fn the_value_spelling_survives_the_codec_boundary_unchanged() {
    // The lock crosses processes as its value too, and the digest taken over
    // canonical bytes is what "the same lock" means there (0041).
    let universe = CandidateUniverse::new(vec![candidate("zlib", "1.3", b"zlib-1.3")]);
    let mut engine = engine_with(&SharedState::default());
    let answer = engine
        .evaluate_pure(&request_over(
            NUMERIC_SEGMENTS,
            &constraint_set_value(&[zlib_constraint()]),
            &universe.to_value(),
            &preference_list_value(&newest()),
            100,
        ))
        .unwrap();
    let lock = Lock::from_resolution(
        NUMERIC_SEGMENTS,
        &newest(),
        &Resolution::from_value(&answer.value).unwrap(),
    )
    .unwrap();
    let decoded = pith_core::Value::decode_canonical(&lock.to_value().encode_canonical()).unwrap();
    let read_back = Lock::from_value(&decoded).unwrap();
    assert_eq!(read_back, lock);
    assert_eq!(read_back.content_id(), lock.content_id());
}

/// A malformed lock read back from its path is refused by a diagnostic that
/// carries the file itself: the source is the file at the path it was read
/// from, the span selects the offending field's written spelling, and a
/// miette render over that diagnostic names the file, the line, and the
/// column, so a reader renders position from structure rather than prose
/// (decision 0053).
#[test]
fn a_malformed_lock_read_back_carries_its_file_and_renders_its_position() {
    let scratch = TempDir::new().unwrap();
    let path = scratch.path().join("pith.lock");
    let universe = CandidateUniverse::new(vec![candidate("zlib", "1.3", b"zlib-1.3")]);
    let mut engine = engine_with(&SharedState::default());
    let answer = engine
        .evaluate_pure(&request_over(
            NUMERIC_SEGMENTS,
            &constraint_set_value(&[zlib_constraint()]),
            &universe.to_value(),
            &preference_list_value(&newest()),
            100,
        ))
        .unwrap();
    let lock = Lock::from_resolution(
        NUMERIC_SEGMENTS,
        &newest(),
        &Resolution::from_value(&answer.value).unwrap(),
    )
    .unwrap();
    lockpublish::write(&lock, &path).unwrap();
    let good = std::fs::read_to_string(&path).unwrap();
    let bad = good.replace(
        &format!("sha256:{}", ContentId::of_blob(b"zlib-1.3").digest()),
        "sha256:not-hex",
    );
    std::fs::write(&path, &bad).unwrap();

    let error = lockpublish::read(&path).unwrap_err();
    let diagnostic = error.iter().next().unwrap();
    let file = diagnostic
        .source
        .as_ref()
        .expect("the refusal carries the file it was refused in");
    assert_eq!(
        file.label.as_ref(),
        path.display().to_string(),
        "the source is named by the path the reader read"
    );
    assert_eq!(
        file.source_text()
            .get(diagnostic.span.start.0 as usize..diagnostic.span.end.0 as usize),
        Some("sha256:not-hex"),
        "the span selects the offending digest field, prefix included"
    );

    let (line_number, column) = {
        let (number, line) = bad
            .lines()
            .enumerate()
            .find(|(_, line)| line.contains("sha256:not-hex"))
            .map(|(index, line)| (index.saturating_add(1), line))
            .unwrap();
        let column = line
            .find("sha256:not-hex")
            .map(|at| line.get(..at).unwrap().chars().count().saturating_add(1))
            .unwrap();
        (number, column)
    };
    let handler = miette::GraphicalReportHandler::new();
    let report = miette::Report::new(diagnostic.clone());
    let mut rendered = String::new();
    handler
        .render_report(&mut rendered, report.as_ref())
        .unwrap();
    assert!(
        rendered.contains(&format!("pith.lock:{line_number}:{column}")),
        "the rendered header names the file, the line, and the column: {rendered}"
    );
    assert!(
        rendered.contains("sha256:not-hex"),
        "the rendered snippet quotes the offending field: {rendered}"
    );
}
