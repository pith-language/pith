//! Tokenizing line-oriented text: bare and quoted tokens, bracketed
//! groups, comments — every token carrying the span of its written
//! spelling in the line it was read from.
//!
//! The module knows no format of its own. It is the shared reading half
//! of wire formats that render one fact per line: the caller defines the
//! keywords and the fields, and every refusal here selects the offending
//! token so the caller can attach it to the source it holds.

use std::fmt::Write as _;

use crate::{ByteOffset, Span};

/// One written field: the text after quoting is resolved, and the span of
/// its written spelling in the source.
#[derive(Debug, Clone)]
pub struct Token {
    pub text: String,
    pub span: Span,
}

impl Token {
    /// Whether the token is exactly `word`.
    #[must_use]
    pub fn is(&self, word: &str) -> bool {
        self.text == word
    }

    /// The token's resolved text.
    #[must_use]
    pub fn spelling(&self) -> &str {
        &self.text
    }
}

/// A refusal from tokenizing: what is wrong, and the span it happened in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub message: String,
    pub span: Span,
}

/// One text field in its written spelling: bare when it contains none of
/// the reserved characters, quoted with backslash escapes otherwise.
#[must_use]
pub fn token(text: &str) -> String {
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

/// Whether `text` needs no quoting.
#[must_use]
pub fn is_bare(text: &str) -> bool {
    !text.is_empty()
        && text.chars().all(|character| {
            !character.is_whitespace()
                && !character.is_control()
                && !matches!(character, '"' | '#' | '\\' | '[' | ']')
        })
}

/// Splits a line into tokens on spaces, keeping each token's span. A span
/// runs from the token's first character to the character after its last,
/// quoted material included; a `[`-bracketed group stays one token with
/// quoting honored inside it; an unquoted `#` ends the line as a comment.
///
/// # Errors
/// Returns a [`Refusal`] spanning from the offending token's start
/// through the end of the line, because every way a token fails leaves
/// the rest of the line suspect.
pub fn tokenize(line: &str, base: ByteOffset) -> Result<Vec<Token>, Refusal> {
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

/// Reads one quoted token from the start of `rest`: the resolved text,
/// and the rest of the line after its closing quote.
///
/// # Errors
/// Returns why the token cannot be read: it is never closed, or it ends
/// on an escape.
pub fn quoted_token(rest: &str) -> Result<(String, &str), String> {
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

fn end_of(base: ByteOffset, line: &str) -> ByteOffset {
    ByteOffset(
        base.0
            .saturating_add(u32::try_from(line.len()).unwrap_or(u32::MAX)),
    )
}

/// The byte offset of `rest`, a tail of `line`.
fn at(base: ByteOffset, line: &str, rest: &str) -> ByteOffset {
    ByteOffset(
        base.0.saturating_add(
            u32::try_from(line.len().saturating_sub(rest.len())).unwrap_or(u32::MAX),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(line: &str) -> Vec<String> {
        tokenize(line, ByteOffset(0))
            .expect("the fixture line tokenizes")
            .into_iter()
            .map(|token| token.text)
            .collect()
    }

    #[test]
    fn bare_and_quoted_tokens_round_trip() {
        assert_eq!(
            tokens("a b c"),
            ["a", "b", "c"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(token("plain"), "plain");
        assert_eq!(token("two words"), "\"two words\"");
        assert_eq!(token("quote\"and\\slash"), "\"quote\\\"and\\\\slash\"");
        assert_eq!(tokens(&token("two words")), ["two words".to_string()]);
    }

    #[test]
    fn comments_end_a_line_and_spans_track_written_spellings() {
        let line = "alpha \"two words\" # trailing";
        let tokens = tokenize(line, ByteOffset(0)).expect("the line tokenizes");
        assert_eq!(tokens.len(), 2);
        let [alpha, quoted] = tokens.as_slice() else {
            unreachable!("the line tokenizes into two tokens");
        };
        assert_eq!(alpha.span, Span::new(ByteOffset(0), ByteOffset(5)));
        assert_eq!(quoted.span, Span::new(ByteOffset(6), ByteOffset(17)));
    }

    #[test]
    fn an_unterminated_quote_refuses_spanning_to_the_lines_end() {
        let refusal =
            tokenize("\"never closed", ByteOffset(3)).expect_err("the token is never closed");
        assert_eq!(refusal.span, Span::new(ByteOffset(3), ByteOffset(16)));
    }
}
