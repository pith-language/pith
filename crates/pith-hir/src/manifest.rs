//! The manifest document: a module's declared subject, its workspace
//! membership, and its path dependencies. The manifest is not a module — it
//! imports nothing and computes nothing — so it has no arenas and no request
//! grammar, only clauses with spans.

use std::fmt;

use pith_diag::Span;

use crate::surface::SurfaceComment;

#[derive(Clone)]
pub struct ParsedManifest {
    pub module: Option<ManifestModule>,
    pub workspace: Option<ManifestWorkspace>,
    pub uses: Box<[ManifestUse]>,
    /// Registry bindings and domain routes: consumer configuration, which
    /// parses in any manifest and carries authority only in a root's.
    pub registries: Box<[ManifestRegistry]>,
    pub domains: Box<[ManifestDomain]>,
    /// Every top-level line comment, including comments documenting no clause.
    pub comments: Box<[SurfaceComment]>,
}

impl ParsedManifest {
    /// The manifest this parse holds once its module clause is settled.
    ///
    /// # Errors
    /// Returns [`MissingModule`] when the parse holds no module clause.
    pub fn manifest(&self) -> Result<Manifest, MissingModule> {
        Manifest::try_from(self)
    }
}

impl TryFrom<&ParsedManifest> for Manifest {
    type Error = MissingModule;

    fn try_from(parsed: &ParsedManifest) -> Result<Self, Self::Error> {
        let module = parsed.module.clone().ok_or(MissingModule)?;
        Ok(Self {
            module,
            workspace: parsed.workspace.clone(),
            uses: parsed.uses.clone(),
            registries: parsed.registries.clone(),
            domains: parsed.domains.clone(),
        })
    }
}

/// The `module domain/name version` clause. Present exactly once in any
/// manifest that parses without errors.
#[derive(Clone)]
pub struct ManifestModule {
    pub subject: ModuleSubject,
    pub subject_span: Span,
    pub version: ManifestVersion,
    pub span: Span,
}

/// The `workspace { members: [...] }` clause. At most one per manifest, and a
/// member manifest cannot declare one.
#[derive(Clone)]
pub struct ManifestWorkspace {
    pub members: Box<[ManifestMember]>,
    pub span: Span,
}

#[derive(Clone)]
pub struct ManifestMember {
    /// A quoted path, relative to the declaring manifest.
    pub path: Box<str>,
    pub span: Span,
}

/// A `use alias = subject <range> from <source>` clause: one binding in the
/// owning module's import environment, and the route that satisfies it.
#[derive(Clone)]
pub struct ManifestUse {
    pub alias: Box<str>,
    pub alias_span: Span,
    pub subject: ModuleSubject,
    pub subject_span: Span,
    /// Which versions of the subject the clause admits. A path dependency is
    /// always [`VersionRange::Any`]: it is a live local input, so there is
    /// nothing to select among.
    pub range: VersionRange,
    pub range_span: Span,
    pub source: DependencySource,
    pub span: Span,
}

impl ManifestUse {
    /// The dependency's path and its span, for the one source kind that
    /// resolves against the local filesystem.
    #[must_use]
    pub fn path(&self) -> Option<(&str, Span)> {
        match &self.source {
            DependencySource::Path { path, span } => Some((path, *span)),
            DependencySource::Registry { .. }
            | DependencySource::Git { .. }
            | DependencySource::Archive { .. } => None,
        }
    }

    /// The span the clause's route is diagnosed at.
    #[must_use]
    pub const fn source_span(&self) -> Span {
        self.source.span()
    }
}

/// Where a dependency's content comes from.
///
/// The variants are the four source kinds M-14 admits, and they are a sum
/// rather than a string plus optional fields so that a route missing what
/// its kind requires — a git revision, an archive digest — has no value.
#[derive(Clone)]
pub enum DependencySource {
    /// The subject's configured registry: the default when a clause writes
    /// no `from`, and the only route that consults a version universe.
    Registry {
        /// A named registry, when the clause selects one explicitly rather
        /// than taking the domain's configured route.
        registry: Option<Box<str>>,
        span: Span,
    },
    /// A directory holding a `module.pi`, relative to the declaring
    /// manifest. Live and unwitnessed.
    Path { path: Box<str>, span: Span },
    /// One module at one immutable revision. Never an index: a written range
    /// is checked against the acquired manifest rather than used to search.
    Git {
        url: Box<str>,
        revision: Box<str>,
        /// The subpath holding the module root, when the repository holds
        /// more than the module.
        subpath: Option<Box<str>>,
        span: Span,
    },
    /// One module's bytes, admitted against a digest written beside them.
    Archive {
        url: Box<str>,
        digest: Box<str>,
        span: Span,
    },
}

impl DependencySource {
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Registry { span, .. }
            | Self::Path { span, .. }
            | Self::Git { span, .. }
            | Self::Archive { span, .. } => *span,
        }
    }

    /// The word a diagnostic names this route by.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Registry { .. } => "registry",
            Self::Path { .. } => "path",
            Self::Git { .. } => "git",
            Self::Archive { .. } => "archive",
        }
    }

    /// Whether the route selects among versions. Only a registry does; the
    /// other three name one module, so a range on them constrains nothing.
    #[must_use]
    pub const fn resolves_versions(&self) -> bool {
        matches!(self, Self::Registry { .. })
    }
}

/// A `registry name = "locator" root "key"` clause: consumer configuration
/// appointing a registry and pinning the root key that vouches for its
/// domain key sets.
#[derive(Clone)]
pub struct ManifestRegistry {
    pub name: Box<str>,
    pub name_span: Span,
    /// Where the index is read from. A hint under 0069: overridable, and
    /// outside every identity.
    pub locator: Box<str>,
    pub locator_span: Span,
    pub root_key: RootKey,
    pub root_key_span: Span,
    pub span: Span,
}

/// A `domain <domain> from <registry>` clause: which registry answers for a
/// domain. There is no search order and no fallback, so a domain with no
/// clause does not resolve.
#[derive(Clone)]
pub struct ManifestDomain {
    pub domain: SubjectSegment,
    pub domain_span: Span,
    pub registry: Box<str>,
    pub registry_span: Span,
    pub span: Span,
}

/// A pinned root key: an algorithm name and its encoded material, spelled
/// `algorithm:material`. Constructed only through validation, so a clean
/// parse cannot hold a key nothing can interpret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootKey {
    algorithm: Box<str>,
    material: Box<str>,
}

impl RootKey {
    #[must_use]
    pub fn algorithm(&self) -> &str {
        &self.algorithm
    }

    #[must_use]
    pub fn material(&self) -> &str {
        &self.material
    }

    /// # Errors
    /// Returns why `spelling` is not a root key: a missing separator, or an
    /// empty half.
    pub fn parse(spelling: &str) -> Result<Self, RootKeyError> {
        let (algorithm, material) = spelling.split_once(':').ok_or(RootKeyError::Separator)?;
        if algorithm.is_empty() {
            return Err(RootKeyError::EmptyAlgorithm);
        }
        if material.is_empty() {
            return Err(RootKeyError::EmptyMaterial);
        }
        Ok(Self {
            algorithm: algorithm.into(),
            material: material.into(),
        })
    }
}

impl fmt::Display for RootKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.algorithm, self.material)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RootKeyError {
    Separator,
    EmptyAlgorithm,
    EmptyMaterial,
}

impl fmt::Display for RootKeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Separator => formatter.write_str("a root key is spelled `algorithm:material`"),
            Self::EmptyAlgorithm => formatter.write_str("a root key names a signature algorithm"),
            Self::EmptyMaterial => formatter.write_str("a root key carries key material"),
        }
    }
}

/// One edge of a version range: a version and whether the edge is inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionBound {
    pub version: ManifestVersion,
    pub inclusive: bool,
}

/// Which versions of a subject a `use` clause admits.
///
/// These are 0040's range constructors, so the manifest grammar and the
/// resolver protocol share one model rather than translating between two.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VersionRange {
    Any,
    Exactly(ManifestVersion),
    AtLeast(VersionBound),
    AtMost(VersionBound),
    Between {
        lower: VersionBound,
        upper: VersionBound,
    },
}

impl VersionRange {
    /// Whether the range admits every version, which is what a clause with
    /// no written range means.
    #[must_use]
    pub const fn is_any(&self) -> bool {
        matches!(self, Self::Any)
    }

    /// Whether `version` is inside the range, under the ordering a
    /// manifest version carries with it.
    ///
    /// Two spellings that compare equal are one version under 0070, so a
    /// range holding `1.2` admits `1.2.0`.
    #[must_use]
    pub fn satisfies(&self, version: &ManifestVersion) -> bool {
        self.shared()
            .contains(&pith_constraint::ByOrdering, &version)
    }

    /// The nonempty intersection of two ranges, in the same constructors a
    /// manifest writes: `None` when they share no version.
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Option<Self> {
        self.shared()
            .intersect(&pith_constraint::ByOrdering, &other.shared())
            .as_ref()
            .map(owned_range)
    }

    fn shared(&self) -> pith_constraint::Range<&ManifestVersion> {
        match self {
            Self::Any => pith_constraint::Range::Any,
            Self::Exactly(version) => pith_constraint::Range::Exactly(version),
            Self::AtLeast(bound) => pith_constraint::Range::AtLeast(shared_edge(bound)),
            Self::AtMost(bound) => pith_constraint::Range::AtMost(shared_edge(bound)),
            Self::Between { lower, upper } => pith_constraint::Range::Between {
                lower: shared_edge(lower),
                upper: shared_edge(upper),
            },
        }
    }
}

/// A manifest bound as a shared edge over its borrowed version.
fn shared_edge(bound: &VersionBound) -> pith_constraint::Edge<&ManifestVersion> {
    pith_constraint::Edge {
        version: &bound.version,
        inclusive: bound.inclusive,
    }
}

/// The owning form of a shared range, in the manifest's own constructors.
fn owned_range(shared: &pith_constraint::Range<&ManifestVersion>) -> VersionRange {
    let edge = |edge: &pith_constraint::Edge<&ManifestVersion>| VersionBound {
        version: edge.version.clone(),
        inclusive: edge.inclusive,
    };
    match shared {
        pith_constraint::Range::Any => VersionRange::Any,
        pith_constraint::Range::Exactly(version) => VersionRange::Exactly((*version).clone()),
        pith_constraint::Range::AtLeast(bound) => VersionRange::AtLeast(edge(bound)),
        pith_constraint::Range::AtMost(bound) => VersionRange::AtMost(edge(bound)),
        pith_constraint::Range::Between { lower, upper } => VersionRange::Between {
            lower: edge(lower),
            upper: edge(upper),
        },
    }
}

impl fmt::Display for VersionRange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bound = |bound: &VersionBound, inclusive: &str, exclusive: &str| {
            format!(
                "{} {}",
                if bound.inclusive {
                    inclusive
                } else {
                    exclusive
                },
                bound.version
            )
        };
        match self {
            Self::Any => formatter.write_str("any"),
            Self::Exactly(version) => write!(formatter, "{version}"),
            Self::AtLeast(lower) => formatter.write_str(&bound(lower, ">=", ">")),
            Self::AtMost(upper) => formatter.write_str(&bound(upper, "<=", "<")),
            Self::Between { lower, upper } => write!(
                formatter,
                "{}, {}",
                bound(lower, ">=", ">"),
                bound(upper, "<=", "<")
            ),
        }
    }
}

/// A manifest whose singleton clauses are settled: the subject, version,
/// workspace membership, and bindings a loader resolves against.
pub struct Manifest {
    module: ManifestModule,
    workspace: Option<ManifestWorkspace>,
    uses: Box<[ManifestUse]>,
    registries: Box<[ManifestRegistry]>,
    domains: Box<[ManifestDomain]>,
}

impl Manifest {
    #[must_use]
    pub fn subject(&self) -> &ModuleSubject {
        &self.module.subject
    }

    #[must_use]
    pub fn version(&self) -> &ManifestVersion {
        &self.module.version
    }

    #[must_use]
    pub fn workspace(&self) -> Option<&ManifestWorkspace> {
        self.workspace.as_ref()
    }

    #[must_use]
    pub fn uses(&self) -> &[ManifestUse] {
        &self.uses
    }

    /// The registry bindings this manifest declares. They are authority only
    /// in a root manifest; a dependency's are diagnosed and ignored.
    #[must_use]
    pub fn registries(&self) -> &[ManifestRegistry] {
        &self.registries
    }

    /// The domain routes this manifest declares, under the same rule as
    /// [`Self::registries`].
    #[must_use]
    pub fn domains(&self) -> &[ManifestDomain] {
        &self.domains
    }

    /// The span of the module clause, where diagnostics about ownership of
    /// the module's source tree land.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.module.span
    }
}

/// A declared `(domain, name)` subject. Paths, aliases, and versions do not
/// enter it; two locations agreeing on it is a refusal, not a merge.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleSubject {
    domain: SubjectSegment,
    name: SubjectSegment,
}

impl ModuleSubject {
    #[must_use]
    pub const fn new(domain: SubjectSegment, name: SubjectSegment) -> Self {
        Self { domain, name }
    }

    #[must_use]
    pub fn domain(&self) -> &SubjectSegment {
        &self.domain
    }

    #[must_use]
    pub fn name(&self) -> &SubjectSegment {
        &self.name
    }

    /// The coordinate spelling: `domain/name`.
    #[must_use]
    pub fn spelling(&self) -> Box<str> {
        self.to_string().into()
    }

    /// # Errors
    /// Returns why `spelling` is not a subject: the segment count, or the
    /// first segment outside the grammar.
    pub fn parse(spelling: &str) -> Result<Self, SubjectError> {
        let (domain, name) = spelling.split_once('/').ok_or(SubjectError::Segments {
            spelling: spelling.into(),
        })?;
        let segment = |text: &str| {
            SubjectSegment::parse(text).map_err(|error| SubjectError::Segment {
                segment: text.into(),
                error,
            })
        };
        Ok(Self::new(segment(domain)?, segment(name)?))
    }
}

impl fmt::Display for ModuleSubject {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.domain.as_str(), self.name.as_str())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SubjectError {
    /// The spelling is not two segments joined by one slash.
    Segments { spelling: Box<str> },
    /// One segment falls outside the segment grammar.
    Segment {
        segment: Box<str>,
        error: SegmentError,
    },
}

impl fmt::Display for SubjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Segments { spelling } => {
                write!(formatter, "`{spelling}` is not a `domain/name` subject")
            }
            Self::Segment { segment, error } => {
                write!(formatter, "`{segment}` is not a subject segment: {error}")
            }
        }
    }
}

/// One lowercase subject segment: an ASCII letter, then letters, digits, or
/// hyphens. Constructed only through validation, so a segment outside the
/// grammar has no value.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubjectSegment(Box<str>);

impl SubjectSegment {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// # Errors
    /// Returns the first way `text` leaves the segment grammar.
    pub fn parse(text: &str) -> Result<Self, SegmentError> {
        let mut characters = text.chars();
        let first = characters.next().ok_or(SegmentError::Empty)?;
        if !first.is_ascii_lowercase() {
            return Err(SegmentError::Start(first));
        }
        if let Some(character) = characters.find(|candidate| !is_segment_byte(*candidate)) {
            return Err(SegmentError::Character(character));
        }
        Ok(Self(text.into()))
    }
}

fn is_segment_byte(character: char) -> bool {
    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
}

#[derive(Debug, PartialEq, Eq)]
pub enum SegmentError {
    Empty,
    Start(char),
    Character(char),
}

impl fmt::Display for SegmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("a segment is nonempty"),
            Self::Start(character) => {
                write!(
                    formatter,
                    "a segment starts with a lowercase letter, not `{character}`"
                )
            }
            Self::Character(character) => {
                write!(
                    formatter,
                    "a segment continues with letters, digits, or hyphens, not `{character}`"
                )
            }
        }
    }
}

/// A manifest version: dotted integer segments under the `numeric-segments`
/// scheme, where a missing trailing segment compares as zero.
///
/// That comparison decides the type's identity. `1.2` and `1.2.0` order
/// equal, so under 0070 they are one version, and equality here is over the
/// canonical form — the segments with trailing zeros removed — rather than
/// over the spelling. The spelling is retained for printing, so formatting a
/// manifest never silently rewrites a version a person wrote.
#[derive(Clone, Debug)]
pub struct ManifestVersion {
    segments: Box<[u64]>,
}

impl ManifestVersion {
    /// # Errors
    /// Returns [`EmptyVersion`] for an empty segment list: a version has at
    /// least one segment.
    pub fn from_segments(segments: impl Into<Box<[u64]>>) -> Result<Self, EmptyVersion> {
        let segments = segments.into();
        if segments.is_empty() {
            return Err(EmptyVersion);
        }
        Ok(Self { segments })
    }

    /// A version from its dotted numeric spelling.
    ///
    /// # Errors
    /// Returns [`InvalidVersionSpelling`] when the spelling is empty, a
    /// segment is empty, or a segment is not a `u64`.
    pub fn parse(spelling: &str) -> Result<Self, InvalidVersionSpelling> {
        let invalid = || InvalidVersionSpelling {
            spelling: spelling.into(),
        };
        let mut segments = Vec::new();
        for segment in spelling.split('.') {
            segments.push(segment.parse::<u64>().map_err(|_| invalid())?);
        }
        Self::from_segments(segments).map_err(|_| invalid())
    }

    /// The segments as written.
    #[must_use]
    pub fn segments(&self) -> &[u64] {
        &self.segments
    }

    /// The identity: the written segments with trailing zeros removed, never
    /// shorter than one segment. Two spellings sharing this are one version.
    #[must_use]
    pub fn canonical(&self) -> &[u64] {
        let mut end = self.segments.len();
        while end > 1 && self.segments.get(end.saturating_sub(1)) == Some(&0) {
            end = end.saturating_sub(1);
        }
        self.segments.get(..end).unwrap_or(&self.segments)
    }

    /// The canonical spelling, which is what a lock and an index entry
    /// record.
    #[must_use]
    pub fn canonical_spelling(&self) -> Box<str> {
        let mut spelling = String::new();
        for (position, segment) in self.canonical().iter().enumerate() {
            if position > 0 {
                spelling.push('.');
            }
            spelling.push_str(&segment.to_string());
        }
        spelling.into()
    }
}

impl PartialEq for ManifestVersion {
    fn eq(&self, other: &Self) -> bool {
        self.canonical() == other.canonical()
    }
}

impl Eq for ManifestVersion {}

impl PartialOrd for ManifestVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ManifestVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.canonical().cmp(other.canonical())
    }
}

impl std::hash::Hash for ManifestVersion {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        self.canonical().hash(hasher);
    }
}

/// A version with no segments, which no spelling produces.
#[derive(Debug, PartialEq, Eq)]
pub struct EmptyVersion;

impl fmt::Display for EmptyVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a version holds at least one segment")
    }
}

impl std::error::Error for EmptyVersion {}

/// A spelling that is not a dotted numeric-segments version.
#[derive(Debug, PartialEq, Eq)]
pub struct InvalidVersionSpelling {
    pub spelling: Box<str>,
}

impl fmt::Display for InvalidVersionSpelling {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "`{}` is not a dotted numeric version: dot-separated `u64` segments",
            self.spelling
        )
    }
}

impl std::error::Error for InvalidVersionSpelling {}

impl fmt::Display for ManifestVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut spelling = String::new();
        for (position, segment) in self.segments.iter().enumerate() {
            if position > 0 {
                spelling.push('.');
            }
            spelling.push_str(&segment.to_string());
        }
        formatter.write_str(&spelling)
    }
}

/// The parse held no module clause, so there is no manifest to load.
#[derive(Debug, PartialEq, Eq)]
pub struct MissingModule;

impl fmt::Display for MissingModule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the manifest declares no `module` clause")
    }
}

impl std::error::Error for MissingModule {}
