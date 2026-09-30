//! Module identity: the `(domain, name)` subject a project declares, and
//! the grammar its segments carry. Constructed only through validation, so
//! an invalid subject has no value.

use std::fmt;

/// A declared `(domain, name)` subject. Paths, names, and versions do not
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
