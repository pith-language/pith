//! Written-body notation, request forms, local definitions, entries, and
//! metadata blocks.

use core::assert_matches;

use pith_core::{Pure, Request, Value};
use pith_diag::SourceId;
use pith_engine::Engine;
use pith_loader::{
    DefinitionKind, FrontendCode, ImportEnv, LoadedModule, ModuleSource, load_module,
};
use proptest::prelude::*;

fn load(text: &str) -> Result<LoadedModule, Box<[pith_diag::Diag]>> {
    load_module(
        &ModuleSource::new("test", SourceId::from_raw(1), "test.pi", text),
        &ImportEnv::new(),
    )
}

fn load_ok(text: &str) -> LoadedModule {
    match load(text) {
        Ok(loaded) => loaded,
        Err(diagnostics) => unreachable!("the notation loads: {diagnostics:?}"),
    }
}

fn load_err(text: &str) -> Box<[pith_diag::Diag]> {
    match load(text) {
        Err(diagnostics) => diagnostics,
        Ok(_) => unreachable!("the notation refuses this module"),
    }
}

fn has_code(diagnostics: &[pith_diag::Diag], code: FrontendCode) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == code.stable())
}

const EXPRESSIONS: &str = "sum Shape = circle(Int) | square

pure rule \"make-shape\"(n: Int) -> Shape = {
  if n == 0 { square() } else { circle(n * 2 - 1) }
}

pure rule area(s: Shape) -> Int = {
  match s {
    circle(radius) { radius * 3 }
    square { 14 }
  }
}

pure rule described(s: Shape) -> Text = {
  match s {
    circle(radius) { concat(describe(radius), \" sides\") }
    square { \"four sides\" }
  }
}

pure rule \"sum-of\"(xs: List<Int>) -> Int = {
  fold xs from 0 { (element, accumulator) -> element + accumulator }
}

";

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
    let area = loaded
        .represented_pure_rule("area")
        .unwrap_or_else(|| unreachable!("the rule elaborates"))
        .interface()
        .clone();
    let make = loaded
        .represented_pure_rule("make-shape")
        .unwrap_or_else(|| unreachable!("the rule elaborates"))
        .interface()
        .clone();
    let built = engine
        .evaluate_pure(&request_of(&make, &[Value::int(3)]))
        .unwrap_or_else(|diagnostics| unreachable!("the shape builds: {diagnostics:?}"));
    let evaluated = engine
        .evaluate_pure(&request_of(&area, &[built.value]))
        .unwrap_or_else(|diagnostics| unreachable!("the area computes: {diagnostics:?}"));
    assert_eq!(evaluated.value, Value::int(15));
}

fn request_of(interface: &pith_core::Interface, inputs: &[Value]) -> Request<Pure> {
    Request::new(
        "notation",
        interface.clone(),
        inputs,
        pith_diag::Span::none(),
    )
}

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

fn interface_of<'a>(loaded: &'a LoadedModule, label: &str) -> &'a pith_core::Interface {
    loaded
        .represented_pure_rule(label)
        .unwrap_or_else(|| unreachable!("the rule elaborates"))
        .interface()
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
fn local_definitions_elaborate_to_first_order_calls() {
    let loaded = load_ok(
        "let base : Int = 40

let message : Text = describe(base + base)

pure rule \"with-locals\"(n: Int) -> Text = {
  concat(message, describe(n))
}
",
    );
    assert_eq!(
        loaded.represented_pure_rules().count(),
        3,
        "two definitions and one rule"
    );
    let mut engine = Engine::new();
    for rule in loaded.represented_pure_rules() {
        assert!(rule.register(&mut engine).is_ok());
    }
    let rule = interface_of(&loaded, "with-locals");
    let evaluated = engine
        .evaluate_pure(&request_of(rule, &[Value::int(1)]))
        .unwrap_or_else(|diagnostics| unreachable!("the locals evaluate: {diagnostics:?}"));
    assert_eq!(evaluated.value, Value::Text("801".into()));
}

#[test]
fn local_definitions_are_earlier_in_file_only() {
    let forward = load_err(
        "let ahead : Int = behind\n\nlet behind : Int = 1\n\npure rule f() -> Int = { ahead }\n",
    );
    assert!(has_code(&forward, FrontendCode::OutOfOrderLocal));

    let recursive = load_err("let loop : Int = loop + 1\n\npure rule f() -> Int = { loop }\n");
    assert!(has_code(&recursive, FrontendCode::OutOfOrderLocal));

    let duplicate = load_err("let one : Int = 1\n\nlet one : Int = 2\n");
    assert!(has_code(&duplicate, FrontendCode::DuplicateLocal));
}

#[test]
fn entries_and_about_blocks_ride_the_surface() {
    let loaded = load_ok(
        "about {\n  description: \"the notation\",\n  maintainers: [\"karol\"],\n}\n\nnominal Report = Bool\n\nentry check : Report = ask (1 == 1)\n",
    );
    let definitions = loaded.positions().definitions();
    assert_matches!(
        definitions,
        [.., definition] if matches!(definition.kind(), DefinitionKind::Entry)
    );
    let [.., about] = loaded.about() else {
        unreachable!("the about block rides the loaded module");
    };
    assert_eq!(about.fields.len(), 2);

    let duplicate = load_err("entry a : Bool = ask (true)\n\nentry a : Bool = ask (false)\n");
    assert!(has_code(&duplicate, FrontendCode::DuplicateEntry));

    let run_entry = load_err("nominal Out = Text\n\nentry dev : Out = run Out (\"sh\")\n");
    assert!(run_error_mentions_the_effect(&run_entry));

    let expression_entry = load_err("entry dev : Bool = true\n");
    assert!(
        expression_entry
            .iter()
            .any(|diagnostic| diagnostic.message.0.contains("bound to a request"))
    );

    let invalid_entry = load_err("entry broken : Missing = ask Missing (unknown)\n");
    assert!(has_code(&invalid_entry, FrontendCode::UnknownName));
}

fn run_error_mentions_the_effect(diagnostics: &[pith_diag::Diag]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.0.contains("pure request"))
}

#[test]
fn a_rule_may_not_request_its_own_interface() {
    let self_request = load_err("pure rule f(n: Int) -> Int = {\n  ask Int (n)\n}\n");
    assert!(has_code(&self_request, FrontendCode::SelfRequest));
}

#[test]
fn builtins_cannot_be_shadowed() {
    let binder = load_err("pure rule f(module: Int) -> Int = { module }\n");
    assert!(has_code(&binder, FrontendCode::BuiltinShadowed));

    let local = load_err("let describe : Int = 1\n");
    assert!(has_code(&local, FrontendCode::BuiltinShadowed));
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

#[test]
fn annotation_and_body_mismatches_are_spanned_refusals() {
    let annotation = load_err("pure rule f() -> Int = {\n  let answer : Int = 1 == 1\n  0\n}\n");
    assert!(has_code(&annotation, FrontendCode::TypeMismatch));

    let wrong_output = load_err("pure rule f() -> Int = { \"text\" }\n");
    assert!(has_code(&wrong_output, FrontendCode::InvalidBody));

    let non_exhaustive = load_err(
        "sum Shape = circle(Int) | square\n\npure rule f(s: Shape) -> Int = {\n  match s {\n    circle(radius) { radius }\n  }\n}\n",
    );
    assert!(has_code(&non_exhaustive, FrontendCode::InvalidBody));

    let unknown_name = load_err("pure rule f(n: Int) -> Int = { missing }\n");
    assert!(has_code(&unknown_name, FrontendCode::UnknownName));

    let duplicate_arm = load_err(
        "sum Shape = circle(Int) | square\n\npure rule f(s: Shape) -> Int = {\n  match s {\n    circle(radius) { radius }\n    circle(other) { other }\n    square { 0 }\n  }\n}\n",
    );
    assert!(has_code(&duplicate_arm, FrontendCode::DuplicateArm));

    let duplicate_binder = load_err("pure rule f(n: Int) -> Int = {\n  let n = 1\n  n\n}\n");
    assert!(has_code(&duplicate_binder, FrontendCode::DuplicateBinder));

    let empty_list = load_err("pure rule f() -> Int = {\n  let items = []\n  0\n}\n");
    assert!(has_code(&empty_list, FrontendCode::TypeMismatch));

    let unwrapped_non_nominal = load_err("pure rule f(t: Text) -> Text = { unwrap(t) }\n");
    assert!(has_code(&unwrapped_non_nominal, FrontendCode::TypeMismatch));
}

#[test]
fn binary_operator_mismatches_name_their_operator() {
    let arithmetic = load_err("pure rule f(ns: List<Int>) -> Int = { 1 + ns }\n");
    assert!(has_code(&arithmetic, FrontendCode::TypeMismatch));
    assert!(
        arithmetic
            .iter()
            .any(|diagnostic| diagnostic.message.0.contains("expected"))
    );
    assert!(
        !arithmetic
            .iter()
            .any(|diagnostic| diagnostic.message.0.contains("`==`"))
    );

    let equality = load_err("pure rule f(ns: List<Int>) -> Int = { 1 == ns }\n");
    assert!(
        equality
            .iter()
            .any(|diagnostic| diagnostic.message.0.contains("compares one type"))
    );
}

#[test]
fn body_digests_agree_under_qualified_and_unqualified_spelling() {
    let dependency = load_ok("nominal In = Text\n\nnominal Out = Text\n");
    let mut imports = ImportEnv::new();
    imports.insert_loaded(&dependency);

    let qualified = load_module(
        &ModuleSource::new(
            "qualified",
            SourceId::from_raw(2),
            "qualified.pi",
            "import test\n\npure rule f(x: test.In) -> test.Out = {\n  let text = ask test.Out (describe(x))\n  text\n}\n",
        ),
        &imports,
    );
    let aliased = load_module(
        &ModuleSource::new(
            "aliased",
            SourceId::from_raw(3),
            "aliased.pi",
            "import test\n\ntype In = test.In\n\ntype Out = test.Out\n\npure rule f(x: In) -> Out = {\n  let text = ask Out (describe(x))\n  text\n}\n",
        ),
        &imports,
    );
    let (Ok(qualified), Ok(aliased)) = (qualified, aliased) else {
        unreachable!("both spellings elaborate");
    };
    let digests = |loaded: &LoadedModule| {
        loaded
            .pure_rules()
            .iter()
            .filter_map(|rule| rule.represented_digest())
            .collect::<Vec<_>>()
    };
    let qualified_digests = digests(&qualified);
    let aliased_digests = digests(&aliased);
    assert_eq!(qualified_digests, aliased_digests);
}

#[test]
fn quoted_names_spell_bodies_and_constructors() {
    let loaded = load_ok(
        "nominal \"expected-owner\" = Text\n\nsum \"the-shape\" = \"one-way\"(Int) | \"two-way\"\n\npure rule \"a-rule\"(x: \"expected-owner\") -> \"the-shape\" = {\n  if unwrap(x) == \"\" { \"two-way\"() } else { \"one-way\"(1) }\n}\n",
    );
    assert!(loaded.represented_pure_rule("a-rule").is_some());
}

proptest! {
    #[test]
    fn arbitrary_text_terminates_stably(text in any::<String>()) {
        let first = load(&text);
        let second = load(&text);
        prop_assert_eq!(first.is_ok(), second.is_ok());
        if let (Err(first), Err(second)) = (first, second) {
            let positions = |diagnostics: &[pith_diag::Diag]| {
                diagnostics
                    .iter()
                    .map(|diagnostic| (diagnostic.code, diagnostic.span))
                    .collect::<Vec<_>>()
            };
            prop_assert_eq!(positions(&first), positions(&second));
        }
    }
}
