use std::sync::Arc;

use pith_diag::{ByteOffset, Diag, Severity, SourceFile, Span};

use crate::FrontendCode;

#[derive(Clone)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub text: Option<Box<str>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Ident,
    Str,
    LineComment,
    Arrow,
    Colon,
    Comma,
    Dot,
    Eq,
    Minus,
    Pipe,
    Lt,
    Gt,
    LParen,
    RParen,
    LBrace,
    RBrace,
    End,
}

pub(crate) const KEYWORDS: &[&str] = &[
    "import", "nominal", "sum", "type", "pure", "action", "rule", "host", "List", "Unit", "Bool",
    "Int", "Text", "Bytes", "Blob",
];

pub(crate) fn lex(source: &Arc<SourceFile>) -> (Vec<Token>, Vec<Diag>) {
    let text = source.source_text();
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let mut position = 0usize;

    while let Some(remaining) = bytes.get(position..) {
        let Some((&byte, after_first)) = remaining.split_first() else {
            break;
        };
        let start = position;
        let (kind, decoded) = match remaining {
            [b'-', b'-', ..] => {
                position = line_end(bytes, start.saturating_add(2));
                (TokenKind::LineComment, None)
            }
            [b'-', b'>', ..] => {
                position = start.saturating_add(2);
                (TokenKind::Arrow, None)
            }
            _ => match byte {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    position = start.saturating_add(1);
                    continue;
                }
                b'"' => match read_string(text, start) {
                    Ok((end, value)) => {
                        position = end;
                        (TokenKind::Str, Some(value.into_boxed_str()))
                    }
                    Err(string_error) => {
                        diagnostics.push(error(
                            FrontendCode::InvalidString,
                            string_error.span,
                            string_error.message,
                            source,
                        ));
                        position = string_error.next;
                        continue;
                    }
                },
                b':' => single(TokenKind::Colon, &mut position, start),
                b',' => single(TokenKind::Comma, &mut position, start),
                b'.' => single(TokenKind::Dot, &mut position, start),
                b'=' => single(TokenKind::Eq, &mut position, start),
                b'-' => single(TokenKind::Minus, &mut position, start),
                b'|' => single(TokenKind::Pipe, &mut position, start),
                b'<' => single(TokenKind::Lt, &mut position, start),
                b'>' => single(TokenKind::Gt, &mut position, start),
                b'(' => single(TokenKind::LParen, &mut position, start),
                b')' => single(TokenKind::RParen, &mut position, start),
                b'{' => single(TokenKind::LBrace, &mut position, start),
                b'}' => single(TokenKind::RBrace, &mut position, start),
                byte if is_ident_start(byte) => {
                    let continuation = after_first
                        .iter()
                        .position(|candidate| !is_ident_continue(*candidate))
                        .unwrap_or(after_first.len());
                    position = start.saturating_add(1).saturating_add(continuation);
                    (TokenKind::Ident, None)
                }
                _ => {
                    let character = text.get(start..).and_then(|rest| rest.chars().next());
                    let width = character.map_or(1, char::len_utf8);
                    let spelling = character.map_or_else(
                        || format!("byte 0x{byte:02x}"),
                        |character| character.to_string(),
                    );
                    position = start.saturating_add(width);
                    diagnostics.push(error(
                        FrontendCode::UnexpectedToken,
                        span(start, position),
                        format!("`{spelling}` is not part of the declaration grammar"),
                        source,
                    ));
                    continue;
                }
            },
        };
        tokens.push(Token {
            kind,
            span: span(start, position),
            text: decoded,
        });
    }

    tokens.push(Token {
        kind: TokenKind::End,
        span: Span::point(ByteOffset(offset_from(position))),
        text: None,
    });
    (tokens, diagnostics)
}

pub(crate) fn error(
    code: FrontendCode,
    span: Span,
    message: impl Into<String>,
    source: &Arc<SourceFile>,
) -> Diag {
    Diag::new(Severity::Error, code.stable(), span, message.into()).with_source(source.clone())
}

fn single(kind: TokenKind, position: &mut usize, start: usize) -> (TokenKind, Option<Box<str>>) {
    *position = start.saturating_add(1);
    (kind, None)
}

fn line_end(bytes: &[u8], from: usize) -> usize {
    bytes.get(from..).map_or(bytes.len(), |remaining| {
        remaining
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |offset| from.saturating_add(offset))
    })
}

fn span(start: usize, end: usize) -> Span {
    Span::new(ByteOffset(offset_from(start)), ByteOffset(offset_from(end)))
}

fn offset_from(position: usize) -> u32 {
    u32::try_from(position).unwrap_or(u32::MAX)
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

struct StringError {
    span: Span,
    message: String,
    next: usize,
}

fn read_string(text: &str, start: usize) -> Result<(usize, String), StringError> {
    let bytes = text.as_bytes();
    let mut decoded = String::new();
    let mut position = start.saturating_add(1);
    loop {
        let Some(remaining) = bytes.get(position..) else {
            return Err(unterminated(start, position));
        };
        match remaining {
            [b'"', ..] => return Ok((position.saturating_add(1), decoded)),
            [b'\n', ..] | [] => return Err(unterminated(start, position)),
            [b'\\', b'"', ..] => {
                decoded.push('"');
                position = position.saturating_add(2);
            }
            [b'\\', b'\\', ..] => {
                decoded.push('\\');
                position = position.saturating_add(2);
            }
            [b'\\', ..] => {
                return Err(StringError {
                    span: span(position, position.saturating_add(1)),
                    message: "only `\\\"` and `\\\\` are escapes in a string literal".to_owned(),
                    next: line_end(bytes, position),
                });
            }
            _ => {
                let Some(character) = text.get(position..).and_then(|rest| rest.chars().next())
                else {
                    return Err(unterminated(start, position));
                };
                decoded.push(character);
                position = position.saturating_add(character.len_utf8());
            }
        }
    }
}

fn unterminated(start: usize, position: usize) -> StringError {
    StringError {
        span: span(start, position),
        message: "the string literal is not closed".to_owned(),
        next: position,
    }
}
