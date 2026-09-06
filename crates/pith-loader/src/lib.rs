//! The module-linkage boundary: `.pi` text in, declarations and rules out,
//! bound onto an engine through its public registration calls. The kernel
//! never resolves imports (decisions 0061, 0038); this crate is where module
//! and interface linkage lives.

mod bind;
mod graph;
mod import;
mod load;
mod loaded;
mod resolve;
mod source;
mod workspace;

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
    DefinitionKind, DefinitionLocation, DependencySource, FrontendCode, Manifest, ManifestDomain,
    ManifestRegistry, ManifestUse, ManifestVersion, MissingModule, ModuleSubject, ParsedManifest,
    PositionSidecar, ReferenceSite, RootKey, RuleCategory, SubjectSegment, VersionBound,
    VersionRange,
};
pub use resolve::{
    Derivation, MODULES_MODULE, ModuleCandidate, ModuleConstraint, ModuleRequirement,
    ModuleResolution, ModuleSelection, ModuleUniverse, Preference, RegisterModuleResolver,
    SolveRequest, TrailEntry, UnknownPreference, resolve_request, resolver_revision_hex,
    resolver_rule, selections_from_value, solve,
};
pub use source::{
    InvalidManifest, ManifestSource, ModuleSource, ParsedManifestFile, ParsedModule,
    format_manifest, format_module, parse_manifest, parse_module, parse_module_sources,
};
pub use workspace::{
    AcquireFailure, AcquiredManifest, AcquiredSource, AdmissionClause, AdmissionRefusal,
    AdmissionRequest, AdmittedSource, CheckedModule, Closure, ElaborateError, ElaboratedWorkspace,
    FrontendInputs, FrontendProjection, LocalDirectory, LocalFiles, MANIFEST_NAME, ModuleFile,
    ModuleStore, Outcome, ProjectedModule, ProjectionError, PublishedInterface, RegistryRoute,
    ResolvedModule, RootFiles, Route, SOURCE_DIRECTORY, SOURCE_SUFFIX, SourceSet, Workspace,
    WorkspaceCheck, WorkspacePass, admit_source,
};
