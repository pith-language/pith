//! The module-linkage boundary: `.pi` text in, declarations and rules out,
//! bound onto an engine through its public registration calls. The kernel
//! never resolves imports; module and interface linkage lives here.

mod bind;
mod graph;
mod import;
mod load;
mod loaded;
pub mod registry;
mod resolve;
mod source;
mod tree;
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
    DefinitionKind, DefinitionLocation, FrontendCode, InputLocator, InputsBlock, MissingSubject,
    ModuleSubject, ModuleVersion, PositionSidecar, Project, ProjectDomain, ProjectHeader,
    ProjectHost, ProjectInclude, ProjectInput, ProjectRegistry, ReferenceSite, RootKey,
    RuleCategory, SubjectSegment, VersionBound, VersionRange,
};
pub use resolve::{
    Derivation, MODULES_MODULE, ModuleCandidate, ModuleConstraint, ModuleRequirement,
    ModuleResolution, ModuleSelection, ModuleUniverse, Preference, RegisterModuleResolver,
    SolveRequest, TrailEntry, UnknownPreference, resolve_request, resolver_revision_hex,
    resolver_rule, selections_from_value, solve,
};
pub use source::{
    InvalidProject, ModuleSource, ParsedModule, ParsedProjectFile, ProjectSource, format_module,
    format_project_file, parse_module, parse_module_sources, parse_project_file,
    parse_project_files,
};
pub use tree::{Measured, measure};
pub use workspace::{
    AcquireFailure, AcquiredProject, AcquiredSource, AdmissionClause, AdmissionRefusal,
    AdmissionRequest, AdmittedSource, BindingOrigin, BindingOverride, BindingPolicy, BindingSite,
    CheckedModule, Closure, ElaborateError, ElaboratedWorkspace, FrontendInputs,
    FrontendProjection, LOCK_NAME, LocalDirectory, LocalFiles, LockWriteError, ModuleFile, Outcome,
    PROJECT_NAME, ProjectFiles, ProjectStore, ProjectedModule, ProjectionError, PublishedInterface,
    RegistryRoute, ResolvedModule, RootFiles, Route, SourceSet, UserBindings, Workspace,
    WorkspaceCheck, WorkspacePass, WorkspaceResolution, admit_source, write_lock,
};
