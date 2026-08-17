//! Tokens, quoting, feature lists, and digest spelling for lock text.
//!
//! Fields use bare tokens when possible and quoted tokens with backslash
//! escapes otherwise. Digests render as `blake3:` followed by lowercase
//! hexadecimal digits; parsing also accepts uppercase digits.
//!
//! Every parse here refuses with a span selecting the offending field in the
//! source, and the message naming what is wrong with it; the caller holds
//! the source and attaches it.

use std::fmt::Write as _;

use pith_diag::{ByteOffset, Span};
use pith_ids::{ContentId, DIGEST_LEN};

use crate::codec::digest_from_hex;

const HEX_LEN: usize = DIGEST_LEN * 2;

/// One written field: the text after quoting is resolved, and the span of
/// its written spelling in the source.
#[derive(Debug)]
pub(crate) struct Token {
    pub text: String,
    pub span: Span,
}

/// A refusal from token or field parsing: what is wrong, and the span it
/// happened in.
#[derive(Debug)]
pub(crate) struct Refusal {
    pub message: String,
    pub span: Span,
}

/// The byte offset of `rest`, a tail of `line`, when `line` begins at
/// `base` in the source.
fn at(base: ByteOffset, line: &str, rest: &str) -> ByteOffset {
    let consumed = line.len().saturating_sub(rest.len());
    ByteOffset(
        base.0
            .saturating_add(u32::try_from(consumed).unwrap_or(u32::MAX)),
    )
}

fn end_of(base: ByteOffset, line: &str) -> ByteOffset {
    ByteOffset(
        base.0
            .saturating_add(u32::try_from(line.len()).unwrap_or(u32::MAX)),
    )
}

/// One text field in its written spelling: bare when it contains none of
/// the reserved characters, quoted with backslash escapes otherwise.
pub(crate) fn token(text: &str) -> String {
    if is_bare(text) {
        return text.into();
    }
    let mut out = String::with_capacity(text.len().saturating_add(2));
    out.push('"');
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if control.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", control as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

pub(crate) fn is_bare(text: &str) -> bool {
    !text.is_empty()
        && text.chars().all(|character| {
            !character.is_whitespace()
                && !character.is_control()
                && !matches!(character, '"' | '#' | '\\' | '[' | ']')
        })
}

/// Split a line into tokens on spaces, keeping each token's span. A span
/// runs from the token's first character to the character after its last,
/// quoted material included; a `[`-bracketed group stays one token with
/// quoting honored inside it; an unquoted `#` ends the line as a comment.
/// A refusal spans the offending token's start through the end of the line,
/// because every way a token fails leaves the rest of the line suspect.
pub(crate) fn tokenize(line: &str, base: ByteOffset) -> Result<Vec<Token>, Refusal> {
    let mut tokens = Vec::new();
    let mut rest = line;
    loop {
        rest = rest.trim_start_matches(' ');
        if rest.is_empty() || rest.starts_with('#') {
            return Ok(tokens);
        }
        let start = at(base, line, rest);
        let (text, remaining) = if let Some(inner) = rest.strip_prefix('[') {
            match bracket_group(inner) {
                Ok((group, remaining)) => (format!("[{group}]"), remaining),
                Err(message) => {
                    return Err(Refusal {
                        message,
                        span: Span::new(start, end_of(base, line)),
                    });
                }
            }
        } else if rest.starts_with('"') {
            match quoted_token(rest) {
                Ok((token, remaining)) => (token, remaining),
                Err(message) => {
                    return Err(Refusal {
                        message,
                        span: Span::new(start, end_of(base, line)),
                    });
                }
            }
        } else {
            bare_token(rest).map_err(|message| Refusal {
                message,
                span: Span::new(start, end_of(base, line)),
            })?
        };
        tokens.push(Token {
            text,
            span: Span::new(start, at(base, line, remaining)),
        });
        rest = remaining;
    }
}

fn bare_token(rest: &str) -> Result<(String, &str), String> {
    let end = rest.find([' ', '"', '[', ']', '#']).unwrap_or(rest.len());
    let (token, remaining) = rest.split_at(end);
    if remaining.starts_with(['"', '[', ']']) {
        return Err(format!(
            "the bare token `{token}` runs into a reserved character; quote the whole token"
        ));
    }
    Ok((token.into(), remaining))
}

fn quoted_token(rest: &str) -> Result<(String, &str), String> {
    let mut token = String::new();
    let mut chars = rest.chars();
    chars.next();
    loop {
        let Some(character) = chars.next() else {
            return Err(format!("the quoted token `{rest}` is never closed"));
        };
        match character {
            '"' => return Ok((token, chars.as_str())),
            '\\' => token.push(escape(&mut chars)?),
            other => token.push(other),
        }
    }
}

fn bracket_group(rest: &str) -> Result<(String, &str), String> {
    let mut group = String::new();
    let mut chars = rest.chars();
    loop {
        let Some(character) = chars.next() else {
            return Err("the bracketed feature list is never closed".into());
        };
        match character {
            ']' => return Ok((group, chars.as_str())),
            '"' => {
                group.push('"');
                loop {
                    let Some(inner) = chars.next() else {
                        return Err("a quoted feature name is never closed".into());
                    };
                    group.push(inner);
                    match inner {
                        '\\' => {
                            chars
                                .next()
                                .map(|escaped| group.push(escaped))
                                .ok_or_else(|| {
                                    "a quoted feature name ends on an escape".to_string()
                                })?;
                        }
                        '"' => break,
                        _ => {}
                    }
                }
            }
            other => group.push(other),
        }
    }
}

fn escape(chars: &mut std::str::Chars<'_>) -> Result<char, String> {
    let Some(escaped) = chars.next() else {
        return Err("a quoted token ends on an escape".into());
    };
    match escaped {
        '\\' => Ok('\\'),
        '"' => Ok('"'),
        'n' => Ok('\n'),
        'r' => Ok('\r'),
        't' => Ok('\t'),
        'u' => {
            let mut hex = String::new();
            if chars.next() != Some('{') {
                return Err("a unicode escape opens with `\\u{`".into());
            }
            let mut closed = false;
            for character in chars.by_ref() {
                match character {
                    '}' => {
                        closed = true;
                        break;
                    }
                    digit if digit.is_ascii_hexdigit() => hex.push(digit),
                    other => return Err(format!("`{other}` is not a hexadecimal digit")),
                }
            }
            if !closed {
                return Err("a unicode escape is never closed with `}`".into());
            }
            u32::from_str_radix(&hex, 16)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| format!("`\\u{{{hex}}}` is not a unicode scalar value"))
        }
        other => Err(format!("`\\{other}` is not an escape this format defines")),
    }
}

/// A feature set in its written spelling: `[`-bracketed, comma-separated,
/// each feature in its own token spelling.
pub(crate) fn features_token(features: &[Box<str>]) -> String {
    let mut out = String::from("[");
    for (index, feature) in features.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&token(feature));
    }
    out.push(']');
    out
}

/// Parses the bracketed, comma-separated feature list a token carries.
///
/// # Errors
/// A [`Refusal`] whose span selects the field when the list or one of its
/// tokens is invalid.
pub(crate) fn parse_features(field: &Token) -> Result<Box<[Box<str>]>, Refusal> {
    let Some(inner) = field
        .text
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return Err(Refusal {
            message: format!("`{}` is not a bracketed feature list", field.text),
            span: field.span,
        });
    };
    if inner.is_empty() {
        return Ok(Box::new([]));
    }
    // The bracket token's text is its written spelling verbatim, so offsets
    // inside it are offsets in the source.
    let inner_base = ByteOffset(field.span.start.0.saturating_add(1));
    let mut features = Vec::new();
    for piece in split_commas(inner, inner_base) {
        features.push(
            unquote(&piece)
                .map_err(|message| Refusal {
                    message,
                    span: piece.span,
                })?
                .into(),
        );
    }
    Ok(features.into())
}

/// Split bracket-group content on commas, keeping quoted pieces whole and
/// each piece's span.
fn split_commas(inner: &str, base: ByteOffset) -> Vec<Token> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut chars = inner.chars();
    let mut start = base;
    while let Some(character) = chars.next() {
        let consumed = inner.len().saturating_sub(chars.as_str().len());
        match character {
            '"' => {
                current.push('"');
                while let Some(inner_character) = chars.next() {
                    current.push(inner_character);
                    match inner_character {
                        '\\' => {
                            if let Some(escaped) = chars.next() {
                                current.push(escaped);
                            }
                        }
                        '"' => break,
                        _ => {}
                    }
                }
            }
            ',' => {
                // The comma is one byte, so the piece ends one byte before
                // the consumed prefix and the next begins at it.
                let end =
                    ByteOffset(base.0.saturating_add(
                        u32::try_from(consumed.saturating_sub(1)).unwrap_or(u32::MAX),
                    ));
                pieces.push(Token {
                    text: current.clone(),
                    span: Span::new(start, end),
                });
                current.clear();
                start = ByteOffset(
                    base.0
                        .saturating_add(u32::try_from(consumed).unwrap_or(u32::MAX)),
                );
            }
            other => current.push(other),
        }
    }
    pieces.push(Token {
        text: current,
        span: Span::new(start, end_of(base, inner)),
    });
    pieces
}

fn unquote(field: &Token) -> Result<String, String> {
    let piece = field.text.trim();
    if piece.starts_with('"') {
        let (token, remaining) = quoted_token(piece)?;
        if !remaining.trim().is_empty() {
            return Err(format!("`{piece}` carries text after its closing quote"));
        }
        return Ok(token);
    }
    if !is_bare(piece) {
        return Err(format!(
            "`{piece}` is not a bare token; quote it to carry the characters it has"
        ));
    }
    Ok(piece.into())
}

/// Parses `blake3:` followed by 64 hexadecimal digits.
///
/// # Errors
/// A [`Refusal`] whose span selects the field when the text is not a digest
/// of this shape.
pub(crate) fn parse_digest(field: &Token, name: &str) -> Result<ContentId, Refusal> {
    let Some(hex) = field.text.strip_prefix(BLAKE3) else {
        return Err(Refusal {
            message: format!(
                "the {name} carried `{}`, rather than a `blake3:`-prefixed digest",
                field.text
            ),
            span: field.span,
        });
    };
    if !hex.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(Refusal {
            message: format!("the {name} carried `{hex}`, which is not hexadecimal"),
            span: field.span,
        });
    }
    let digest = digest_from_hex(hex).ok_or_else(|| Refusal {
        message: format!(
            "the {name} carried {} hexadecimal digits rather than {HEX_LEN}",
            hex.len()
        ),
        span: field.span,
    })?;
    Ok(ContentId::from_digest(digest))
}

/// The digest prefix the written form spells, shared by every directive and
/// binding line that names content.
pub(crate) const BLAKE3: &str = "blake3:";

/// A version range in its written spelling: `*`, `=1.0`, `>=1.0`, `>1.0`,
/// `<=2.0`, `<2.0`, or a comma-joined pair of a lower and an upper edge such
/// as `>=1.0,<2.0`. Each edge carries its own inclusivity, so the closed
/// constructor set of ranges reads and writes without loss.
pub(crate) fn range_token(range: &crate::constraint::Range) -> String {
    use crate::constraint::{Bound, Range};
    fn edge(inclusive: &str, exclusive: &str, bound: &Bound) -> String {
        format!(
            "{}{}",
            if bound.inclusive {
                inclusive
            } else {
                exclusive
            },
            bound.version.as_ref()
        )
    }
    match range {
        Range::Any => "*".into(),
        Range::Exactly(version) => format!("={version}"),
        Range::AtLeast(bound) => edge(">=", ">", bound),
        Range::AtMost(bound) => edge("<=", "<", bound),
        Range::Between { lower, upper } => {
            format!("{},{}", edge(">=", ">", lower), edge("<=", "<", upper))
        }
    }
}

/// Parses the range spelling [`range_token`] writes.
///
/// # Errors
/// A [`Refusal`] whose span selects the field when the text is not a range
/// of this shape. A malformed two-edge range names which edge failed; a
/// malformed single token names the grammar rather than guessing which side
/// it was meant to be.
pub(crate) fn parse_range(field: &Token) -> Result<crate::constraint::Range, Refusal> {
    use crate::constraint::{Bound, Range};
    let text = field.text.as_str();
    let bad = |complaint: &str| Refusal {
        message: format!("`{text}` is not {complaint}"),
        span: field.span,
    };
    // The version spelling belongs to the domain's scheme, but the operator
    // characters belong to this grammar: a version carrying one would re-split
    // the token, so the parse refuses it rather than guessing a cut.
    fn version(spelled: &str) -> Option<&str> {
        (!spelled.is_empty() && !spelled.contains(['=', '<', '>', '*', ','])).then_some(spelled)
    }
    let bound = |edge: &str, operator: &str| -> Option<Bound> {
        edge.strip_prefix(operator)
            .and_then(version)
            .map(|spelled| Bound::new(spelled, true))
    };
    let exclusive = |edge: &str, operator: &str| -> Option<Bound> {
        edge.strip_prefix(operator)
            .and_then(version)
            .map(|spelled| Bound::new(spelled, false))
    };
    let lower = |edge: &str| {
        bound(edge, ">=")
            .or_else(|| exclusive(edge, ">"))
            .ok_or_else(|| bad("a lower edge (`>=1.0` or `>1.0`) of a two-edge range"))
    };
    let upper = |edge: &str| {
        bound(edge, "<=")
            .or_else(|| exclusive(edge, "<"))
            .ok_or_else(|| bad("an upper edge (`<=2.0` or `<2.0`) of a two-edge range"))
    };
    if text == "*" {
        return Ok(Range::Any);
    }
    match text.split(',').collect::<Vec<_>>().as_slice() {
        [single] => {
            if let Some(spelled) = single.strip_prefix('=').and_then(version) {
                return Ok(Range::Exactly(spelled.into()));
            }
            if let Some(at_least) = bound(single, ">=").or_else(|| exclusive(single, ">")) {
                return Ok(Range::AtLeast(at_least));
            }
            if let Some(at_most) = bound(single, "<=").or_else(|| exclusive(single, "<")) {
                return Ok(Range::AtMost(at_most));
            }
            Err(bad(
                "a range this format spells: `*`, `=1.0`, `>=1.0`, `>1.0`, `<=2.0`, `<2.0`, or \
                 a comma-joined pair such as `>=1.0,<2.0`",
            ))
        }
        [low, high] => Ok(Range::Between {
            lower: lower(low)?,
            upper: upper(high)?,
        }),
        _ => Err(bad(
            "a range of at most two comma-joined edges, lower then upper",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint::{Bound, Range};

    fn field(text: &str, base: u32) -> Token {
        Token {
            text: text.into(),
            span: Span::new(
                ByteOffset(base),
                ByteOffset(base.saturating_add(text.len() as u32)),
            ),
        }
    }

    fn select(source: &str, span: Span) -> &str {
        source
            .get(span.start.0 as usize..span.end.0 as usize)
            .unwrap()
    }

    #[test]
    fn every_range_constructor_round_trips_through_its_token() {
        let ranges = [
            Range::Any,
            Range::Exactly("1.3".into()),
            Range::AtLeast(Bound::new("1.0", true)),
            Range::AtLeast(Bound::new("2.0", false)),
            Range::AtMost(Bound::new("1.9", true)),
            Range::AtMost(Bound::new("1.9", false)),
            Range::Between {
                lower: Bound::new("1.0", true),
                upper: Bound::new("2.0", false),
            },
        ];
        for range in &ranges {
            let token = range_token(range);
            assert!(
                is_bare(&token),
                "`{token}` stays one bare token, so a requirement clause cannot re-split it"
            );
            assert_eq!(&parse_range(&field(&token, 0)).unwrap(), range);
        }
        assert_eq!(
            range_token(&Range::AtLeast(Bound::new("1.0", true))),
            ">=1.0"
        );
        assert_eq!(
            range_token(&Range::Between {
                lower: Bound::new("1.0", true),
                upper: Bound::new("2.0", false),
            }),
            ">=1.0,<2.0"
        );
    }

    #[test]
    fn a_malformed_range_is_refused_with_the_field_and_its_span() {
        for text in ["1.0", ">=1.0,>=2.0", ">=", ">=1.0,<2.0,>=3.0", ""] {
            let field = field(text, 12);
            let refusal = parse_range(&field).unwrap_err();
            assert_eq!(refusal.span, field.span, "the span selects the field");
            assert!(
                refusal.message.contains(text),
                "the message names the offending text `{text}`: {}",
                refusal.message
            );
        }
    }

    #[test]
    fn tokens_carry_spans_of_their_written_spelling_at_their_base() {
        let line = "bind zlib 1.3 [shared,zlib]";
        let base = ByteOffset(31);
        let source = format!("{}{line}", " ".repeat(base.0 as usize));
        let tokens = tokenize(line, base).unwrap();
        let spelled: Vec<&str> = tokens
            .iter()
            .map(|token| select(&source, token.span))
            .collect();
        assert_eq!(spelled, ["bind", "zlib", "1.3", "[shared,zlib]"]);
        let bracket = tokens.last().unwrap();
        assert_eq!(bracket.text, "[shared,zlib]");
    }

    #[test]
    fn a_quoted_token_spans_its_escapes_at_written_length() {
        let line = r#""a b" plain"#;
        let tokens = tokenize(line, ByteOffset(0)).unwrap();
        let quoted = tokens.first().unwrap();
        assert_eq!(quoted.text, "a b");
        assert_eq!(select(line, quoted.span), r#""a b""#);
        let line = r#"  "tab\tthere""#;
        let tokens = tokenize(line, ByteOffset(0)).unwrap();
        let quoted = tokens.first().unwrap();
        assert_eq!(quoted.text, "tab\tthere");
        assert_eq!(select(line, quoted.span), r#""tab\tthere""#);
    }

    #[test]
    fn a_refusal_spans_the_rest_of_the_line_from_the_offending_token() {
        let line = "bind \"unclosed zlib";
        let refusal = tokenize(line, ByteOffset(0)).unwrap_err();
        assert_eq!(select(line, refusal.span), "\"unclosed zlib");
        let line = "bind running]into";
        let refusal = tokenize(line, ByteOffset(0)).unwrap_err();
        assert_eq!(
            select(line, refusal.span),
            "running]into",
            "the bare token's refusal spans from the token, not the whole line"
        );
    }

    #[test]
    fn the_written_digest_prefix_names_the_kernels_hash_function() {
        assert_eq!(
            BLAKE3,
            format!("{}:", pith_ids::DIGEST_ALGORITHM),
            "the written prefix and the hasher are one fact, bound here"
        );
    }

    #[test]
    fn a_comma_piece_of_a_feature_list_spans_its_written_spelling() {
        let line = r#"[ok,"two words"]"#;
        let tokens = tokenize(line, ByteOffset(0)).unwrap();
        let list = tokens.first().unwrap();
        let inner = list.text.get(1..list.text.len().saturating_sub(1)).unwrap();
        let pieces = split_commas(inner, ByteOffset(1));
        assert_eq!(
            pieces.first().map(|piece| select(line, piece.span)),
            Some("ok")
        );
        assert_eq!(
            pieces.get(1).map(|piece| select(line, piece.span)),
            Some("\"two words\"")
        );
    }
}
