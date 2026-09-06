//! The bodies and the cutoff: when `bodies-of` recomputes, when it is
//! reused or hydrated, and what a dependent's key does and does not name.

use pith_core::Value;
use pith_engine::{EvaluationSource, MemoryEngineStateStore};

use super::{
    ALPHA, ALPHA_BODY_EDIT, ALPHA_LABEL_EDIT, ALPHA_REPRESENTATION_EDIT, BETA, BROKEN, Driver,
    bodies_of, diagnostics_of, field_of, frontend_import, frontend_imports, frontend_source,
    representation_of,
};
use pith_loader::interface_of_request;

/// The exact case 0063's unresolved section deferred to the notation: editing
/// a rule's body text moves `bodies-of` while the interface surface and its
/// ABI digest stay byte-identical, the `interface-of` computation itself is
/// reusable across the edit, and a dependent whose imports name that surface
/// reuses its attempt.
#[test]
fn a_body_edit_moves_bodies_and_leaves_the_interface_surface_byte_identical() {
    let mut driver = Driver::new(MemoryEngineStateStore::default());

    let alpha_interface = driver.publish_interface("alpha", ALPHA);
    let first = match bodies_of(
        &mut driver.engine,
        "alpha",
        &[("alpha.pi".into(), alpha_interface.source)],
        frontend_imports([]),
    ) {
        Ok(evaluation) => evaluation,
        Err(diagnostics) => unreachable!("alpha did not elaborate: {diagnostics:?}"),
    };
    assert_eq!(first.source, EvaluationSource::Computed);

    // The dependent evaluates against the original surface, so an attempt
    // exists for the edit to be measured against.
    let beta_blob = driver.publish(BETA.as_bytes());
    let before = match bodies_of(
        &mut driver.engine,
        "beta",
        &[("beta.pi".into(), beta_blob)],
        frontend_imports([frontend_import(alpha_interface)]),
    ) {
        Ok(evaluation) => evaluation,
        Err(_) => unreachable!("the reusable lookup re-ran and failed"),
    };
    assert_eq!(before.source, EvaluationSource::Computed);

    let edited = driver.publish_interface("alpha", ALPHA_BODY_EDIT);
    assert_eq!(
        edited.surface, alpha_interface.surface,
        "a body edit moved the interface surface"
    );
    assert_eq!(
        edited.abi, alpha_interface.abi,
        "a body edit moved the ABI digest"
    );

    let interface_of_edited = match driver.engine.evaluate_with_content(&interface_of_request(
        frontend_source("alpha", [("alpha.pi".into(), edited.source)]),
        frontend_imports([]),
    )) {
        Ok(evaluation) => evaluation,
        Err(diagnostics) => unreachable!("interface-of failed: {diagnostics:?}"),
    };
    assert_eq!(
        interface_of_edited.source,
        EvaluationSource::Reused,
        "a body edit moved the interface-of computation"
    );

    let edited_bodies = match bodies_of(
        &mut driver.engine,
        "alpha",
        &[("alpha.pi".into(), edited.source)],
        frontend_imports([]),
    ) {
        Ok(evaluation) => evaluation,
        Err(diagnostics) => unreachable!("edited alpha did not elaborate: {diagnostics:?}"),
    };
    assert_eq!(
        edited_bodies.source,
        EvaluationSource::Computed,
        "a body edit left bodies-of reusable"
    );

    // A dependent of alpha imports the surface, not the bodies: its key names
    // the surface identity, which the body edit did not move.
    let second = match bodies_of(
        &mut driver.engine,
        "beta",
        &[("beta.pi".into(), beta_blob)],
        frontend_imports([frontend_import(edited)]),
    ) {
        Ok(evaluation) => evaluation,
        Err(_) => unreachable!("the reusable lookup re-ran and failed"),
    };
    assert_eq!(
        second.source,
        EvaluationSource::Reused,
        "a body edit in an import moved the dependent's key"
    );
}

#[test]
fn the_abi_cutoff_holds_across_an_import_edit_the_interface_does_not_cover() {
    let mut driver = Driver::new(MemoryEngineStateStore::default());

    let alpha_interface = driver.publish_interface("alpha", ALPHA);
    let alpha_bodies = match bodies_of(
        &mut driver.engine,
        "alpha",
        &[("alpha.pi".into(), alpha_interface.source)],
        frontend_imports([]),
    ) {
        Ok(evaluation) => evaluation.value,
        Err(diagnostics) => unreachable!("alpha did not elaborate: {diagnostics:?}"),
    };
    let beta_blob = driver.publish(BETA.as_bytes());
    let imports = frontend_imports([frontend_import(alpha_interface)]);

    let first = match bodies_of(
        &mut driver.engine,
        "beta",
        &[("beta.pi".into(), beta_blob)],
        imports,
    ) {
        Ok(evaluation) => evaluation,
        Err(_) => unreachable!("bodies-of over an elaborable module completes"),
    };
    assert_eq!(first.source, EvaluationSource::Computed);
    let representation = representation_of(first.value);
    let Value::List(rules) = field_of(&representation, "rules") else {
        unreachable!("the rules field is a list");
    };
    assert_eq!(rules.len(), 1, "the wrap rule did not elaborate");
    assert!(
        diagnostics_of(&representation).is_empty(),
        "an elaborable module carried diagnostics"
    );

    let edited_interface = driver.publish_interface("alpha", ALPHA_LABEL_EDIT);
    assert_eq!(
        edited_interface.surface, alpha_interface.surface,
        "the surface artifact moved under a rule-label edit"
    );
    assert_eq!(edited_interface.abi, alpha_interface.abi);
    let edited_alpha_bodies = match bodies_of(
        &mut driver.engine,
        "alpha",
        &[("alpha.pi".into(), edited_interface.source)],
        frontend_imports([]),
    ) {
        Ok(evaluation) => evaluation.value,
        Err(diagnostics) => unreachable!("edited alpha did not elaborate: {diagnostics:?}"),
    };
    assert_ne!(edited_alpha_bodies, alpha_bodies);
    let second = match bodies_of(
        &mut driver.engine,
        "beta",
        &[("beta.pi".into(), beta_blob)],
        frontend_imports([frontend_import(edited_interface)]),
    ) {
        Ok(evaluation) => evaluation,
        Err(_) => unreachable!("the reusable lookup re-ran and failed"),
    };
    assert_eq!(
        second.source,
        EvaluationSource::Reused,
        "the reusable lookup missed after a rule-label edit"
    );

    let changed_interface = driver.publish_interface("alpha", ALPHA_REPRESENTATION_EDIT);
    assert_ne!(
        changed_interface.surface, alpha_interface.surface,
        "the surface artifact held under a representation edit"
    );
    let changed = bodies_of(
        &mut driver.engine,
        "beta",
        &[("beta.pi".into(), beta_blob)],
        frontend_imports([frontend_import(changed_interface)]),
    );
    assert_eq!(
        changed.map_or(EvaluationSource::Computed, |evaluation| evaluation.source),
        EvaluationSource::Computed,
        "the dependent was served stale after a semantic import edit"
    );
}

#[test]
fn a_fresh_engine_hydrates_the_cutoff_attempt() {
    let state = MemoryEngineStateStore::default();
    let mut first = Driver::new(state.clone());

    let alpha_interface = first.publish_interface("alpha", ALPHA);
    let beta_blob = first.publish(BETA.as_bytes());
    let evaluation = bodies_of(
        &mut first.engine,
        "beta",
        &[("beta.pi".into(), beta_blob)],
        frontend_imports([frontend_import(alpha_interface)]),
    );
    assert_eq!(
        evaluation.map_or(EvaluationSource::Computed, |evaluated| evaluated.source),
        EvaluationSource::Computed
    );
    drop(first);

    let mut second = Driver::new(state);
    let republished = second.publish(BETA.as_bytes());
    assert_eq!(
        republished, beta_blob,
        "content identity moved for equal bytes"
    );
    let resurfaced = second.publish_interface("alpha", ALPHA);
    assert_eq!(resurfaced.surface, alpha_interface.surface);
    let hydrated = bodies_of(
        &mut second.engine,
        "beta",
        &[("beta.pi".into(), beta_blob)],
        frontend_imports([frontend_import(resurfaced)]),
    );
    assert_eq!(
        hydrated.map_or(EvaluationSource::Computed, |evaluation| evaluation.source),
        EvaluationSource::Hydrated,
        "the fresh engine recomputed what durable state already held"
    );
}

#[test]
fn a_module_of_many_files_elaborates_as_one() {
    let mut driver = Driver::new(MemoryEngineStateStore::default());
    let types_blob = driver.publish(super::GAMMA_TYPES.as_bytes());
    let rules_blob = driver.publish(super::GAMMA_RULES.as_bytes());
    let evaluation = match bodies_of(
        &mut driver.engine,
        "gamma",
        &[
            ("gamma/types.pi".into(), types_blob),
            ("gamma/rules.pi".into(), rules_blob),
        ],
        frontend_imports([]),
    ) {
        Ok(evaluation) => evaluation,
        Err(_) => unreachable!("the two-file module elaborates"),
    };
    let representation = representation_of(evaluation.value);
    let Value::List(rules) = field_of(&representation, "rules") else {
        unreachable!("the rules field is a list");
    };
    assert_eq!(rules.len(), 1, "the cross-file reference did not elaborate");
    let Some(Value::Text(name)) = rules.first().map(|rule| field_of(rule, "name")) else {
        unreachable!("a rule entry names its rule");
    };
    assert_eq!(name.as_ref(), "label-of");
}

#[test]
fn a_module_that_does_not_elaborate_is_data_not_a_failed_attempt() {
    let mut driver = Driver::new(MemoryEngineStateStore::default());
    let broken_blob = driver.publish(BROKEN.as_bytes());
    let evaluation = match bodies_of(
        &mut driver.engine,
        "broken",
        &[("broken.pi".into(), broken_blob)],
        frontend_imports([]),
    ) {
        Ok(evaluation) => evaluation,
        Err(diagnostics) => unreachable!("a user error failed the attempt: {diagnostics:?}"),
    };
    let representation = representation_of(evaluation.value);
    let Value::List(rules) = field_of(&representation, "rules") else {
        unreachable!("the rules field is a list");
    };
    assert!(
        rules.is_empty(),
        "a rule that did not elaborate reached registration data"
    );
    let diagnostics = diagnostics_of(&representation);
    let Some(diagnostic) = diagnostics.first() else {
        unreachable!("the unknown name went unreported");
    };
    let Value::Int(code) = field_of(diagnostic, "code") else {
        unreachable!("a diagnostic carries a code");
    };
    assert_eq!(
        code.to_string(),
        "3007",
        "the diagnostic does not carry the unknown-name code"
    );
    assert!(
        matches!(field_of(diagnostic, "source"), Value::Blob(id) if *id == broken_blob),
        "the diagnostic does not name the file it came from"
    );
    let Value::List(incomplete) = field_of(&representation, "incomplete") else {
        unreachable!("the incomplete field is a list");
    };
    assert_eq!(incomplete.len(), 1);
    let Some(incomplete_rule) = incomplete.first() else {
        unreachable!("the incomplete rule was reported");
    };
    assert_eq!(
        field_of(incomplete_rule, "name"),
        &Value::Text("leak".into())
    );
}
