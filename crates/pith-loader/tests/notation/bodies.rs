//! Represented bodies: the notation a body is written in, and what those
//! bodies evaluate to through the engine.

use core::assert_matches;

use pith_core::Value;
use pith_engine::Engine;

use super::{EXPRESSIONS, interface_of, load_ok, request_of};
use pith_loader::DefinitionKind;

#[test]
fn expressions_elaborate_to_represented_bodies() {
    let loaded = load_ok(EXPRESSIONS);
    let rule = loaded
        .represented_pure_rule("area")
        .unwrap_or_else(|| unreachable!("the rule elaborates"));
    let mut engine = Engine::new();
    let registered = rule.register(&mut engine);
    assert!(registered.is_ok());
    assert_matches!(
        loaded.positions().definitions(),
        [.., definition] if matches!(definition.kind(), DefinitionKind::RepresentedRule(_))
    );
}

#[test]
fn represented_bodies_evaluate_through_the_engine() {
    let loaded = load_ok(EXPRESSIONS);
    let mut engine = Engine::new();
    for rule in loaded.represented_pure_rules() {
        let registered = rule.register(&mut engine);
        assert!(registered.is_ok());
    }
    let area = interface_of(&loaded, "area");
    let make = interface_of(&loaded, "make-shape");
    let built = engine
        .evaluate_pure(&request_of(make, &[Value::int(3)]))
        .unwrap_or_else(|diagnostics| unreachable!("the shape builds: {diagnostics:?}"));
    let evaluated = engine
        .evaluate_pure(&request_of(area, &[built.value]))
        .unwrap_or_else(|diagnostics| unreachable!("the area computes: {diagnostics:?}"));
    assert_eq!(evaluated.value, Value::int(15));
}

#[test]
fn blob_materialization_elaborates_and_evaluates() {
    let loaded = load_ok(
        "nominal Object = Blob

pure rule materialized(o: Object) -> Text = {
  let raw = bytes of unwrap(o)
  decode(raw)
}
",
    );
    let mut engine = Engine::new();
    let rule = loaded
        .represented_pure_rule("materialized")
        .unwrap_or_else(|| unreachable!("the rule elaborates"));
    let registered = rule.register(&mut engine);
    assert!(registered.is_ok());
}

#[test]
fn quoted_names_spell_bodies_and_constructors() {
    let loaded = load_ok(
        "nominal \"expected-owner\" = Text\n\nsum \"the-shape\" = \"one-way\"(Int) | \"two-way\"\n\npure rule \"a-rule\"(x: \"expected-owner\") -> \"the-shape\" = {\n  if unwrap(x) == \"\" { \"two-way\"() } else { \"one-way\"(1) }\n}\n",
    );
    assert!(loaded.represented_pure_rule("a-rule").is_some());
}
