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
    let text = base
        .files()
        .sources()
        .first()
        .map(|source| source.source_text().as_bytes().to_vec())
        .unwrap_or_default();
    assert_eq!(base.artifact_id(), ContentId::of_blob(&text));
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

#[test]
fn renaming_an_import_alias_leaves_every_digest_it_can_unchanged() {
    let dependency = load(
        "example/dep",
        "nominal Message = Text\npure rule greet(Message) -> Text = host\n",
        &ImportEnv::new(),
    );
    let mut through_binding = ImportEnv::new();
    through_binding.insert_alias("dep", &dependency);
    let mut through_alias = ImportEnv::new();
    through_alias.insert_alias("greeting", &dependency);

    let consumer = |binding: &str| {
        load(
            "example/consumer",
            &format!(
                "import {binding}\npure rule render({binding}.Message) -> Text = {{ ask Text (\"x\") }}\n"
            ),
            if binding == "dep" {
                &through_binding
            } else {
                &through_alias
            },
        )
    };
    let named = consumer("dep");
    let aliased = consumer("greeting");

    assert_eq!(
        named.abi_digest(),
        aliased.abi_digest(),
        "an alias edit moved the semantic ABI"
    );
    assert_eq!(
        named.interface_surface().encode(),
        aliased.interface_surface().encode(),
        "an alias edit moved the interface surface"
    );
    let body = |loaded: &LoadedModule| {
        loaded
            .represented_pure_rule("render")
            .map(|rule| rule.digest())
    };
    assert_eq!(body(&named), body(&aliased));
    assert_eq!(
        named
            .pure_rule("render")
            .map(|rule| rule.coordinate().clone()),
        aliased
            .pure_rule("render")
            .map(|rule| rule.coordinate().clone())
    );
}

#[test]
fn two_aliases_for_one_subject_produce_one_decodable_surface() {
    let dependency = load("example/dep", "nominal Message = Text\n", &ImportEnv::new());
    let mut single = ImportEnv::new();
    single.insert_alias("dep", &dependency);
    let mut double = ImportEnv::new();
    double.insert_alias("dep", &dependency);
    double.insert_alias("also_dep", &dependency);

    let once = load(
        "example/consumer",
        "import dep\nnominal W = dep.Message\n",
        &single,
    );
    let twice = load(
        "example/consumer",
        "import dep\nimport also_dep\nnominal W = dep.Message\n",
        &double,
    );

    let encoded = twice.interface_surface().encode();
    let decoded = pith_loader::InterfaceSurface::decode(&encoded)
        .unwrap_or_else(|error| unreachable!("the two-alias surface decodes: {error}"));
    assert_eq!(
        decoded.encode(),
        encoded,
        "decode re-encodes the same bytes"
    );

    // The imports half is the canonical set: one entry per imported
    // subject, whichever aliases reached it, the same form the ABI takes.
    let expected = [(dependency.module().into(), dependency.abi_digest())];
    assert_eq!(twice.interface_surface().imports(), &expected);
    assert_eq!(once.interface_surface().imports(), &expected);
    // The ABI agrees: a second binding for one subject, changing no
    // declarations, leaves the digest alone.
    assert_eq!(
        twice.abi_digest(),
        once.abi_digest(),
        "a second alias for one subject moved the ABI"
    );
}
