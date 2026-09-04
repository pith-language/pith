use std::assert_matches;

use pith_core::Value;
use pith_engine::{Engine, EvaluationSource, MemoryEngineStateStore};
use pith_ids::{ContentId, ModuleAbiDigest};
use pith_loader::{
    FrontendImport, FrontendImportEnv, FrontendInputError, FrontendSource, ImportEnv,
    InterfaceSurface, LoadedModule, ModuleSource, RegisterFrontend, bodies_of_request,
    interface_of_request, load_module,
};
use pith_store::MemoryContentStore;

const ALPHA: &str = "\
-- Documents the object type a dependent elaborates against.
nominal Object = Blob

pure rule \"objects-of\"(List<Blob>) -> List<Object> = host
";

const ALPHA_DOC_EDIT: &str = "\
-- Documents the object type a dependent elaborates against.
-- A second line no consumer can observe.

nominal Object = Blob

pure rule \"objects-of\"(List<Blob>) -> List<Object> = host
";

const ALPHA_LABEL_EDIT: &str = "\
-- Documents the object type a dependent elaborates against.
nominal Object = Blob

pure rule \"renamed-objects-of\"(List<Blob>) -> List<Object> = host
";

/// The same declaration with a represented body where `host` was: a semantic
/// edit the interface surface cannot see, because a body rides no interface
/// and no declaration digest. This is the cutoff witness M-12 waited for —
/// 0063's unresolved section names it, and M-13's notation is what makes the
/// edit expressible.
const ALPHA_BODY_EDIT: &str = "\
-- Documents the object type a dependent elaborates against.
nominal Object = Blob

pure rule \"objects-of\"(sources: List<Blob>) -> List<Object> = {
  let wrapped : List<Object> = fold sources from [] {
    (source, objects) -> append([Object(source)], objects)
  }
  wrapped
}
";

const ALPHA_REPRESENTATION_EDIT: &str = "\
-- Documents the object type a dependent elaborates against.
nominal Object = Text

pure rule \"objects-of\"(List<Blob>) -> List<Object> = host
";

const BETA: &str = "\
import alpha

pure rule \"wrap\"(alpha.Object) -> alpha.Object = host
";

const GAMMA_TYPES: &str = "nominal Label = Text\n";

const GAMMA_RULES: &str = "pure rule \"label-of\"(Label) -> Label = host\n";

const BROKEN: &str = "pure rule \"leak\"(Missing) -> Blob = host\n";

struct Driver {
    engine: Engine,
}

#[derive(Clone, Copy)]
struct PublishedInterface {
    abi: ModuleAbiDigest,
    surface: ContentId,
    source: ContentId,
}

impl Driver {
    fn new(state: MemoryEngineStateStore) -> Self {
        let mut engine = Engine::with_state_store(MemoryContentStore::default(), state);
        engine.register_frontend();
        Self { engine }
    }

    fn publish(&mut self, bytes: &[u8]) -> ContentId {
        match self.engine.put_blob(bytes) {
            Ok(id) => id,
            Err(_) => unreachable!("the memory content store is infallible"),
        }
    }

    fn load_module(module: &str, text: &str) -> LoadedModule {
        match load_module(
            &ModuleSource::new(
                module,
                pith_diag::SourceId::from_raw(0),
                format!("{module}.pi"),
                text,
            ),
            &ImportEnv::new(),
        ) {
            Ok(loaded) => loaded,
            Err(_) => unreachable!("the fixture module elaborates"),
        }
    }

    fn publish_interface(&mut self, module: &str, text: &str) -> PublishedInterface {
        let source = self.publish(text.as_bytes());
        let request = interface_of_request(
            frontend_source(module, [(format!("{module}.pi").into(), source)]),
            frontend_imports([]),
        );
        let evaluation = match self.engine.evaluate_with_content(&request) {
            Ok(evaluation) => evaluation,
            Err(diagnostics) => unreachable!("interface-of failed: {diagnostics:?}"),
        };
        let representation = representation_of(evaluation.value);
        let Value::Nominal {
            representation: abi,
            ..
        } = field_of(&representation, "abi")
        else {
            unreachable!("the ABI field is nominal");
        };
        let Value::Blob(abi) = abi.as_ref() else {
            unreachable!("the ABI representation is a digest");
        };
        let Value::Bytes(surface_bytes) = field_of(&representation, "surface") else {
            unreachable!("the surface field is encoded bytes");
        };
        let surface = match InterfaceSurface::decode(surface_bytes) {
            Ok(surface) => surface,
            Err(error) => unreachable!("interface-of returned a malformed surface: {error}"),
        };
        let published = self.publish(surface_bytes);
        assert_eq!(published, surface.content_id());
        assert_eq!(abi.digest(), surface.abi_digest().digest());
        PublishedInterface {
            abi: surface.abi_digest(),
            surface: published,
            source,
        }
    }
}

fn frontend_source(
    module: &str,
    files: impl IntoIterator<Item = (Box<str>, ContentId)>,
) -> FrontendSource {
    match FrontendSource::new(module, files) {
        Ok(source) => source,
        Err(error) => unreachable!("the fixture source is canonical: {error}"),
    }
}

fn frontend_imports(entries: impl IntoIterator<Item = FrontendImport>) -> FrontendImportEnv {
    match FrontendImportEnv::new(entries) {
        Ok(imports) => imports,
        Err(error) => unreachable!("the fixture imports are canonical: {error}"),
    }
}

fn frontend_import(interface: PublishedInterface) -> FrontendImport {
    FrontendImport::new("alpha", "alpha", interface.abi, interface.surface)
}

fn bodies_of(
    engine: &mut Engine,
    module: &str,
    files: &[(Box<str>, ContentId)],
    imports: FrontendImportEnv,
) -> pith_diag::PithResult<pith_engine::Evaluation> {
    let request = bodies_of_request(frontend_source(module, files.iter().cloned()), imports);
    engine.evaluate_with_content(&request)
}

fn representation_of(value: Value) -> Value {
    let Value::Nominal { representation, .. } = value else {
        unreachable!("a graph output is the frontend nominal");
    };
    *representation
}

fn field_of<'a>(representation: &'a Value, name: &str) -> &'a Value {
    let Value::Record(fields) = representation else {
        unreachable!("a graph output's representation is a record");
    };
    fields
        .iter()
        .find(|field| field.name.as_ref() == name)
        .map_or(&Value::Unit, |field| &field.payload)
}

fn diagnostics_of(representation: &Value) -> &[Value] {
    match field_of(representation, "diagnostics") {
        Value::List(diagnostics) => diagnostics,
        _ => unreachable!("the diagnostics field is a list"),
    }
}

#[test]
fn the_surface_artifact_round_trips_and_moves_only_for_semantic_edits() {
    let surface = Driver::load_module("alpha", ALPHA).interface_surface();
    assert_eq!(
        InterfaceSurface::decode(&surface.encode()),
        Ok(surface.clone()),
        "the artifact did not round-trip"
    );

    let doc_edited = Driver::load_module("alpha", ALPHA_DOC_EDIT).interface_surface();
    assert_eq!(
        doc_edited.encode(),
        surface.encode(),
        "a documentation edit moved the interface surface"
    );
    assert_eq!(
        doc_edited.abi_digest(),
        surface.abi_digest(),
        "a documentation edit moved the ABI digest"
    );
    let label_edited = Driver::load_module("alpha", ALPHA_LABEL_EDIT).interface_surface();
    assert_eq!(label_edited.encode(), surface.encode());

    let representation_edited =
        Driver::load_module("alpha", ALPHA_REPRESENTATION_EDIT).interface_surface();
    assert_ne!(
        representation_edited.encode(),
        surface.encode(),
        "a representation edit left the interface surface unchanged"
    );
    assert_ne!(
        representation_edited.abi_digest(),
        surface.abi_digest(),
        "a representation edit left the ABI digest unchanged"
    );
}

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
    let types_blob = driver.publish(GAMMA_TYPES.as_bytes());
    let rules_blob = driver.publish(GAMMA_RULES.as_bytes());
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

#[test]
fn frontend_inputs_have_one_canonical_order_and_refuse_duplicate_keys() {
    let first = ContentId::of_blob(b"first");
    let second = ContentId::of_blob(b"second");
    let sorted = frontend_source("module", [("a.pi".into(), first), ("b.pi".into(), second)]);
    let reversed = frontend_source("module", [("b.pi".into(), second), ("a.pi".into(), first)]);
    assert_eq!(sorted, reversed);
    assert_matches!(
        FrontendSource::new(
            "module",
            [("same.pi".into(), first), ("same.pi".into(), second)]
        ),
        Err(FrontendInputError::DuplicateSourcePath { path }) if path.as_ref() == "same.pi"
    );

    let alpha = Driver::load_module("alpha", ALPHA).interface_surface();
    let alpha_import =
        FrontendImport::new("alpha", "alpha", alpha.abi_digest(), alpha.content_id());
    let duplicate = FrontendImport::new("alpha", "other", alpha.abi_digest(), alpha.content_id());
    assert_matches!(
        FrontendImportEnv::new([alpha_import, duplicate]),
        Err(FrontendInputError::DuplicateImportBinding { binding })
            if binding.as_ref() == "alpha"
    );
}
