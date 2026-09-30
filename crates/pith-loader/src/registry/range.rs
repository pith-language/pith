//! A version range in its written spelling: `*`, `=1.2`, `>=1.2`, `>1.2`,
//! `<=2.0`, `<2.0`, or a comma-joined lower and upper edge such as
//! `>=1.2,<2.0`. Each edge carries its own inclusivity, so the manifest's
//! closed constructor set reads and writes without loss, and a range is
//! always one token wide.

use pith_hir::{ModuleVersion, VersionBound, VersionRange};

use super::token::{Malformed, Token};

/// The written spelling of `range`.
#[must_use]
pub(crate) fn render(range: &VersionRange) -> String {
    let edge = |inclusive: &str, exclusive: &str, bound: &VersionBound| {
        format!(
            "{}{}",
            if bound.inclusive {
                inclusive
            } else {
                exclusive
            },
            bound.version.canonical_spelling()
        )
    };
    match range {
        VersionRange::Any => "*".into(),
        VersionRange::Exactly(version) => format!("={}", version.canonical_spelling()),
        VersionRange::AtLeast(bound) => edge(">=", ">", bound),
        VersionRange::AtMost(bound) => edge("<=", "<", bound),
        VersionRange::Between { lower, upper } => {
            format!("{},{}", edge(">=", ">", lower), edge("<=", "<", upper))
        }
    }
}

/// # Errors
/// Returns [`Malformed`] spanning the token when it is not a range's
/// spelling.
pub(crate) fn parse(token: &Token) -> Result<VersionRange, Malformed> {
    let refuse = |message: String| Malformed {
        message,
        span: token.span,
    };
    if token.is("*") {
        return Ok(VersionRange::Any);
    }
    let mut edges = Vec::new();
    for edge in token.spelling().split(',') {
        let bound = |spelling: &str| {
            ModuleVersion::parse(spelling).map_err(|_| {
                refuse(format!(
                    "the range edge `{edge}` bounds with a dotted numeric version"
                ))
            })
        };
        let edge = if let Some(rest) = edge.strip_prefix(">=") {
            Edge::AtLeast {
                inclusive: true,
                version: bound(rest)?,
            }
        } else if let Some(rest) = edge.strip_prefix('>') {
            Edge::AtLeast {
                inclusive: false,
                version: bound(rest)?,
            }
        } else if let Some(rest) = edge.strip_prefix("<=") {
            Edge::AtMost {
                inclusive: true,
                version: bound(rest)?,
            }
        } else if let Some(rest) = edge.strip_prefix('<') {
            Edge::AtMost {
                inclusive: false,
                version: bound(rest)?,
            }
        } else if let Some(rest) = edge.strip_prefix('=') {
            Edge::Exactly(bound(rest)?)
        } else {
            return Err(refuse(format!(
                "the range edge `{edge}` carries no comparison"
            )));
        };
        edges.push(edge);
    }
    match edges.as_slice() {
        [Edge::Exactly(version)] => Ok(VersionRange::Exactly(version.clone())),
        [Edge::AtLeast { inclusive, version }] => {
            Ok(VersionRange::AtLeast(bound_of(inclusive, version)))
        }
        [Edge::AtMost { inclusive, version }] => {
            Ok(VersionRange::AtMost(bound_of(inclusive, version)))
        }
        [
            Edge::AtLeast {
                inclusive: lower_inclusive,
                version: lower,
            },
            Edge::AtMost {
                inclusive: upper_inclusive,
                version: upper,
            },
        ] => Ok(VersionRange::Between {
            lower: bound_of(lower_inclusive, lower),
            upper: bound_of(upper_inclusive, upper),
        }),
        [
            _lower @ Edge::AtLeast { .. },
            _upper @ Edge::AtMost { .. },
            ..,
        ] => Err(refuse("a range is one or two comma-joined edges".into())),
        _ => Err(refuse(
            "a two-edge range runs a `>` lower edge into a `<` upper edge".into(),
        )),
    }
}

enum Edge {
    Exactly(ModuleVersion),
    AtLeast {
        inclusive: bool,
        version: ModuleVersion,
    },
    AtMost {
        inclusive: bool,
        version: ModuleVersion,
    },
}

fn bound_of(inclusive: &bool, version: &ModuleVersion) -> VersionBound {
    VersionBound {
        version: version.clone(),
        inclusive: *inclusive,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pith_diag::Span;

    #[test]
    fn range_tokens_round_trip_over_the_constructor_set() {
        let version = |spelling: &str| ModuleVersion::parse(spelling).unwrap();
        let bound = |spelling: &str, inclusive: bool| VersionBound {
            version: version(spelling),
            inclusive,
        };
        let ranges = [
            VersionRange::Any,
            VersionRange::Exactly(version("1.2")),
            VersionRange::AtLeast(bound("1.2", true)),
            VersionRange::AtLeast(bound("1.2", false)),
            VersionRange::AtMost(bound("2.0", true)),
            VersionRange::AtMost(bound("2.0", false)),
            VersionRange::Between {
                lower: bound("1.2", true),
                upper: bound("2.0", false),
            },
        ];
        for range in ranges {
            let token = Token {
                text: render(&range),
                span: Span::none(),
            };
            assert_eq!(parse(&token).as_ref(), Ok(&range));
        }
    }
}
