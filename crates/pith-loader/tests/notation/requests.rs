//! The request forms: the five constructs, the types that check them, and
//! the positions where a request may appear.

use pith_core::Value;
use pith_engine::Engine;

use super::{has_code, interface_of, load_err, load_ok, request_of};
use pith_loader::FrontendCode;

#[test]
fn the_five_request_constructs_elaborate() {
    let loaded = load_ok(
        "nominal Object = Blob

pure rule double(n: Int) -> Int = { n + n }

pure rule \"is-zero\"(n: Int) -> Bool = { n == 0 }

pure rule negate(b: Bool) -> Bool = { if b { false } else { true } }

pure rule one(n: Int) -> Text = {
  let doubled = ask Int (n)
  describe(doubled)
}

pure rule batch(flag: Bool) -> Text = {
  let (doubled, zero) = ask all (ask Int (7), ask Bool (flag))
  if zero { describe(doubled) } else { \"nonzero\" }
}

pure rule \"fan-out\"(ns: List<Int>) -> List<Int> = {
  ask all Int [ for n in ns { } (n) ]
}

pure rule filtered(ns: List<Int>) -> List<Text> = {
  ask all Text [ for n in ns { if n != 0 | if n != 5 } (n) ]
}

pure rule derived(ns: List<Int>) -> Text = {
  let doubled = ask all Int [ for n in ns { let once = n + n } (once) ]
  describe(doubled)
}

pure rule materialized(o: Object) -> Text = {
  let raw = bytes of unwrap(o)
  decode(raw)
}
",
    );
    let mut engine = Engine::new();
    for rule in loaded.represented_pure_rules() {
        let registered = rule.register(&mut engine);
        assert!(registered.is_ok());
    }
    let one = interface_of(&loaded, "one");
    let evaluated = engine
        .evaluate_pure(&request_of(one, &[Value::int(20)]))
        .unwrap_or_else(|diagnostics| unreachable!("the ask evaluates: {diagnostics:?}"));
    assert_eq!(evaluated.value, Value::Text("40".into()));

    let batch = interface_of(&loaded, "batch");
    let flagged = engine
        .evaluate_pure(&request_of(batch, &[Value::Bool(false)]))
        .unwrap_or_else(|diagnostics| unreachable!("the batch evaluates: {diagnostics:?}"));
    assert_eq!(flagged.value, Value::Text("14".into()));
    let unflagged = engine
        .evaluate_pure(&request_of(batch, &[Value::Bool(true)]))
        .unwrap_or_else(|diagnostics| unreachable!("the batch evaluates: {diagnostics:?}"));
    assert_eq!(unflagged.value, Value::Text("nonzero".into()));

    let fan_out = interface_of(&loaded, "fan-out");
    let evaluated = engine
        .evaluate_pure(&request_of(
            fan_out,
            &[Value::List(Box::new([
                Value::int(1),
                Value::int(2),
                Value::int(3),
            ]))],
        ))
        .unwrap_or_else(|diagnostics| unreachable!("the fan-out evaluates: {diagnostics:?}"));
    assert_eq!(
        evaluated.value,
        Value::List(Box::new([Value::int(2), Value::int(4), Value::int(6),]))
    );

    let filtered = interface_of(&loaded, "filtered");
    let evaluated = engine
        .evaluate_pure(&request_of(
            filtered,
            &[Value::List(Box::new([
                Value::int(0),
                Value::int(5),
                Value::int(0),
                Value::int(7),
            ]))],
        ))
        .unwrap_or_else(|diagnostics| unreachable!("the filter evaluates: {diagnostics:?}"));
    assert_eq!(
        evaluated.value,
        Value::List(Box::new([Value::Text("14".into())]))
    );

    let derived = interface_of(&loaded, "derived");
    let evaluated = engine
        .evaluate_pure(&request_of(
            derived,
            &[Value::List(Box::new([Value::int(1), Value::int(2)]))],
        ))
        .unwrap_or_else(|diagnostics| unreachable!("the derivation evaluates: {diagnostics:?}"));
    let doubled = Value::List(Box::new([Value::int(4), Value::int(8)]));
    assert_eq!(evaluated.value, Value::Text(doubled.describe().into()));
}

#[test]
fn head_types_elide_from_the_positions_that_check_them() {
    let loaded = load_ok(
        "nominal Report = Bool

pure rule verdict(b: Bool) -> Report = { Report(b) }

pure rule \"tail-elided\"(n: Int) -> Report = {
  ask (n == 0)
}

pure rule \"let-elided\"(s: Text) -> Report = {
  let answer : Report = ask (s == \"\")
  answer
}

pure rule \"list-elided\"(ns: List<Int>) -> List<Report> = {
  ask all [ for n in ns { } (n == 0) ]
}
",
    );
    let mut engine = Engine::new();
    for rule in loaded.represented_pure_rules() {
        assert!(rule.register(&mut engine).is_ok());
    }
    let elided = interface_of(&loaded, "tail-elided");
    let evaluated = engine
        .evaluate_pure(&request_of(elided, &[Value::int(1)]))
        .unwrap_or_else(|diagnostics| unreachable!("the elision evaluates: {diagnostics:?}"));
    assert_eq!(
        evaluated.value,
        Value::Nominal {
            name: "test.Report".into(),
            representation: Box::new(Value::Bool(false)),
        }
    );
}

#[test]
fn a_headless_request_without_a_checking_type_is_refused() {
    let headless_let =
        load_err("pure rule f(n: Int) -> Bool = {\n  let answer = ask (n)\n  answer\n}\n");
    assert!(has_code(&headless_let, FrontendCode::HeadlessRequest));

    let headless_member = load_err(
        "pure rule f(n: Int) -> Bool = {\n  let (a, b) = ask all (ask (n), ask Bool (n == 0))\n  b\n}\n",
    );
    assert!(has_code(&headless_member, FrontendCode::HeadlessRequest));

    let non_list_annotation = load_err(
        "pure rule f(n: Int) -> Bool = {\n  let answer : Bool = ask all [ for n in [n] { } (n) ]\n  true\n}\n",
    );
    assert!(has_code(
        &non_list_annotation,
        FrontendCode::HeadlessRequest
    ));
}

#[test]
fn a_rule_may_not_request_its_own_interface() {
    let self_request = load_err("pure rule f(n: Int) -> Int = {\n  ask Int (n)\n}\n");
    assert!(has_code(&self_request, FrontendCode::SelfRequest));
}

#[test]
fn requests_live_only_in_checking_position() {
    let nested = load_err("pure rule f(n: Int) -> Int = { 1 + ask Int (n) }\n");
    assert!(nested.iter().any(
        |diagnostic| diagnostic.message.0.contains("checking position")
            || diagnostic.message.0.contains("right-hand side")
    ));

    let group_with_single =
        load_err("pure rule f(n: Int) -> Int = {\n  let (a, b) = ask Int (n)\n  a\n}\n");
    assert!(
        group_with_single
            .iter()
            .any(|diagnostic| diagnostic.message.0.contains("binder group"))
    );

    let single_with_group = load_err(
        "pure rule f(n: Int) -> Int = {\n  let answer = ask all (ask Int (n), ask Int (n))\n  answer\n}\n",
    );
    assert!(
        single_with_group
            .iter()
            .any(|diagnostic| diagnostic.message.0.contains("group of names"))
    );

    let action_in_batch = load_err(
        "nominal Out = Text\n\npure rule f(n: Int) -> Int = {\n  let (a, b) = ask all (ask Int (n), run Out (\"sh\"))\n  a\n}\n",
    );
    assert!(
        action_in_batch
            .iter()
            .any(|diagnostic| diagnostic.message.0.contains("pure requests only"))
    );

    let batch_tail =
        load_err("pure rule f(n: Int) -> Int = {\n  ask all (ask Int (n), ask Int (n))\n}\n");
    assert!(
        batch_tail
            .iter()
            .any(|diagnostic| diagnostic.message.0.contains("body's tail"))
    );
}

#[test]
fn comprehension_filters_after_bindings_are_refused() {
    let late = load_err(
        "pure rule f(ns: List<Int>) -> List<Int> = {\n  ask all Int [ for n in ns { let m = n + 1 | if m != 0 } (m) ]\n}\n",
    );
    assert!(has_code(&late, FrontendCode::FilterAfterBinding));
}

#[test]
fn empty_matches_and_unbatchable_requests_are_refused() {
    let empty_match =
        load_err("sum Choice = Yes | No\npure rule f(value: Choice) -> Int = { match value {} }\n");
    assert!(has_code(&empty_match, FrontendCode::InvalidBody));

    let bytes_in_batch = load_err(
        "nominal Object = Blob\npure rule f(value: Object) -> Bytes = {\n  let (raw, other) = ask all (bytes of unwrap(value), ask Bytes ())\n  raw\n}\n",
    );
    assert!(has_code(&bytes_in_batch, FrontendCode::UnexpectedToken));
}
