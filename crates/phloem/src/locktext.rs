//! Tokens, quoting, feature lists, and digest spelling for lock text.
//!
//! Fields use bare tokens when possible and quoted tokens with backslash
//! escapes otherwise. Digests render as `sha256:` followed by lowercase
//! hexadecimal digits; parsing also accepts uppercase digits.

use std::fmt::Write as _;

use pith_diag::PithResult;
use pith_ids::{ContentId, DIGEST_LEN};

use crate::codec::digest_from_hex;
use crate::diag;

const HEX_LEN: usize = DIGEST_LEN * 2;

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

/// Split a line into tokens on spaces. Double-quoted tokens may contain any
/// character through backslash escapes; a `[`-bracketed group stays one
/// token with quoting honored inside it; an unquoted `#` ends the line as a
/// comment.
pub(crate) fn tokenize(line: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut rest = line;
    loop {
        rest = rest.trim_start_matches(' ');
        if rest.is_empty() {
            return Ok(tokens);
        }
        if rest.starts_with('#') {
            return Ok(tokens);
        }
        if let Some(remaining) = rest.strip_prefix('[') {
            let (group, remaining) = bracket_group(remaining)?;
            tokens.push(format!("[{group}]"));
            rest = remaining;
            continue;
        }
        let (token, remaining) = if rest.starts_with('"') {
            quoted_token(rest)?
        } else {
            bare_token(rest)?
        };
        tokens.push(token);
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

/// Parses a bracketed, comma-separated feature list.
///
/// # Errors
/// Returns a diagnostic when the list or one of its tokens is invalid.
pub(crate) fn parse_features(text: &str, number: usize) -> PithResult<Box<[Box<str>]>> {
    let Some(inner) = text
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return Err(diag(format!(
            "line {number}: `{text}` is not a bracketed feature list"
        )));
    };
    if inner.is_empty() {
        return Ok(Box::new([]));
    }
    let mut features = Vec::new();
    for piece in split_commas(inner) {
        features.push(
            unquote(&piece)
                .map_err(|message| diag(format!("line {number}: {message}")))?
                .into(),
        );
    }
    Ok(features.into())
}

/// Split bracket-group content on commas, keeping quoted pieces whole.
fn split_commas(inner: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut chars = inner.chars();
    while let Some(character) = chars.next() {
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
                pieces.push(current.clone());
                current.clear();
            }
            other => current.push(other),
        }
    }
    pieces.push(current);
    pieces
}

fn unquote(piece: &str) -> Result<String, String> {
    let piece = piece.trim();
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

/// Parses `sha256:` followed by 64 hexadecimal digits.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming the line and the field when the
/// text is not a digest of this shape.
pub(crate) fn parse_digest(text: &str, field: &str, number: usize) -> PithResult<ContentId> {
    let Some(hex) = text.strip_prefix(SHA256) else {
        return Err(diag(format!(
            "line {number}: {field} carried `{text}` rather than a `sha256:`-prefixed digest"
        )));
    };
    if !hex.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(diag(format!(
            "line {number}: {field} carried `{hex}`, which is not hexadecimal"
        )));
    }
    let digest = digest_from_hex(hex).ok_or_else(|| {
        diag(format!(
            "line {number}: {field} carried {} hexadecimal digits rather than {HEX_LEN}",
            hex.len()
        ))
    })?;
    Ok(ContentId::from_digest(digest))
}

/// The digest prefix the written form spells, shared by every directive and
/// binding line that names content.
pub(crate) const SHA256: &str = "sha256:";
