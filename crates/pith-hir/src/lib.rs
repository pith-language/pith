mod merge;
mod position;
mod surface;

use pith_diag::StableCode;

pub use merge::{MergedModule, ModuleFiles, merge_module_files};
pub use position::{DefinitionKind, DefinitionLocation, PositionSidecar, ReferenceSite};
pub use surface::{
    ParsedSurface, SurfaceBody, SurfaceConstructor, SurfaceDeclaration, SurfaceField,
    SurfaceImport, SurfaceRule, SurfaceTypeArena, SurfaceTypeId, SurfaceTypeNode,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuleCategory {
    Pure,
    Action,
}

impl RuleCategory {
    #[must_use]
    pub const fn abi_tag(self) -> u8 {
        match self {
            Self::Pure => 0,
            Self::Action => 1,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FrontendCode {
    UnexpectedToken = 1,
    InvalidString = 2,
    DuplicateDeclaration = 3,
    DuplicateField = 4,
    RecursiveAlias = 5,
    CyclicDeclaration = 6,
    UnknownName = 7,
    UnknownImport = 8,
    MissingRule = 9,
    DuplicateImport = 10,
    DuplicateRule = 11,
    DuplicateInterface = 12,
    UndeclaredQualifiedAccess = 13,
    SourceNotUtf8 = 14,
    MalformedSurface = 15,
}

impl FrontendCode {
    #[must_use]
    pub const fn stable(self) -> StableCode {
        StableCode::frontend(self as u32)
    }
}
