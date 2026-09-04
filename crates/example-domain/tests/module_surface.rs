use pith_core::{BodyRevision, Request};
use pith_diag::{SourceId, Span};
use pith_engine::Engine;
use pith_ids::ContentId;
use pith_loader::{ImportEnv, ModuleSource, load_module};

fn loaded_module() -> pith_loader::LoadedModule {
    let source = ModuleSource::new(
        "example",
        SourceId::from_raw(1),
        "example.pi",
        include_str!("../example.pi"),
    );
    let result = load_module(&source, &ImportEnv::new());
    assert!(result.is_ok(), "example.pi did not elaborate");
    match result {
        Ok(loaded) => loaded,
        Err(_) => unreachable!(),
    }
}

#[test]
fn the_surface_matches_the_live_table() {
    let loaded = loaded_module();
    let surface_digests = loaded
        .table()
        .iter()
        .map(|declaration| (declaration.coordinate().clone(), declaration.digest()))
        .collect::<Vec<_>>();
    let live_digests = example_domain::types::table()
        .iter()
        .map(|declaration| (declaration.coordinate().clone(), declaration.digest()))
        .collect::<Vec<_>>();
    assert_eq!(surface_digests, live_digests);
}

#[test]
fn typed_declarations_bind_the_host_body_and_register_the_represented_entry() {
    let loaded = loaded_module();
    let pure = loaded
        .represented_pure_rule("render-entry")
        .unwrap_or_else(|| unreachable!("example.pi declares no represented `render-entry`"));
    let action = loaded
        .action_rule("render")
        .unwrap_or_else(|| unreachable!("example.pi declares no action rule `render`"));
    let mut engine = Engine::new();
    pure.register(&mut engine)
        .unwrap_or_else(|error| unreachable!("the represented body does not register: {error}"));
    action.bind(&mut engine, BodyRevision(1), example_domain::RenderAction);

    let request = Request::new(
        "render-entry",
        pure.interface().clone(),
        [
            example_domain::types::renderer().content(ContentId::of_blob(b"renderer")),
            example_domain::types::template().content(ContentId::of_blob(b"template")),
            example_domain::types::bindings_value([] as [(&str, &str); 0]),
        ],
        Span::none(),
    );
    let query = engine.query();
    let selected = query
        .select(&request)
        .unwrap_or_else(|diag| unreachable!("{}", diag.message.0));
    let selected_rule = query
        .rule(selected.rule)
        .unwrap_or_else(|| unreachable!("selection returned a rule the engine does not hold"));
    assert_eq!(selected_rule.label.as_ref(), "render-entry");
}
