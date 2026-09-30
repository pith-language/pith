//! Versions and the ranges an input admits over them. A version is a
//! canonical segment sequence under the `numeric-segments` scheme; a range
//! uses the resolver protocol's constructors, so the project grammar and
//! the resolver share one model.

use std::fmt;

/// One edge of a version range: a version and whether the edge is inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionBound {
    pub version: ModuleVersion,
    pub inclusive: bool,
}

/// Which versions of a subject an input admits.
///
/// These are the resolver's range constructors, so the project grammar and
/// the resolver protocol share one model rather than translating between two.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VersionRange {
    Any,
    Exactly(ModuleVersion),
    AtLeast(VersionBound),
    AtMost(VersionBound),
    Between {
        lower: VersionBound,
        upper: VersionBound,
    },
}

impl VersionRange {
    /// Whether the range admits every version, which is what an entry with
    /// no written range means.
    #[must_use]
    pub const fn is_any(&self) -> bool {
        matches!(self, Self::Any)
    }

    /// Whether `version` is inside the range, under the ordering a module
    /// version carries with it.
    ///
    /// Two spellings that compare equal are one version, so a range holding
    /// `1.2` admits `1.2.0`.
    #[must_use]
    pub fn satisfies(&self, version: &ModuleVersion) -> bool {
        self.shared()
            .contains(&pith_constraint::ByOrdering, &version)
    }

    /// The nonempty intersection of two ranges, in the same constructors an
    /// input writes: `None` when they share no version.
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Option<Self> {
        self.shared()
            .intersect(&pith_constraint::ByOrdering, &other.shared())
            .as_ref()
            .map(owned_range)
    }

    fn shared(&self) -> pith_constraint::Range<&ModuleVersion> {
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

/// A range bound as a shared edge over its borrowed version.
fn shared_edge(bound: &VersionBound) -> pith_constraint::Edge<&ModuleVersion> {
    pith_constraint::Edge {
        version: &bound.version,
        inclusive: bound.inclusive,
    }
}

/// The owning form of a shared range, in the project grammar's own
/// constructors.
fn owned_range(shared: &pith_constraint::Range<&ModuleVersion>) -> VersionRange {
    let edge = |edge: &pith_constraint::Edge<&ModuleVersion>| VersionBound {
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

/// A module version: dotted integer segments under the `numeric-segments`
/// scheme, where a missing trailing segment compares as zero. `1.2` and
/// `1.2.0` order equal, so they are one version: identity is over the
/// canonical form (the segments with trailing zeros removed), not the
/// spelling. The spelling is retained for printing, so formatting a project
/// never silently rewrites a version a person wrote.
#[derive(Clone, Debug)]
pub struct ModuleVersion {
    segments: Box<[u64]>,
}

impl ModuleVersion {
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

impl PartialEq for ModuleVersion {
    fn eq(&self, other: &Self) -> bool {
        self.canonical() == other.canonical()
    }
}

impl Eq for ModuleVersion {}

impl PartialOrd for ModuleVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ModuleVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.canonical().cmp(other.canonical())
    }
}

impl std::hash::Hash for ModuleVersion {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        self.canonical().hash(hasher);
    }
}

impl fmt::Display for ModuleVersion {
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
