//! DTO projections of pith-core's value and type IR. Lives here so pith-core
//! does not depend on serde.

#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "value_kind", rename_all = "snake_case")]
pub enum ValueRepr {
    Unit,
    Bool {
        b: bool,
    },
    /// Decimal text preserves integers that JSON number consumers would round.
    Int {
        decimal: Box<str>,
    },
    Text {
        s: Box<str>,
    },
    Bytes {
        len: u64,
    },
    Blob {
        digest: Box<str>,
    },
    Nominal {
        name: Box<str>,
        representation: Box<ValueRepr>,
    },
    List {
        elements: Box<[ValueRepr]>,
    },
    Record {
        fields: Box<[(Box<str>, ValueRepr)]>,
    },
    Sum {
        name: Box<str>,
        constructor: Box<str>,
        payload: Option<Box<ValueRepr>>,
    },
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "type_kind", rename_all = "snake_case")]
pub enum TypeRepr {
    Unit,
    Bool,
    Int,
    Text,
    Bytes,
    Blob,
    Nominal {
        name: Box<str>,
    },
    List {
        element: Box<TypeRepr>,
    },
    Record {
        fields: Box<[(Box<str>, TypeRepr)]>,
    },
    Sum {
        name: Box<str>,
        constructors: Box<[SumConstructorRepr]>,
    },
    /// A recursion cut keeps the projected type finite.
    Cut,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct SumConstructorRepr {
    pub name: Box<str>,
    pub payload: Option<Box<TypeRepr>>,
}

/// The version of the DTO contract below. `--output json` is a machine
/// surface, so the shape a reader parses is versioned separately from the
/// binary: bump this when a DTO's shape changes, and never for a change that
/// only adds a command. The `query_view_shape_is_stable` snapshots are what
/// make the number mean something.
pub const QUERY_API_VERSION: u32 = 3;

/// A DTO projection of `pith_diag::Severity`. Mirrored here rather than
/// imported for the reason `ValueRepr` is: this crate holds serde and depends
/// on nothing else in the stack. The driver converts through an exhaustive
/// match, so a variant added upstream fails to compile rather than drifting.
#[derive(Copy, Clone, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SeverityRepr {
    Error,
    Warning,
    Info,
    Note,
}

/// One diagnostic, projected for a machine reader. The rendered form a person
/// sees still goes through miette at the CLI boundary; this carries the same
/// facts without the snippet.
#[derive(Clone, Debug, serde::Serialize)]
pub struct DiagnosticRepr {
    pub severity: SeverityRepr,
    /// The stable code (K-11). Never renumbered, only added to.
    pub code: u32,
    /// The source file the diagnostic points into, when it points into one.
    pub label: Option<Box<str>>,
    /// One-based line and column, absent for a diagnostic carrying no source.
    pub line: Option<u64>,
    pub column: Option<u64>,
    pub message: Box<str>,
}

/// What `pith check` found. Reported whether or not elaboration succeeded,
/// which is the property that keeps `check` outside the entry mechanism.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CheckReport {
    pub module: Box<str>,
    pub path: Box<str>,
    /// The module's semantic ABI digest, present only when it elaborated.
    pub abi_digest: Option<Box<str>>,
    pub diagnostics: Box<[DiagnosticRepr]>,
    pub errors: u64,
    pub warnings: u64,
}

/// What `pith fmt` did to one module. Like `check`, it works on source that
/// does not elaborate — but not on source that does not parse, which is a
/// refusal rather than a status.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FmtReport {
    pub module: Box<str>,
    pub path: Box<str>,
    pub status: FmtStatus,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FmtStatus {
    /// Already canonical; nothing was written.
    Unchanged,
    /// Not canonical, and written back.
    Formatted,
    /// Not canonical, and left alone because `--check` was asked for.
    WouldFormat,
}

/// The effect category written before a rule declaration.
#[derive(Copy, Clone, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleCategoryRepr {
    Pure,
    Action,
}

/// Whether a rule is implemented by the host or represented body IR.
#[derive(Copy, Clone, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TierRepr {
    Host,
    Represented,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct InterfaceRepr {
    pub inputs: Box<[TypeRepr]>,
    pub output: Box<TypeRepr>,
    /// The one-line spelling `Display for Interface` produces. The surface
    /// notation renders the interface literal wherever the short call-site
    /// form is written, so the rendered line travels with the structure.
    pub rendered: Box<str>,
}

/// A declaration's semantic body.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "declaration_kind", rename_all = "snake_case")]
pub enum DeclarationBodyRepr {
    Nominal {
        representation: Box<TypeRepr>,
    },
    Sum {
        constructors: Box<[SumConstructorRepr]>,
    },
    Alias {
        target: Box<TypeRepr>,
    },
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct DeclarationView {
    pub name: Box<str>,
    pub body: DeclarationBodyRepr,
    /// The declaration grammar's own spelling of the body, from
    /// `Display for DeclarationBody`. A reader gets the source form back
    /// rather than a second notation invented at the renderer.
    pub rendered: Box<str>,
    /// The declaration's own revision digest. Doc text does not participate,
    /// so editing a description leaves this where it was.
    pub digest: Box<str>,
    pub documentation: Box<str>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct RuleView {
    pub label: Box<str>,
    pub category: RuleCategoryRepr,
    pub tier: TierRepr,
    pub interface: InterfaceRepr,
    pub documentation: Box<str>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ImportView {
    pub module: Box<str>,
    pub abi_digest: Box<str>,
}

/// What `pith explore` shows: everything a module declares, and which tier
/// answers each rule.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ModuleView {
    pub module: Box<str>,
    pub path: Box<str>,
    pub abi_digest: Box<str>,
    pub imports: Box<[ImportView]>,
    pub declarations: Box<[DeclarationView]>,
    pub rules: Box<[RuleView]>,
}

/// One entry of a stored tree. Symlinks are stored rather than followed.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "entry_kind", rename_all = "snake_case")]
pub enum TreeEntryRepr {
    File {
        name: Box<str>,
        content: Box<str>,
        executable: bool,
    },
    Tree {
        name: Box<str>,
        content: Box<str>,
    },
    Symlink {
        name: Box<str>,
        /// The target as the manifest stores it, lossily rendered. The bytes
        /// are not required to be UTF-8, and the identity lives in the tree.
        target: Box<str>,
    },
}

/// A tree's entries, in the canonical name order the manifest fixes.
#[derive(Clone, Debug, serde::Serialize)]
pub struct TreeListing {
    pub tree: Box<str>,
    pub entries: Box<[TreeEntryRepr]>,
}

/// Content admitted to or read from the store, named by its identity.
#[derive(Clone, Debug, serde::Serialize)]
pub struct StoredContent {
    pub id: Box<str>,
    pub kind: StoredContentKind,
    /// The path the bytes came from or were written to, when there was one.
    pub path: Option<Box<str>>,
}

#[derive(Copy, Clone, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredContentKind {
    Blob,
    Tree,
}

/// Attempt counts by terminal state.
#[derive(Copy, Clone, Debug, Default, serde::Serialize)]
pub struct AttemptCounts {
    pub total: u64,
    pub pending: u64,
    pub complete: u64,
    pub failed: u64,
    pub cancelled: u64,
}

/// What `pith state info` reports about one engine-state database.
#[derive(Clone, Debug, serde::Serialize)]
pub struct StateInfo {
    /// The adapter holding the records, which is also who a repair question
    /// is addressed to.
    pub adapter: Box<str>,
    pub schema_version: u32,
    pub semantic_encoding_version: u32,
    pub attempts: AttemptCounts,
    /// Entries in the reusable index: one per computation key holding a
    /// completed reusable attempt.
    pub reusable_index: u64,
}

/// What `pith state check` reports: every durable record read back through
/// the decode validation an individual lookup applies.
#[derive(Clone, Debug, serde::Serialize)]
pub struct StateCheck {
    pub records: u64,
}

/// What `pith gc --dry-run` would reclaim when reusable-index entries are the
/// only roots. Reclaimable counts are upper bounds until additional retention
/// roots are configured.
#[derive(Clone, Debug, serde::Serialize)]
pub struct GcPreview {
    /// Reusable-index entries: the roots.
    pub roots: u64,
    /// Attempts reachable from the roots over recorded dependency edges.
    pub retained_attempts: u64,
    pub reclaimable_attempts: u64,
    pub content: ContentPreview,
}

/// Content retained directly by engine state or transitively through trees.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct ContentPreview {
    pub blobs: u64,
    pub trees: u64,
    pub live_blobs: u64,
    pub live_trees: u64,
    pub reclaimable_blobs: u64,
    pub reclaimable_trees: u64,
    /// On-disk sizes summed over the objects each field counts.
    pub total_bytes: u64,
    pub live_bytes: u64,
    pub reclaimable_bytes: u64,
}

/// Everything a query method can answer with. One tagged enum rather than a
/// payload variant per command, so adding a command extends this in one place
/// and `QUERY_API_VERSION` covers the whole surface.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "view", rename_all = "snake_case")]
pub enum QueryView {
    Check(CheckReport),
    Format(FmtReport),
    Module(ModuleView),
    Tree(TreeListing),
    Content(StoredContent),
    State(StateInfo),
    StateCheck(StateCheck),
    Gc(GcPreview),
}
