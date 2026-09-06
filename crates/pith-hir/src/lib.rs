mod body;
mod manifest;
mod merge;
mod position;
mod surface;

use pith_diag::StableCode;

pub use body::{
    SurfaceArm, SurfaceBatchMember, SurfaceBinder, SurfaceClause, SurfaceExpr, SurfaceExprArena,
    SurfaceExprId, SurfaceOperator, SurfaceRequest, SurfaceStatement, SurfaceValue,
    SurfaceValueField, SurfaceWrittenBody,
};
pub use manifest::{
    DependencySource, EmptyVersion, InvalidVersionSpelling, Manifest, ManifestDomain,
    ManifestMember, ManifestModule, ManifestRegistry, ManifestUse, ManifestVersion,
    ManifestWorkspace, MissingModule, ModuleSubject, ParsedManifest, RootKey, RootKeyError,
    SegmentError, SubjectError, SubjectSegment, VersionBound, VersionRange,
};
pub use merge::{MergedModule, ModuleFiles, merge_module_files};
pub use position::{DefinitionKind, DefinitionLocation, PositionSidecar, ReferenceSite};
pub use surface::{
    ParsedSurface, SurfaceAbout, SurfaceAboutValue, SurfaceBody, SurfaceComment,
    SurfaceConstructor, SurfaceDeclaration, SurfaceEntry, SurfaceField, SurfaceImport,
    SurfaceLocal, SurfaceParam, SurfaceRule, SurfaceRuleBody, SurfaceTypeArena, SurfaceTypeId,
    SurfaceTypeNode,
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
    HeadlessRequest = 16,
    BuiltinShadowed = 17,
    DuplicateEntry = 18,
    DuplicateLocal = 19,
    OutOfOrderLocal = 20,
    SelfRequest = 21,
    InvalidBody = 22,
    DuplicateArm = 23,
    FilterAfterBinding = 24,
    TypeMismatch = 25,
    DuplicateBinder = 26,
    WrongDocument = 27,
    DuplicateManifestClause = 28,
    InvalidSubject = 29,
    InvalidVersion = 30,
    UnsupportedSource = 31,
    InvalidPath = 32,
    DuplicateBinding = 33,
    DuplicateMember = 34,
    MissingManifest = 35,
    NestedWorkspace = 36,
    DuplicateSubject = 37,
    DependencyCycle = 38,
    SymlinkedSource = 39,
    EmptySourceSet = 40,
    SubjectMismatch = 41,
    UnreadableSource = 42,
    IrregularSource = 43,
    DuplicateRegistry = 44,
    InvalidRootKey = 45,
    DependencySuppliedAuthority = 46,
    UnroutedDomain = 47,
    UnenrolledKey = 48,
    UnsignedKeySet = 49,
    KeySetRollback = 50,
    IndexFork = 51,
    WrongIndexCache = 52,
    ContentMismatch = 53,
    IndexUnreachable = 54,
    WithdrawnSelection = 55,
    PolicyRefusal = 56,
    UnpublishablePath = 57,
    InvalidRange = 58,
    RangeWithoutResolution = 59,
    IncompleteSource = 60,
    ConflictingRoutes = 61,
    DuplicateVersion = 62,
    UnexpectedSubject = 63,
    Unsatisfiable = 64,
    SearchExhausted = 65,
    StaleLockEntry = 66,
    ModeExceeded = 67,
    MalformedResolution = 68,
}

impl FrontendCode {
    #[must_use]
    pub const fn stable(self) -> StableCode {
        StableCode::frontend(self as u32)
    }
}
