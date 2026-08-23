use pith_core::{BodyRevision, DeclarationTable};
use pith_ids::{ContentId, DeclarationDigest};
use pith_loader::{ImportEnv, LoadedModule, ModuleSource, load_module};

fn load(module: &str, text: &str, imports: &ImportEnv) -> LoadedModule {
    let result = load_module(
        &ModuleSource::new(
            module,
            pith_diag::SourceId::from_raw(1),
            format!("{module}.pi"),
            text,
        ),
        imports,
    );
    let diagnostics = result
        .as_ref()
        .err()
        .map_or_else(String::new, |diagnostics| {
            diagnostics
                .iter()
                .map(|diagnostic| {
                    let (line, column) = diagnostic
                        .source
                        .as_ref()
                        .map_or((0, 0), |source| source.line_col(diagnostic.span.start));
                    format!("{line}:{column}: {}", diagnostic.message.0)
                })
                .collect::<Vec<_>>()
                .join("; ")
        });
    assert!(result.is_ok(), "{module}: {diagnostics}");
    match result {
        Ok(loaded) => loaded,
        Err(_) => unreachable!(),
    }
}

fn digests(table: &DeclarationTable) -> Vec<(Box<str>, DeclarationDigest)> {
    table
        .iter()
        .map(|declaration| {
            (
                declaration.coordinate().spelling().into(),
                declaration.digest(),
            )
        })
        .collect()
}

#[test]
fn live_and_surface_declaration_tables_agree() {
    let xylem = load(
        "xylem",
        include_str!("../../xylem/xylem.pi"),
        &ImportEnv::new(),
    );
    let mut xylem_import = ImportEnv::new();
    xylem_import.insert_loaded(&xylem);
    let phloem = load(
        "phloem",
        include_str!("../../phloem/phloem.pi"),
        &xylem_import,
    );
    let stele = load(
        "stele",
        include_str!("../../stele/stele.pi"),
        &ImportEnv::new(),
    );
    assert_eq!(digests(xylem.table()), digests(xylem::types::table()));
    assert_eq!(
        digests(phloem.table()),
        phloem::declarations::registered()
            .into_iter()
            .map(|(name, digest)| (name.into(), digest))
            .collect::<Vec<_>>()
    );
    assert_eq!(digests(stele.table()), digests(stele::types::table()));
}

#[test]
fn xylem_rule_revisions_agree() {
    let loaded = load(
        "xylem",
        include_str!("../../xylem/xylem.pi"),
        &ImportEnv::new(),
    );
    let toolchains = xylem::Toolchains::new(Box::new([]));
    let universe = xylem::HeaderUniverse::new(Box::new([]));
    let live_actions = [
        (
            "discover",
            xylem::HeaderDiscoveryAction::new(toolchains.clone(), universe.clone())
                .rule()
                .revision,
        ),
        (
            "compile",
            xylem::CompileAction::new(toolchains.clone(), universe)
                .rule()
                .revision,
        ),
        (
            "link",
            xylem::LinkAction::new(toolchains.clone()).rule().revision,
        ),
        (
            "generate",
            xylem::GenerateAction::new(toolchains.clone())
                .rule()
                .revision,
        ),
        ("test", xylem::TestAction::new(toolchains).rule().revision),
    ];
    for (label, live_revision) in live_actions {
        let declaration = loaded
            .action_rule(label)
            .unwrap_or_else(|| unreachable!("xylem.pi declares no action rule `{label}`"));
        assert_eq!(declaration.rule(BodyRevision(1)).revision, live_revision);
    }

    let live_pure = [
        ("compile-entry", xylem::CompileRule::rule().revision),
        ("link-entry", xylem::LinkRule::rule().revision),
        ("generate-entry", xylem::GenerateRule::rule().revision),
        ("test-entry", xylem::TestRule::rule().revision),
    ];
    for (label, live_revision) in live_pure {
        let declaration = loaded
            .pure_rule(label)
            .unwrap_or_else(|| unreachable!("xylem.pi declares no pure rule `{label}`"));
        assert_eq!(declaration.rule(BodyRevision(1)).revision, live_revision);
    }
}

#[test]
fn artifact_only_edits_do_not_move_the_abi() {
    let base = "nominal A = Text\nnominal B = Blob\npure rule f(A) -> B = host\n";
    let edited = "-- documentation\nnominal B = Blob\n\nnominal A = Text\npure rule renamed(A) -> B = host\n";
    let base = load("test", base, &ImportEnv::new());
    let edited = load("test", edited, &ImportEnv::new());
    assert_ne!(base.artifact_id(), edited.artifact_id());
    assert_eq!(base.abi_digest(), edited.abi_digest());
    assert_eq!(
        base.artifact_id(),
        ContentId::of_blob(base.source().source_text().as_bytes())
    );
}

#[test]
fn semantic_surface_edits_move_the_abi() {
    let dependency = load("dep", "nominal A = Text\n", &ImportEnv::new());
    let mut imports = ImportEnv::new();
    imports.insert_loaded(&dependency);
    let base = load(
        "test",
        "import dep\nnominal A = Text\npure rule f(A) -> A = host\n",
        &imports,
    );
    let representation = load(
        "test",
        "import dep\nnominal A = Blob\npure rule f(A) -> A = host\n",
        &imports,
    );
    let interface = load(
        "test",
        "import dep\nnominal A = Text\npure rule f(A, A) -> A = host\n",
        &imports,
    );
    let category = load(
        "test",
        "import dep\nnominal A = Text\naction rule f(A) -> A = host\n",
        &imports,
    );
    assert_ne!(base.abi_digest(), representation.abi_digest());
    assert_ne!(base.abi_digest(), interface.abi_digest());
    assert_ne!(base.abi_digest(), category.abi_digest());

    let changed_dependency = load("dep", "nominal A = Blob\n", &ImportEnv::new());
    let mut changed_imports = ImportEnv::new();
    changed_imports.insert_loaded(&changed_dependency);
    let imported = load(
        "test",
        "import dep\nnominal A = Text\npure rule f(A) -> A = host\n",
        &changed_imports,
    );
    assert_ne!(base.abi_digest(), imported.abi_digest());
}
