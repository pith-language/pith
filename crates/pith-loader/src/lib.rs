//! The module-linkage boundary: `.pi` text in, declarations and rules out,
//! bound onto an engine through its public registration calls. The kernel
//! never resolves imports (decisions 0061, 0038); this crate is where module
//! and interface linkage lives.

mod bind;
mod graph;
mod import;
mod load;
mod loaded;
mod source;

pub use bind::{
    EntryDeclaration, HostRuleDeclaration, RepresentedRuleDeclaration, RuleDeclaration,
};
pub use graph::{
    ELABORATOR_SEMANTIC_VERSION, FrontendImport, FrontendImportEnv, FrontendInputError,
    FrontendSource, InterfaceSurface, RegisterFrontend, bodies_of_request, index_of_request,
    interface_of_request,
};
pub use import::{BUILTIN_MODULE, ImportEnv, exec_type};
pub use load::{elaborate_module, load_module};
pub use loaded::LoadedModule;
pub use pith_elaborator::{GRAMMAR_VERSION, ImportedModule};
pub use pith_hir::{
    DefinitionKind, DefinitionLocation, FrontendCode, PositionSidecar, ReferenceSite, RuleCategory,
};
pub use source::{ModuleSource, ParsedModule, format_module, parse_module};
