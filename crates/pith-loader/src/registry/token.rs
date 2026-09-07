//! Tokens for the index's line format.
//!
//! Fields are bare tokens when possible and quoted tokens with backslash
//! escapes otherwise; every token carries the span of its written
//! spelling, so a parse refusal points into the file it was read from.

use pith_diag::{ByteOffset, Span};

/// One written field: the text after quoting is resolved, and the span of
/// its written spelling.
#[derive(Clone)]
pub(crate) struct Token {
    pub(crate) text: String,
    pub(crate) span: Span,
}

impl Token {
    pub(crate) fn is(&self, word: &str) -> bool {
        self.text == word
    }

    pub(crate) fn spelling(&self) -> &str {
        &self.text
    }
}

/// A parse refusal: what is wrong, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Malformed {
    pub message: Box<str>,
    pub span: Span,
}

/// The span from `position`'s token to the line's end: everything after
/// the last well-formed field is suspect together.
pub(crate) fn span_from(tokens: &[Token], position: usize) -> Span {
    let start = tokens
        .get(position)
        .map_or_else(Span::none, |token| token.span);
    let end = tokens.last().map_or(start, |last| last.span);
    Span::new(start.start, end.end)
}

/// Splits a line into tokens on spaces, keeping each token's span. A `#`
/// ends the line as a comment.
///
/// # Errors
/// Returns [`Malformed`] naming the token that cannot be read, spanning
/// from it to the line's end.
pub(crate) fn tokenize(line: &str, base: ByteOffset) -> Result<Vec<Token>, Malformed> {
    let mut tokens = Vec::new();
    let mut rest = line;
    loop {
        rest = rest.trim_start_matches(' ');
        if rest.is_empty() || rest.starts_with('#') {
            return Ok(tokens);
        }
        let start = at(base, line, rest);
        let (text, remaining) = if rest.starts_with('"') {
            quoted_token(rest)
        } else {
            bare_token(rest)
        }
        .map_err(|message| Malformed {
            message: message.into(),
            span: Span::new(start, end_of(base, line)),
        })?;
        tokens.push(Token {
            text,
            span: Span::new(start, at(base, line, remaining)),
        });
        rest = remaining;
    }
}

/// One text field in its written spelling: quoted, with backslash escapes
/// for the characters a quoted token cannot carry.
pub(crate) fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len().saturating_add(2));
    out.push('"');
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn bare_token(rest: &str) -> Result<(String, &str), String> {
    let end = rest.find([' ', '"', '#']).unwrap_or(rest.len());
    let (token, remaining) = rest.split_at(end);
    if remaining.starts_with('"') {
        return Err(format!(
            "the bare token `{token}` runs into a quote; quote the whole token"
        ));
    }
    Ok((token.into(), remaining))
}

fn quoted_token(rest: &str) -> Result<(String, &str), String> {
    let mut token = String::new();
    let mut characters = rest.chars();
    characters.next();
    loop {
        let Some(character) = characters.next() else {
            return Err(format!("the quoted token `{rest}` is never closed"));
        };
        match character {
            '"' => return Ok((token, characters.as_str())),
            '\\' => token.push(escape(&mut characters)?),
            other => token.push(other),
        }
    }
}

fn escape(characters: &mut std::str::Chars<'_>) -> Result<char, String> {
    let Some(escaped) = characters.next() else {
        return Err("an escape ends the token without a character".into());
    };
    match escaped {
        'n' => Ok('\n'),
        'r' => Ok('\r'),
        't' => Ok('\t'),
        '\\' => Ok('\\'),
        '"' => Ok('"'),
        other => Err(format!(
            "the escape `\\{other}` is none of n, r, t, backslash, or quote"
        )),
    }
}

fn end_of(base: ByteOffset, line: &str) -> ByteOffset {
    ByteOffset(
        base.0
            .saturating_add(u32::try_from(line.len()).unwrap_or(u32::MAX)),
    )
}

/// The offset of `rest`, a tail of `line`.
fn at(base: ByteOffset, line: &str, rest: &str) -> ByteOffset {
    ByteOffset(
        base.0.saturating_add(
            u32::try_from(line.len().saturating_sub(rest.len())).unwrap_or(u32::MAX),
        ),
    )
}
