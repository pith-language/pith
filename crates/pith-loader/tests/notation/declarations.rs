//! The module-level surface: local definitions, entries and metadata
//! blocks, the builtins that cannot be shadowed, and the spanned refusals
//! of annotation and body mismatches.

use core::assert_matches;

use pith_core::Value;
use pith_engine::Engine;

use super::{has_code, interface_of, load_err, load_ok, request_of};
use pith_diag::SourceId;
use pith_loader::{
    DefinitionKind, FrontendCode, ImportEnv, LoadedModule, ModuleSource, load_module,
};

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
fn builtins_cannot_be_shadowed() {
    // `module` opens a header clause and cannot be spelled as a binder at
    // all; the other builtin names parse and are refused at elaboration.
    let keyword = load_err("pure rule f(module: Int) -> Int = { 0 }\n");
    assert!(!keyword.is_empty(), "`module` is a reserved word");

    let binder = load_err("pure rule f(describe: Int) -> Int = { describe }\n");
    assert!(has_code(&binder, FrontendCode::BuiltinShadowed));

    let local = load_err("let describe : Int = 1\n");
    assert!(has_code(&local, FrontendCode::BuiltinShadowed));
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
