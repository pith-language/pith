//! The direct loader over a multi-file module: the merged span space
//! elaborates as one module, and every position and diagnostic attributes
//! to the file that owns it, which concatenating source strings would
//! erase.

use pith_loader::{
    FrontendCode, ImportEnv, ModuleFile, SourceSet, elaborate_module, parse_module_sources,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn source_set(files: &[(&str, &str)]) -> SourceSet {
    SourceSet::new(
        files
            .iter()
            .map(|(path, text)| ModuleFile::new(*path, *text)),
    )
    .unwrap_or_else(|error| unreachable!("the fixture holds distinct paths: {error}"))
}

fn elaborate(files: &[(&str, &str)]) -> Result<pith_loader::LoadedModule, Box<[pith_diag::Diag]>> {
    let parsed = parse_module_sources("example/dep", &source_set(files));
    elaborate_module(parsed, &ImportEnv::new())
}

fn has_code(diagnostics: &[pith_diag::Diag], code: FrontendCode) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == code.stable())
}

#[test]
fn a_declaration_in_one_file_is_visible_to_the_rules_in_another() -> TestResult {
    let loaded = match elaborate(&[
        (
            "src/rules.pi",
            "pure rule greet(Message) -> Text = { ask Text (\"hello\") }\n",
        ),
        ("src/types.pi", "nominal Message = Text\n"),
    ]) {
        Ok(loaded) => loaded,
        Err(diagnostics) => unreachable!("the module elaborates: {diagnostics:?}"),
    };
    assert_eq!(loaded.module(), "example/dep");
    assert_eq!(loaded.table().len(), 1);
    assert!(loaded.represented_pure_rule("greet").is_some());
    Ok(())
}

#[test]
fn an_error_in_the_second_file_names_that_file_and_local_offsets() -> TestResult {
    let diagnostics = elaborate(&[
        ("src/types.pi", "nominal Message = Text\n"),
        (
            "src/rules.pi",
            "-- a rule over a missing type\npure rule greet(Missing) -> Text = host\n",
        ),
    ])
    .err()
    .unwrap_or_else(|| unreachable!("the missing type refuses the module"));
    assert!(has_code(&diagnostics, FrontendCode::UnknownName));
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == FrontendCode::UnknownName.stable())
        .unwrap_or_else(|| unreachable!("the missing type is diagnosed"));
    let source = diagnostic
        .source
        .as_ref()
        .unwrap_or_else(|| unreachable!("the diagnostic carries its source"));
    assert_eq!(source.label.as_ref(), "src/rules.pi");
    let (line, column) = source.line_col(diagnostic.span.start);
    assert_eq!(
        (line, column),
        (2, 17),
        "the span is local to the second file"
    );
    Ok(())
}

#[test]
fn a_duplicate_declaration_across_files_is_refused_at_the_later_file() -> TestResult {
    let diagnostics = elaborate(&[
        ("src/a.pi", "nominal Message = Text\n"),
        ("src/b.pi", "nominal Message = Bytes\n"),
    ])
    .err()
    .unwrap_or_else(|| unreachable!("the duplicate declaration refuses the module"));
    assert!(has_code(&diagnostics, FrontendCode::DuplicateDeclaration));
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == FrontendCode::DuplicateDeclaration.stable())
        .unwrap_or_else(|| unreachable!("the duplicate is diagnosed"));
    let source = diagnostic
        .source
        .as_ref()
        .unwrap_or_else(|| unreachable!("the diagnostic carries its source"));
    assert_eq!(
        source.label.as_ref(),
        "src/b.pi",
        "sorted order makes b.pi later"
    );
    Ok(())
}

#[test]
fn the_source_set_order_is_canonical_by_construction() {
    // The set sorts at acquisition, so the same files elaborate the same
    // merged module whatever order a directory enumeration produced.
    let files = [
        ("src/b.pi", "nominal B = Text\n"),
        ("src/a.pi", "nominal A = Text\n"),
    ];
    let forward = parse_module_sources("example/dep", &source_set(&files));
    let reversed = parse_module_sources("example/dep", &source_set(&[files[1], files[0]]));
    assert_eq!(forward.artifact_id(), reversed.artifact_id());
}

#[test]
fn go_to_definition_answers_from_the_file_the_offset_names() -> TestResult {
    let loaded = match elaborate(&[
        (
            "src/rules.pi",
            "pure rule greet(who: Message) -> Text = { unwrap who }\n",
        ),
        ("src/types.pi", "nominal Message = Text\n"),
    ]) {
        Ok(loaded) => loaded,
        Err(diagnostics) => unreachable!("the module elaborates: {diagnostics:?}"),
    };
    let sources = loaded.files().sources();
    let rules = sources
        .first()
        .cloned()
        .unwrap_or_else(|| unreachable!("sorted order"));
    let types = sources
        .get(1)
        .cloned()
        .unwrap_or_else(|| unreachable!("two files"));

    // The `Message` reference sits in rules.pi at a local offset that also
    // exists in types.pi; the lookup must answer from rules.pi's reference
    // and land on types.pi's definition.
    let reference = loaded
        .positions()
        .references()
        .iter()
        .find(|reference| {
            reference.written_coordinate().name.as_ref() == "Message"
                && reference.source().label.as_ref() == "src/rules.pi"
        })
        .unwrap_or_else(|| unreachable!("the reference is recorded with its file"));
    let definition = loaded
        .go_to_definition(&rules, reference.span().start)
        .unwrap_or_else(|| unreachable!("the reference resolves from its own file"));
    assert_eq!(
        definition.source().label.as_ref(),
        "src/types.pi",
        "the definition is attributed to the file that declares it"
    );
    assert_eq!(definition.coordinate().name.as_ref(), "Message");

    // An offset inside types.pi resolves within types.pi alone: the
    // declaration's own span, not rules.pi's reference.
    let declared = loaded
        .go_to_definition(&types, definition.span().start)
        .unwrap_or_else(|| unreachable!("the declaration covers its own offset"));
    assert_eq!(declared.source().label.as_ref(), "src/types.pi");
    Ok(())
}
