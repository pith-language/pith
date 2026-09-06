//! The module resolver domain, registered and driven through the same
//! engine API a peer domain uses.
//!
//! These tests hold slice 2's registration claim: resolution is a pure
//! rule — `modules.resolve` over four declared inputs — bound onto the
//! engine through its public registration call, so dependency selection is
//! a computation the engine can explain and reuse rather than a subroutine
//! beside it. The search performs no I/O; every input is a value, and the
//! answers are functions of those values alone.

use pith_engine::{Engine, MemoryEngineStateStore};
use pith_hir::{ManifestVersion, ModuleSubject, VersionBound, VersionRange};
use pith_loader::{
    ModuleCandidate, ModuleConstraint, ModuleRequirement, ModuleResolution, ModuleSelection,
    ModuleUniverse, Preference, RegisterModuleResolver, SolveRequest, resolve_request,
    resolver_rule, selections_from_value, solve,
};

fn diagnostics_to_error(diagnostics: pith_diag::DiagnosticSink) -> Box<dyn std::error::Error> {
    format!("the resolve request failed: {diagnostics:?}").into()
}
use pith_store::MemoryContentStore;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn engine() -> Engine {
    let mut engine = Engine::with_state_store(
        MemoryContentStore::default(),
        MemoryEngineStateStore::default(),
    );
    engine.register_module_resolver();
    engine
}

fn subject(spelling: &str) -> ModuleSubject {
    ModuleSubject::parse(spelling).unwrap_or_else(|error| unreachable!("{error}"))
}

fn version(spelling: &str) -> ManifestVersion {
    ManifestVersion::parse(spelling).unwrap_or_else(|error| unreachable!("{error}"))
}

fn exactly(spelling: &str) -> VersionRange {
    VersionRange::Exactly(version(spelling))
}

fn at_least(spelling: &str) -> VersionRange {
    VersionRange::AtLeast(VersionBound {
        version: version(spelling),
        inclusive: true,
    })
}

fn at_most(spelling: &str) -> VersionRange {
    VersionRange::AtMost(VersionBound {
        version: version(spelling),
        inclusive: false,
    })
}

fn requirement(spelling: &str, range: VersionRange) -> ModuleRequirement {
    ModuleRequirement {
        subject: subject(spelling),
        range,
    }
}

fn candidate(
    spelling: &str,
    version_spelling: &str,
    requires: &[ModuleRequirement],
) -> ModuleCandidate {
    ModuleCandidate {
        subject: subject(spelling),
        version: version(version_spelling),
        requires: requires.to_vec().into(),
        origin: format!("registry-fixture/{spelling}/{version_spelling}").into(),
    }
}

fn constraint(spelling: &str, range: VersionRange, attribution: &str) -> ModuleConstraint {
    ModuleConstraint {
        subject: subject(spelling),
        range,
        attribution: attribution.into(),
    }
}

/// The diamond fixture: two consumers bound to one shared subject through
/// overlapping ranges, with three shared releases available.
fn diamond() -> (Vec<ModuleConstraint>, ModuleUniverse) {
    (
        vec![
            constraint("example/app", exactly("1.0.0"), "root"),
            constraint("example/left", exactly("1.0.0"), "root"),
            constraint("example/right", exactly("1.0.0"), "root"),
        ],
        ModuleUniverse::new([
            candidate(
                "example/app",
                "1.0.0",
                &[requirement("example/shared", at_least("1.0.0"))],
            ),
            candidate(
                "example/left",
                "1.0.0",
                &[requirement("example/shared", at_most("2.0.0"))],
            ),
            candidate(
                "example/right",
                "1.0.0",
                &[requirement("example/shared", at_least("1.5.0"))],
            ),
            candidate("example/shared", "1.0.0", &[]),
            candidate("example/shared", "1.5.0", &[]),
            candidate("example/shared", "2.0.0", &[]),
        ]),
    )
}

/// The canonical version a selection records, for comparison.
fn chose(selection: &ModuleSelection) -> Box<str> {
    selection.version.canonical_spelling()
}

fn selected<'answer>(
    answer: &'answer ModuleResolution,
    spelling: &str,
) -> Option<&'answer ModuleSelection> {
    let ModuleResolution::Solved { selections, .. } = answer else {
        return None;
    };
    selections
        .iter()
        .find(|selection| selection.subject == subject(spelling))
}

#[test]
fn the_resolver_registers_through_the_engine_and_solves_a_diamond() -> TestResult {
    let mut engine = engine();
    let (constraints, universe) = diamond();
    let request = resolve_request(&constraints, &universe, Preference::Newest, 100);
    let evaluation = engine
        .evaluate_pure(&request)
        .map_err(diagnostics_to_error)?;
    let selections = selections_from_value(&evaluation.value).map_err(diagnostics_to_error)?;
    let of = |spelling: &str| {
        selections
            .iter()
            .find(|selection| selection.subject == subject(spelling))
            .map(chose)
    };
    assert_eq!(of("example/app").as_deref(), Some("1"));
    assert_eq!(of("example/left").as_deref(), Some("1"));
    assert_eq!(of("example/right").as_deref(), Some("1"));
    // The diamond agrees on one version: the newest the shared range
    // admits under both consumers' requirements.
    assert_eq!(of("example/shared").as_deref(), Some("1.5"));
    Ok(())
}

#[test]
fn a_case_requiring_backtracking_solves_by_revising_an_earlier_choice() {
    // The newest app release pins the shared dependency below what the
    // right consumer admits, so the search must revise the app choice it
    // already made.
    let universe = ModuleUniverse::new([
        candidate(
            "example/app",
            "1.1.0",
            &[requirement("example/shared", exactly("1.2.0"))],
        ),
        candidate(
            "example/app",
            "1.0.0",
            &[requirement("example/shared", at_least("1.0.0"))],
        ),
        candidate(
            "example/right",
            "1.0.0",
            &[requirement("example/shared", at_least("1.5.0"))],
        ),
        candidate("example/shared", "1.0.0", &[]),
        candidate("example/shared", "1.2.0", &[]),
        candidate("example/shared", "1.5.0", &[]),
    ]);
    let answer = solve(&SolveRequest {
        constraints: [
            constraint("example/app", at_least("1.0.0"), "root"),
            constraint("example/right", exactly("1.0.0"), "root"),
        ]
        .into(),
        universe,
        preference: Preference::Newest,
        budget: 100,
    });
    assert!(
        matches!(answer, ModuleResolution::Solved { .. }),
        "backtracking revises the app choice: {answer:?}"
    );
    assert_eq!(
        selected(&answer, "example/app").map(chose).as_deref(),
        Some("1")
    );
    assert_eq!(
        selected(&answer, "example/shared").map(chose).as_deref(),
        Some("1.5")
    );
}

#[test]
fn incompatible_ranges_report_the_chain_that_fails() {
    let (constraints, universe) = diamond();
    let unsatisfiable = ModuleUniverse::new(
        universe
            .candidates()
            .iter()
            .filter(|candidate| candidate.subject != subject("example/shared"))
            .cloned(),
    );
    let answer = solve(&SolveRequest {
        constraints: constraints.into(),
        universe: unsatisfiable,
        preference: Preference::Newest,
        budget: 100,
    });
    let ModuleResolution::Unsatisfiable { derivation } = answer else {
        unreachable!("a shared dependency with no release is unsatisfiable");
    };
    assert_eq!(derivation.subject, subject("example/shared"));
    assert!(
        !derivation.constraints.is_empty(),
        "the derivation names the requirements in force: {derivation:?}"
    );
}

#[test]
fn exhaustion_is_distinct_from_unsatisfiability() {
    let (constraints, universe) = diamond();
    let answer = solve(&SolveRequest {
        constraints: constraints.into(),
        universe,
        preference: Preference::Newest,
        budget: 2,
    });
    let ModuleResolution::BudgetExhausted { budget, .. } = answer else {
        unreachable!("a search limit is never evidence of unsatisfiability: {answer:?}")
    };
    assert_eq!(budget, 2);
}

#[test]
fn two_realizations_of_one_version_are_underdetermined_not_ranked() {
    let tied = ModuleUniverse::new([
        candidate("example/shared", "1.5.0", &[]),
        ModuleCandidate {
            subject: subject("example/shared"),
            version: version("1.5.0"),
            requires: Box::new([]),
            origin: "mirror-fixture/example/shared/1.5.0".into(),
        },
        candidate(
            "example/app",
            "1.0.0",
            &[requirement("example/shared", at_least("1.0.0"))],
        ),
    ]);
    let answer = solve(&SolveRequest {
        constraints: [constraint("example/app", exactly("1.0.0"), "root")].into(),
        universe: tied,
        preference: Preference::Newest,
        budget: 100,
    });
    let ModuleResolution::Underdetermined {
        subject: tied_subject,
        tied,
    } = answer
    else {
        unreachable!("a preference cannot rank two realizations of one version: {answer:?}")
    };
    assert_eq!(tied_subject, subject("example/shared"));
    assert_eq!(tied.len(), 2, "the refusal names both realizations");
}

#[test]
fn input_permutations_produce_one_answer_and_one_universe() {
    let (constraints, universe) = diamond();
    let mut reversed_constraints = constraints.clone();
    reversed_constraints.reverse();
    let mut reversed_universe = ModuleUniverse::new(universe.candidates().iter().rev().cloned());
    reversed_universe = ModuleUniverse::new(reversed_universe.candidates().iter().rev().cloned());
    assert_eq!(
        universe.content_id(),
        reversed_universe.content_id(),
        "the universe has one identity regardless of assembly order"
    );
    let one = solve(&SolveRequest {
        constraints: constraints.into(),
        universe: universe.clone(),
        preference: Preference::Newest,
        budget: 100,
    });
    let two = solve(&SolveRequest {
        constraints: reversed_constraints.into(),
        universe: reversed_universe,
        preference: Preference::Newest,
        budget: 100,
    });
    assert_eq!(one, two);
}

#[test]
fn the_preference_selects_the_end_of_the_ordering_it_names() {
    let universe = ModuleUniverse::new([
        candidate(
            "example/app",
            "1.0.0",
            &[requirement("example/shared", at_least("1.0.0"))],
        ),
        candidate("example/shared", "1.0.0", &[]),
        candidate("example/shared", "1.5.0", &[]),
        candidate("example/shared", "1.9.0", &[]),
    ]);
    let constraints = [constraint("example/app", exactly("1.0.0"), "root")];
    let newest = solve(&SolveRequest {
        constraints: constraints.clone().into(),
        universe: universe.clone(),
        preference: Preference::Newest,
        budget: 100,
    });
    let oldest = solve(&SolveRequest {
        constraints: constraints.into(),
        universe,
        preference: Preference::Oldest,
        budget: 100,
    });
    assert_eq!(
        selected(&newest, "example/shared").map(chose).as_deref(),
        Some("1.9")
    );
    assert_eq!(
        selected(&oldest, "example/shared").map(chose).as_deref(),
        Some("1")
    );
}

#[test]
fn an_unconstrained_subject_with_no_release_is_absent_not_failed() {
    let answer = solve(&SolveRequest {
        constraints: [constraint("example/app", exactly("1.0.0"), "root")].into(),
        universe: ModuleUniverse::new([candidate("example/app", "1.0.0", &[])]),
        preference: Preference::Newest,
        budget: 10,
    });
    assert!(matches!(answer, ModuleResolution::Solved { .. }));
}

#[test]
fn the_rule_is_modules_resolve_and_its_revision_is_derived() {
    let rule = resolver_rule();
    assert_eq!(
        (rule.coordinate.module.as_ref(), rule.label.as_ref()),
        ("modules", "resolve"),
    );
    // The revision derives from the interface rather than being authored,
    // so every registration of this rule keys the engine identically.
    assert_eq!(
        resolver_rule().revision.digest(),
        resolver_rule().revision.digest()
    );
}
