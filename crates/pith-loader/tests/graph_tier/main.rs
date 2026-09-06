//! The frontend graph tier: the surface artifact, the bodies the cutoff
//! reuses, and the canonical order of the tier's inputs.
//!
//! [`interface`] holds the surface artifact's round trips and the canonical
//! input order; [`bodies`] holds the cutoff — a body edit moving `bodies-of`
//! while the interface and every dependent's key stay put. Both drive the
//! tier through the `Driver` here, which registers the frontend rules the
//! way a caller would.

mod bodies;
mod interface;

use pith_core::Value;
use pith_engine::{Engine, MemoryEngineStateStore};
use pith_ids::{ContentId, ModuleAbiDigest};
use pith_loader::{
    FrontendImport, FrontendImportEnv, FrontendSource, ImportEnv, InterfaceSurface, LoadedModule,
    ModuleSource, RegisterFrontend, bodies_of_request, interface_of_request, load_module,
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

/// What one published module hands its dependents.
#[derive(Clone, Copy)]
struct PublishedInterface {
    abi: ModuleAbiDigest,
    surface: ContentId,
    source: ContentId,
}

/// An engine with the frontend rules registered, and the publishing helpers
/// the tests drive it with.
struct Driver {
    engine: Engine,
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
