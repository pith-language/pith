mod artifact;
mod rules;
mod values;

pub use artifact::InterfaceSurface;
pub use values::{
    FRONTEND_MODULE, FrontendImport, FrontendImportEnv, FrontendInputError, FrontendSource,
};

use pith_core::manifest::encode_str;
use pith_core::{
    BODY_ENCODING_VERSION, Interface, Pure, Request, Rule, RuleIdentity, RuleRevision, Type,
};
use pith_diag::Span;

use rules::Projection;

pub const ELABORATOR_SEMANTIC_VERSION: u32 = 2;

pub trait RegisterFrontend {
    fn register_frontend(&mut self);
}

impl RegisterFrontend for pith_engine::Engine {
    fn register_frontend(&mut self) {
        let interface_of = frontend_rule("interface-of", values::module_interface_type());
        self.register_rule(
            interface_of,
            rules::FrontendRule::new(Projection::Interface),
        );
        let bodies_of = frontend_rule("bodies-of", values::bodies_type());
        self.register_rule(bodies_of, rules::FrontendRule::new(Projection::Bodies));
        let index_of = frontend_rule("index-of", values::index_type());
        self.register_rule(index_of, rules::FrontendRule::new(Projection::Index));
    }
}

fn frontend_rule(label: &str, output: Type) -> Rule<Pure> {
    let interface = Interface {
        inputs: Box::new([values::source_type(), values::import_env_type()]),
        output,
    };
    let identity = RuleIdentity::of_module_declaration(FRONTEND_MODULE, label);
    let revision = RuleRevision::of_manifest(identity, &elaborator_revision_manifest(&interface));
    Rule::new(FRONTEND_MODULE, revision, label, interface, Span::none())
}

fn elaborator_revision_manifest(interface: &Interface) -> Vec<u8> {
    let mut manifest = ELABORATOR_SEMANTIC_VERSION.to_le_bytes().to_vec();
    encode_str(&mut manifest, env!("CARGO_PKG_VERSION"));
    manifest.push(BODY_ENCODING_VERSION);
    manifest.extend_from_slice(&interface.encode_canonical());
    manifest
}

#[must_use]
pub fn interface_of_request(source: FrontendSource, imports: FrontendImportEnv) -> Request<Pure> {
    graph_request(
        "interface-of",
        values::module_interface_type(),
        source,
        imports,
    )
}

#[must_use]
pub fn bodies_of_request(source: FrontendSource, imports: FrontendImportEnv) -> Request<Pure> {
    graph_request("bodies-of", values::bodies_type(), source, imports)
}

#[must_use]
pub fn index_of_request(source: FrontendSource, imports: FrontendImportEnv) -> Request<Pure> {
    graph_request("index-of", values::index_type(), source, imports)
}

fn graph_request(
    label: &str,
    output: Type,
    source: FrontendSource,
    imports: FrontendImportEnv,
) -> Request<Pure> {
    Request::new(
        label,
        Interface {
            inputs: Box::new([values::source_type(), values::import_env_type()]),
            output,
        },
        [source.into_value(), imports.into_value()],
        Span::none(),
    )
}
