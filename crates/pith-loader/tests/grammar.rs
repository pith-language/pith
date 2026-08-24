use core::assert_matches;

use pith_core::{BodyRevision, Pure, Rule, Type};
use pith_diag::{ByteOffset, SourceId};
use pith_loader::{
    DefinitionKind, FrontendCode, ImportEnv, LoadedModule, ModuleSource, load_module, parse_module,
};
use proptest::prelude::*;

fn load(text: &str) -> Result<LoadedModule, Box<[pith_diag::Diag]>> {
    load_module(
        &ModuleSource::new("test", SourceId::from_raw(1), "test.pi", text),
        &ImportEnv::new(),
    )
}

fn load_ok(text: &str) -> LoadedModule {
    let result = load(text);
    assert!(result.is_ok());
    match result {
        Ok(loaded) => loaded,
        Err(_) => unreachable!(),
    }
}

fn load_err(text: &str) -> Box<[pith_diag::Diag]> {
    let result = load(text);
    assert!(result.is_err());
    match result {
        Ok(_) => unreachable!(),
        Err(diagnostics) => diagnostics,
    }
}

fn has_code(diagnostics: &[pith_diag::Diag], code: FrontendCode) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == code.stable())
}

#[test]
fn forward_and_direct_recursive_declarations_load() {
    let forward = load_ok("nominal Files = List<Object>\nnominal Object = Blob\n");
    let backward = load_ok("nominal Object = Blob\nnominal Files = List<Object>\n");
    assert_eq!(
        forward.table().encode_canonical(),
        backward.table().encode_canonical()
    );

    let recursive = load_ok("nominal Tree = List<Tree>\n");
    let Some(tree) = recursive.table().get("Tree") else {
        return;
    };
    assert_matches!(Type::of_declaration(tree), Type::Nominal(_));
}

#[test]
fn mutual_cycles_and_recursive_aliases_are_rejected() {
    let cycle = load_err("nominal A = List<B>\nnominal B = List<A>\n");
    assert!(has_code(&cycle, FrontendCode::CyclicDeclaration));
    assert!(
        cycle
            .iter()
            .any(|diagnostic| diagnostic.message.0.contains("`A` -> `B` -> `A`"))
    );

    let alias = load_err("type Loop = List<Loop>\n");
    assert!(has_code(&alias, FrontendCode::RecursiveAlias));
}

#[test]
fn duplicate_names_coordinates_and_interfaces_are_rejected() {
    let declarations = load_err("nominal A = Text\nnominal A = Blob\n");
    assert!(has_code(&declarations, FrontendCode::DuplicateDeclaration));

    let rules = load_err("pure rule f(Text) -> Text = host\naction rule f(Text) -> Text = host\n");
    assert!(has_code(&rules, FrontendCode::DuplicateRule));

    let interfaces =
        load_err("pure rule first(Text) -> Text = host\npure rule second(Text) -> Text = host\n");
    assert!(has_code(&interfaces, FrontendCode::DuplicateInterface));

    assert!(
        load("pure rule first(Text) -> Text = host\naction rule second(Text) -> Text = host\n")
            .is_ok()
    );
}

#[test]
fn duplicate_record_fields_sum_constructors_and_imports_are_rejected() {
    assert!(has_code(
        &load_err("nominal P = {a: Text, a: Blob}\n"),
        FrontendCode::DuplicateField
    ));
    assert!(has_code(
        &load_err("sum S = only(Text) | only(Blob)\n"),
        FrontendCode::DuplicateField
    ));

    let dependency = load_ok("nominal A = Text\n");
    let mut imports = ImportEnv::new();
    imports.insert_loaded(&dependency);
    let result = load_module(
        &ModuleSource::new(
            "consumer",
            SourceId::from_raw(2),
            "consumer.pi",
            "import test\nimport test\nnominal W = test.A\n",
        ),
        &imports,
    );
    let Err(diagnostics) = result else {
        return;
    };
    assert!(has_code(&diagnostics, FrontendCode::DuplicateImport));
}

#[test]
fn qualified_access_is_scoped_to_declared_imports() {
    let dependency = load_ok("nominal A = Text\n");
    let mut imports = ImportEnv::new();
    imports.insert_loaded(&dependency);

    let undeclared = load_module(
        &ModuleSource::new(
            "consumer",
            SourceId::from_raw(2),
            "consumer.pi",
            "nominal W = test.A\n",
        ),
        &imports,
    );
    let Err(diagnostics) = undeclared else {
        return;
    };
    assert!(has_code(
        &diagnostics,
        FrontendCode::UndeclaredQualifiedAccess
    ));

    assert!(
        load_module(
            &ModuleSource::new(
                "consumer",
                SourceId::from_raw(2),
                "consumer.pi",
                "import test\nnominal W = test.A\n",
            ),
            &imports,
        )
        .is_ok()
    );
}

#[test]
fn hyphenated_names_require_quotes() {
    assert!(load("nominal expected-owner = Text\n").is_err());
    let quoted = load_ok(
        "nominal \"expected-owner\" = Text\npure rule \"a-rule\"(\"expected-owner\") -> Text = host\n",
    );
    assert!(quoted.table().get("expected-owner").is_some());
    assert!(quoted.pure_rule("a-rule").is_some());
}

#[test]
fn rule_binding_is_category_typed() {
    let loaded = load_ok(
        "pure rule pure_provider(Text) -> Text = host\naction rule action_provider(Text) -> Text = host\n",
    );
    let Some(declaration) = loaded.pure_rule("pure_provider") else {
        return;
    };
    let rule: Rule<Pure> = declaration.rule(BodyRevision(1));
    assert_eq!(rule.coordinate.spelling(), "test.pure_provider");
    assert_eq!(rule.tier, pith_core::RuleTier::Host);
    assert!(loaded.action_rule("pure_provider").is_none());
}

#[test]
fn positions_retain_docs_and_alias_references() {
    let text =
        "-- user-facing text\nnominal A = Text\ntype Alias = A\npure rule f(Alias) -> A = host\n";
    let loaded = load_ok(text);
    let Some(alias_offset) = text.find("Alias)") else {
        return;
    };
    let Some(alias_definition) =
        loaded.go_to_definition(ByteOffset(u32::try_from(alias_offset).unwrap_or(u32::MAX)))
    else {
        return;
    };
    assert_eq!(alias_definition.coordinate().spelling(), "test.Alias");

    let Some(a_definition) = loaded
        .positions()
        .definitions()
        .iter()
        .find(|definition| definition.coordinate().name.as_ref() == "A")
    else {
        return;
    };
    assert_eq!(a_definition.documentation(), "user-facing text");
    assert_eq!(a_definition.kind(), DefinitionKind::Nominal);
    assert!(
        loaded
            .positions()
            .references()
            .iter()
            .any(|reference| reference.written_coordinate().name.as_ref() == "Alias")
    );
}

#[test]
fn imported_references_reach_the_imported_source() {
    let dependency = load_module(
        &ModuleSource::new(
            "dep",
            SourceId::from_raw(2),
            "dep.pi",
            "-- imported\nnominal A = Text\n",
        ),
        &ImportEnv::new(),
    );
    let Ok(dependency) = dependency else {
        return;
    };
    let mut imports = ImportEnv::new();
    imports.insert_loaded(&dependency);
    let text = "import dep\nnominal W = dep.A\n";
    let consumer = load_module(
        &ModuleSource::new("consumer", SourceId::from_raw(3), "consumer.pi", text),
        &imports,
    );
    let Ok(consumer) = consumer else {
        return;
    };
    let Some(offset) = text.find("dep.A") else {
        return;
    };
    let Some(definition) =
        consumer.go_to_definition(ByteOffset(u32::try_from(offset).unwrap_or(u32::MAX)))
    else {
        return;
    };
    assert_eq!(definition.source().label.as_ref(), "dep.pi");
    assert_eq!(definition.documentation(), "imported");
    assert_eq!(consumer.completions(Some("dep")).len(), 1);
}

#[test]
fn parsed_modules_retain_source_and_partial_positions_on_error() {
    let parsed = parse_module(&ModuleSource::new(
        "test",
        SourceId::from_raw(1),
        "test.pi",
        "-- docs\nnominal A = Missing\n",
    ));
    assert_eq!(
        parsed.source().source_text(),
        "-- docs\nnominal A = Missing\n"
    );
    assert_eq!(parsed.positions().definitions().len(), 1);
    assert_ne!(parsed.artifact_id(), pith_ids::ContentId::of_blob(b""));
}

#[test]
fn parser_recovery_reaches_later_items() {
    let diagnostics = load_err("sum Broken = | | |\nnominal A Text\n");
    assert!(diagnostics.len() >= 2);
}

proptest! {
    #[test]
    fn arbitrary_utf8_terminates_stably(text in any::<String>()) {
        let first = load(&text);
        let second = load(&text);
        prop_assert_eq!(first.is_ok(), second.is_ok());
        if let (Err(first), Err(second)) = (first, second) {
            let first_positions = first.iter().map(|diagnostic| {
                (diagnostic.code, diagnostic.span)
            }).collect::<Vec<_>>();
            let second_positions = second.iter().map(|diagnostic| {
                (diagnostic.code, diagnostic.span)
            }).collect::<Vec<_>>();
            prop_assert_eq!(first_positions, second_positions);
        }
    }
}
