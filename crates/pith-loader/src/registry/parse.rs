//! The index's parsers: one line of a subject's file, and one domain's
//! key-set document, each into the values [`super::line`] renders.
//!
//! Every refusal names the field that is wrong and spans the token that
//! earned it, so the reader attaches it to the file it was read from.

use std::collections::BTreeSet;

use pith_diag::{ByteOffset, Span};
use pith_hir::{ManifestVersion, ModuleSubject};
use pith_ids::ContentDigest;

use super::keys::{Detached, Public};
use super::line::{
    ADMITTED, BY, DIGEST_PREFIX, GENERATION, KEY, KeySet, Line, Pin, REASON, RELEASE, REQUIRES,
    REVISION, Release, SIG, SUBPATH, THRESHOLD, TREE, WITHDRAW, Withdrawal,
};
use super::range;
use super::token::{Malformed, Token, span_from, tokenize};
use crate::ModuleRequirement;

impl Line {
    /// # Errors
    /// [`Malformed`] naming what is not a line.
    pub fn parse(text: &str, base: ByteOffset) -> Result<Self, Malformed> {
        let tokens = tokenize(text, base)?;
        let first = tokens.first().ok_or_else(|| Malformed {
            message: "an index line is never empty".into(),
            span: Span::new(base, base),
        })?;
        match first.spelling() {
            RELEASE => Ok(Self::Release(release(&tokens)?)),
            WITHDRAW => Ok(Self::Withdrawal(withdrawal(&tokens)?)),
            other => Err(Malformed {
                message: format!(
                    "`{other}` names no line kind; a line is `{RELEASE}` or `{WITHDRAW}`"
                ),
                span: first.span,
            }),
        }
    }
}

impl KeySet {
    /// Parses a key-set document, line by line from its first byte.
    ///
    /// # Errors
    /// [`Malformed`] naming the first line that is not the document's shape.
    pub fn parse(text: &str, base: ByteOffset) -> Result<Self, Malformed> {
        let mut lines = field_lines(text, base)?.into_iter();
        let (keyword, value, span) = lines.next().ok_or_else(|| ends_before(base, GENERATION))?;
        if keyword != GENERATION {
            return Err(unexpected(GENERATION, keyword.as_str(), span));
        }
        let generation = count(&value, span)?;
        let (keyword, value, span) = lines.next().ok_or_else(|| ends_before(base, THRESHOLD))?;
        if keyword != THRESHOLD {
            return Err(unexpected(THRESHOLD, keyword.as_str(), span));
        }
        let threshold = u32::try_from(count(&value, span)?).map_err(|_| Malformed {
            message: "a threshold is a count small enough to be one".into(),
            span,
        })?;
        let mut keys = BTreeSet::new();
        let signature = loop {
            let (keyword, value, span) = lines.next().ok_or_else(|| ends_before(base, SIG))?;
            match keyword.as_str() {
                SIG => {
                    break Detached::parse(&value).map_err(|error| Malformed {
                        message: error.to_string(),
                        span,
                    })?;
                }
                KEY => {
                    keys.insert(Public::parse(&value).map_err(|error| Malformed {
                        message: error.to_string(),
                        span,
                    })?);
                }
                other => return Err(unexpected(KEY, other, span)),
            }
        };
        if lines.next().is_some() {
            return Err(Malformed {
                message: "the key set carries nothing after its signature".into(),
                span: Span::new(base, base),
            });
        }
        Ok(KeySet::from_claims(generation, threshold, keys, signature))
    }
}

/// The document's lines, each its keyword and its one field. Every key-set
/// line is two tokens, and empty lines carry nothing.
fn field_lines(text: &str, base: ByteOffset) -> Result<Vec<(String, String, Span)>, Malformed> {
    let mut fields = Vec::new();
    let mut offset = 0usize;
    for raw in text.split_inclusive('\n') {
        let clean = raw.trim_end_matches(['\n', '\r']);
        let line_base = ByteOffset(
            base.0
                .saturating_add(u32::try_from(offset).unwrap_or(u32::MAX)),
        );
        offset = offset.saturating_add(raw.len());
        if clean.is_empty() {
            continue;
        }
        let tokens = tokenize(clean, line_base)?;
        let [keyword, value] = tokens.as_slice() else {
            return Err(Malformed {
                message: "a key-set line is one keyword and one field".into(),
                span: Span::new(
                    line_base,
                    ByteOffset(
                        line_base
                            .0
                            .saturating_add(u32::try_from(clean.len()).unwrap_or(u32::MAX)),
                    ),
                ),
            });
        };
        fields.push((
            keyword.spelling().to_owned(),
            value.text.clone(),
            value.span,
        ));
    }
    Ok(fields)
}

fn ends_before(base: ByteOffset, name: &str) -> Malformed {
    Malformed {
        message: format!("the key set ends before its `{name}` line"),
        span: Span::new(base, base),
    }
}

fn unexpected(expected: &str, found: &str, span: Span) -> Malformed {
    Malformed {
        message: format!("`{found}` names no key-set line here; expected `{expected}`"),
        span,
    }
}

fn count(value: &str, span: Span) -> Result<u64, Malformed> {
    value.parse().map_err(|_| Malformed {
        message: format!("`{value}` is a plain count"),
        span,
    })
}

fn release(tokens: &[Token]) -> Result<Release, Malformed> {
    let mut position = 1;
    let version = version(tokens.get(position), RELEASE)?;
    position = position.saturating_add(1);
    let mut requires = Vec::new();
    while tokens.get(position).is_some_and(|token| token.is(REQUIRES)) {
        let Some(requirement) = requirement(tokens, &mut position) else {
            return Err(Malformed {
                message: format!("`{REQUIRES}` names a subject and a range"),
                span: span_from(tokens, position),
            });
        };
        requires.push(requirement);
    }
    expect_keyword(tokens, &mut position, REVISION)?;
    let revision = field(tokens.get(position), REVISION)?;
    position = position.saturating_add(1);
    let mut subpath: Option<&str> = None;
    if tokens.get(position).is_some_and(|token| token.is(SUBPATH)) {
        position = position.saturating_add(1);
        subpath = Some(field(tokens.get(position), SUBPATH)?);
        position = position.saturating_add(1);
    }
    let pin = match subpath {
        Some(subpath) => Pin::at_subpath(revision, subpath),
        None => Pin::new(revision),
    };
    expect_keyword(tokens, &mut position, TREE)?;
    let tree = digest(tokens.get(position))?;
    position = position.saturating_add(1);
    expect_keyword(tokens, &mut position, BY)?;
    let signer = public(tokens.get(position))?;
    position = position.saturating_add(1);
    expect_keyword(tokens, &mut position, SIG)?;
    let signature = signature(tokens.get(position))?;
    position = position.saturating_add(1);
    expect_keyword(tokens, &mut position, ADMITTED)?;
    let admitted = number(tokens.get(position), ADMITTED)?;
    if tokens.get(position.saturating_add(1)).is_some() {
        return Err(Malformed {
            message: format!("the release line carries nothing after `{ADMITTED}`"),
            span: span_from(tokens, position),
        });
    }
    Ok(Release::from_claims(
        version,
        requires.into(),
        pin,
        tree,
        signer,
        signature,
        admitted,
    ))
}

/// One `requires <subject> <range>` clause, advancing `position` past it.
fn requirement(tokens: &[Token], position: &mut usize) -> Option<ModuleRequirement> {
    let subject = tokens.get(position.saturating_add(1))?;
    let range = tokens.get(position.saturating_add(2))?;
    let subject = ModuleSubject::parse(subject.spelling()).ok()?;
    let range = range::parse(range).ok()?;
    *position = position.saturating_add(3);
    Some(ModuleRequirement { subject, range })
}

fn withdrawal(tokens: &[Token]) -> Result<Withdrawal, Malformed> {
    let mut position = 1;
    let version = version(tokens.get(position), WITHDRAW)?;
    position = position.saturating_add(1);
    expect_keyword(tokens, &mut position, REASON)?;
    let reason = field(tokens.get(position), REASON)?;
    position = position.saturating_add(1);
    expect_keyword(tokens, &mut position, BY)?;
    let signer = public(tokens.get(position))?;
    position = position.saturating_add(1);
    expect_keyword(tokens, &mut position, SIG)?;
    let signature = signature(tokens.get(position))?;
    position = position.saturating_add(1);
    if tokens.get(position).is_some() {
        return Err(Malformed {
            message: format!("the withdrawal line carries nothing after its `{SIG}`"),
            span: span_from(tokens, position),
        });
    }
    Ok(Withdrawal::from_claims(
        version,
        reason.into(),
        signer,
        signature,
    ))
}

fn expect_keyword(tokens: &[Token], position: &mut usize, word: &str) -> Result<(), Malformed> {
    if tokens.get(*position).is_some_and(|token| token.is(word)) {
        *position = position.saturating_add(1);
        return Ok(());
    }
    Err(Malformed {
        message: format!("the release line carries `{word}` here"),
        span: span_from(tokens, *position),
    })
}

fn public(token: Option<&Token>) -> Result<Public, Malformed> {
    Public::parse(field(token, BY)?).map_err(|error| Malformed {
        message: error.to_string(),
        span: token.map_or_else(Span::none, |token| token.span),
    })
}

fn version(token: Option<&Token>, line: &str) -> Result<ManifestVersion, Malformed> {
    let spelling = field(token, line)?;
    ManifestVersion::parse(spelling).map_err(|_| Malformed {
        message: format!("the {line} version `{spelling}` is dotted numeric segments"),
        span: token.map_or_else(Span::none, |token| token.span),
    })
}

fn field<'token>(token: Option<&'token Token>, name: &str) -> Result<&'token str, Malformed> {
    token.map(Token::spelling).ok_or_else(|| Malformed {
        message: format!("the line ends before its `{name}` field"),
        span: Span::none(),
    })
}

fn digest(token: Option<&Token>) -> Result<ContentDigest, Malformed> {
    let spelling = field(token, TREE)?;
    spelling
        .strip_prefix(DIGEST_PREFIX)
        .ok_or_else(|| Malformed {
            message: format!("the tree is `{DIGEST_PREFIX}` followed by hexadecimal"),
            span: token.map_or_else(Span::none, |token| token.span),
        })?
        .parse()
        .map_err(|_| Malformed {
            message: format!("`{spelling}` is not a digest"),
            span: token.map_or_else(Span::none, |token| token.span),
        })
}

fn signature(token: Option<&Token>) -> Result<Detached, Malformed> {
    Detached::parse(field(token, SIG)?).map_err(|error| Malformed {
        message: error.to_string(),
        span: token.map_or_else(Span::none, |token| token.span),
    })
}

fn number(token: Option<&Token>, name: &str) -> Result<u64, Malformed> {
    field(token, name)?.parse().map_err(|_| Malformed {
        message: format!("the {name} is a plain count"),
        span: token.map_or_else(Span::none, |token| token.span),
    })
}
