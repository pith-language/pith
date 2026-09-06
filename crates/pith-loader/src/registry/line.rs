//! The index's line format: one fact per line, canonically rendered,
//! parsed with spans into the file it was read from.
//!
//! A signature covers the canonical rendering of a line's values —
//! everything before the `sig` field — so equal values verify equal and
//! any edited value does not. The registry's admission time follows the
//! signature, asserted by the host rather than the publisher and never
//! covered by either.

use std::collections::BTreeSet;

use pith_diag::{ByteOffset, Span};
use pith_hir::{ManifestVersion, ModuleSubject, VersionBound, VersionRange};
use pith_ids::ContentDigest;

use super::keys::{Detached, Public, Signing};
use crate::ModuleRequirement;

const RELEASE: &str = "release";
const WITHDRAW: &str = "withdraw";
const REQUIRES: &str = "requires";
const REVISION: &str = "revision";
const SUBPATH: &str = "subpath";
const TREE: &str = "tree";
const BY: &str = "by";
const SIG: &str = "sig";
const ADMITTED: &str = "admitted";
const REASON: &str = "reason";
const GENERATION: &str = "generation";
const THRESHOLD: &str = "threshold";
const KEY: &str = "key";

/// The digest prefix, bound to the hasher's own name by test so the
/// written spelling and the algorithm cannot drift apart.
const DIGEST_PREFIX: &str = "blake3:";

/// One written field: the text after quoting is resolved, and the span of
/// its written spelling.
#[derive(Clone)]
pub(crate) struct Token {
    text: String,
    span: Span,
}

impl Token {
    fn is(&self, word: &str) -> bool {
        self.text == word
    }

    fn spelling(&self) -> &str {
        &self.text
    }
}

/// A parse refusal: what is wrong, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Malformed {
    pub message: Box<str>,
    pub span: Span,
}

/// One line of a subject's index file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Release(Release),
    Withdrawal(Withdrawal),
}

impl Line {
    /// # Errors
    /// Returns [`Malformed`] naming the field that is not a line.
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
                )
                .into(),
                span: first.span,
            }),
        }
    }

    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::Release(release) => release.render(),
            Self::Withdrawal(withdrawal) => withdrawal.render(),
        }
    }
}

/// A revision pin: the immutable revision a tree was measured from, as
/// the serving host spells it, and the subpath holding the module root
/// when the revision holds more than the module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    revision: Box<str>,
    subpath: Option<Box<str>>,
}

impl Pin {
    #[must_use]
    pub fn new(revision: impl Into<Box<str>>) -> Self {
        Self {
            revision: revision.into(),
            subpath: None,
        }
    }

    #[must_use]
    pub fn at_subpath(revision: impl Into<Box<str>>, subpath: impl Into<Box<str>>) -> Self {
        Self {
            revision: revision.into(),
            subpath: Some(subpath.into()),
        }
    }

    #[must_use]
    pub fn revision(&self) -> &str {
        &self.revision
    }

    #[must_use]
    pub fn subpath(&self) -> Option<&str> {
        self.subpath.as_deref()
    }
}

/// A release line: what one version of one subject is, which publisher
/// key asserts it, and where the signature over that claim sits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    version: ManifestVersion,
    requires: Box<[ModuleRequirement]>,
    pin: Pin,
    tree: ContentDigest,
    signer: Public,
    signature: Detached,
    admitted: u64,
}

impl Release {
    #[must_use]
    pub fn version(&self) -> &ManifestVersion {
        &self.version
    }

    #[must_use]
    pub fn requires(&self) -> &[ModuleRequirement] {
        &self.requires
    }

    #[must_use]
    pub const fn pin(&self) -> &Pin {
        &self.pin
    }

    #[must_use]
    pub const fn tree(&self) -> ContentDigest {
        self.tree
    }

    /// The key the line names as its signer, so a refusal can name the
    /// key and the set it is absent from.
    #[must_use]
    pub const fn signer(&self) -> &Public {
        &self.signer
    }

    #[must_use]
    pub const fn signature(&self) -> &Detached {
        &self.signature
    }

    /// When the registry admitted the entry, in seconds since the unix
    /// epoch. Asserted by the host, covered by neither signature.
    #[must_use]
    pub const fn admitted(&self) -> u64 {
        self.admitted
    }

    /// The bytes a publisher's signature covers: the canonical rendering
    /// of every claim the publisher makes.
    #[must_use]
    pub fn render_unsigned(&self) -> String {
        let mut line = format!("{RELEASE} {}", self.version.canonical_spelling());
        for requirement in &self.requires {
            line.push_str(&format!(
                " {REQUIRES} {} {}",
                requirement.subject,
                requirement_range(&requirement.range)
            ));
        }
        line.push_str(&format!(" {REVISION} {}", self.pin.revision));
        if let Some(subpath) = &self.pin.subpath {
            line.push_str(&format!(" {SUBPATH} {}", quoted(subpath)));
        }
        line.push_str(&format!(
            " {TREE} {DIGEST_PREFIX}{} {BY} {}",
            self.tree,
            self.signer.spelling()
        ));
        line
    }

    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "{} {SIG} {} {ADMITTED} {}",
            self.render_unsigned(),
            self.signature.spelling(),
            self.admitted
        )
    }

    /// Assembles a release from derived claims, a signature over
    /// [`Self::render_unsigned`], and the host's admission time.
    #[must_use]
    pub fn from_claims(
        version: ManifestVersion,
        requires: Box<[ModuleRequirement]>,
        pin: Pin,
        tree: ContentDigest,
        signer: Public,
        signature: Detached,
        admitted: u64,
    ) -> Self {
        Self {
            version,
            requires,
            pin,
            tree,
            signer,
            signature,
            admitted,
        }
    }

    /// The release those claims describe, signed: the signature covers the
    /// canonical rendering of everything but itself and the admission
    /// time, which arrive after it.
    #[must_use]
    pub fn signed(
        version: ManifestVersion,
        requires: Box<[ModuleRequirement]>,
        pin: Pin,
        tree: ContentDigest,
        signer: &Signing,
        admitted: u64,
    ) -> Self {
        let unsigned = Self::from_claims(
            version.clone(),
            requires.clone(),
            pin.clone(),
            tree,
            signer.public(),
            signer.sign(&[]),
            admitted,
        )
        .render_unsigned();
        Self::from_claims(
            version,
            requires,
            pin,
            tree,
            signer.public(),
            signer.sign(unsigned.as_bytes()),
            admitted,
        )
    }
}

/// A withdrawal line: one version marked unfit, with a reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Withdrawal {
    version: ManifestVersion,
    reason: Box<str>,
    signer: Public,
    signature: Detached,
}

impl Withdrawal {
    #[must_use]
    pub fn version(&self) -> &ManifestVersion {
        &self.version
    }

    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    #[must_use]
    pub const fn signer(&self) -> &Public {
        &self.signer
    }

    #[must_use]
    pub const fn signature(&self) -> &Detached {
        &self.signature
    }

    /// The bytes the issuer's signature covers.
    #[must_use]
    pub fn render_unsigned(&self) -> String {
        format!(
            "{WITHDRAW} {} {REASON} {} {BY} {}",
            self.version.canonical_spelling(),
            quoted(&self.reason),
            self.signer.spelling()
        )
    }

    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "{} {SIG} {}",
            self.render_unsigned(),
            self.signature.spelling()
        )
    }

    #[must_use]
    pub fn from_claims(
        version: ManifestVersion,
        reason: Box<str>,
        signer: Public,
        signature: Detached,
    ) -> Self {
        Self {
            version,
            reason,
            signer,
            signature,
        }
    }

    /// The withdrawal those claims describe, signed by `issuer`.
    #[must_use]
    pub fn signed(version: ManifestVersion, reason: Box<str>, issuer: &Signing) -> Self {
        let unsigned = Self::from_claims(
            version.clone(),
            reason.clone(),
            issuer.public(),
            issuer.sign(&[]),
        )
        .render_unsigned();
        Self::from_claims(
            version,
            reason,
            issuer.public(),
            issuer.sign(unsigned.as_bytes()),
        )
    }
}

/// A domain's key-set document: the keys whose signatures speak for the
/// domain, how many must sign, and the generation that advances when the
/// set changes. Signed by the registry's root key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeySet {
    generation: u64,
    threshold: u32,
    keys: BTreeSet<Public>,
    signature: Detached,
}

impl KeySet {
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn threshold(&self) -> u32 {
        self.threshold
    }

    #[must_use]
    pub fn keys(&self) -> &BTreeSet<Public> {
        &self.keys
    }

    #[must_use]
    pub const fn signature(&self) -> &Detached {
        &self.signature
    }

    /// The bytes the root key's signature covers.
    #[must_use]
    pub fn render_unsigned(&self) -> String {
        let mut document = format!(
            "{GENERATION} {}\n{THRESHOLD} {}",
            self.generation, self.threshold
        );
        for key in &self.keys {
            document.push_str(&format!("\n{KEY} {}", key.spelling()));
        }
        document
    }

    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "{}\n{SIG} {}\n",
            self.render_unsigned(),
            self.signature.spelling()
        )
    }

    #[must_use]
    pub fn from_claims(
        generation: u64,
        threshold: u32,
        keys: BTreeSet<Public>,
        signature: Detached,
    ) -> Self {
        Self {
            generation,
            threshold,
            keys,
            signature,
        }
    }

    /// The key set those claims describe, signed by `root`.
    #[must_use]
    pub fn signed(generation: u64, threshold: u32, keys: BTreeSet<Public>, root: &Signing) -> Self {
        let unsigned = Self::from_claims(generation, threshold, keys.clone(), root.sign(&[]))
            .render_unsigned();
        Self::from_claims(generation, threshold, keys, root.sign(unsigned.as_bytes()))
    }

    /// Parses a key-set document, line by line from its first byte.
    ///
    /// # Errors
    /// Returns [`Malformed`] naming the first line that is not the
    /// document's shape.
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
                        message: error.to_string().into(),
                        span,
                    })?;
                }
                KEY => {
                    keys.insert(Public::parse(&value).map_err(|error| Malformed {
                        message: error.to_string().into(),
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
        Ok(Self::from_claims(generation, threshold, keys, signature))
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
                span: Span::new(line_base, end_of(line_base, clean)),
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
        message: format!("the key set ends before its `{name}` line").into(),
        span: Span::new(base, base),
    }
}

fn unexpected(expected: &str, found: &str, span: Span) -> Malformed {
    Malformed {
        message: format!("`{found}` names no key-set line here; expected `{expected}`").into(),
        span,
    }
}

fn count(value: &str, span: Span) -> Result<u64, Malformed> {
    value.parse().map_err(|_| Malformed {
        message: format!("`{value}` is a plain count").into(),
        span,
    })
}

/// A version range in its written spelling: `*`, `=1.2`, `>=1.2`, `>1.2`,
/// `<=2.0`, `<2.0`, or a comma-joined lower and upper edge such as
/// `>=1.2,<2.0`. Each edge carries its own inclusivity, so the manifest's
/// closed constructor set reads and writes without loss.
fn requirement_range(range: &VersionRange) -> String {
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

fn parse_requirement_range(token: &Token) -> Result<VersionRange, Malformed> {
    let refuse = |message: String| Malformed {
        message: message.into(),
        span: token.span,
    };
    if token.is("*") {
        return Ok(VersionRange::Any);
    }
    let mut edges = Vec::new();
    for edge in token.spelling().split(',') {
        let bound = |spelling: &str| {
            ManifestVersion::parse(spelling).map_err(|_| {
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
            Ok(VersionRange::AtLeast(edge_bound(inclusive, version)))
        }
        [Edge::AtMost { inclusive, version }] => {
            Ok(VersionRange::AtMost(edge_bound(inclusive, version)))
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
            lower: edge_bound(lower_inclusive, lower),
            upper: edge_bound(upper_inclusive, upper),
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
    Exactly(ManifestVersion),
    AtLeast {
        inclusive: bool,
        version: ManifestVersion,
    },
    AtMost {
        inclusive: bool,
        version: ManifestVersion,
    },
}

fn edge_bound(inclusive: &bool, version: &ManifestVersion) -> VersionBound {
    VersionBound {
        version: version.clone(),
        inclusive: *inclusive,
    }
}

fn release(tokens: &[Token]) -> Result<Release, Malformed> {
    let mut position = 1;
    let version = version(tokens.get(position), RELEASE)?;
    position = position.saturating_add(1);
    let mut requires = Vec::new();
    while tokens.get(position).is_some_and(|token| token.is(REQUIRES)) {
        let subject = tokens.get(position.saturating_add(1)).map(Token::spelling);
        let range = tokens.get(position.saturating_add(2));
        let Some((subject, range)) = subject.zip(range) else {
            return Err(Malformed {
                message: format!("`{REQUIRES}` names a subject and a range").into(),
                span: span_from(tokens, position),
            });
        };
        let subject = ModuleSubject::parse(subject).map_err(|error| Malformed {
            message: error.to_string().into(),
            span: tokens
                .get(position.saturating_add(1))
                .map_or_else(|| span_from(tokens, position), |token| token.span),
        })?;
        let range = parse_requirement_range(range)?;
        requires.push(ModuleRequirement { subject, range });
        position = position.saturating_add(3);
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
            message: format!("the release line carries nothing after `{ADMITTED}`").into(),
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

fn expect_keyword(tokens: &[Token], position: &mut usize, word: &str) -> Result<(), Malformed> {
    if tokens.get(*position).is_some_and(|token| token.is(word)) {
        *position = position.saturating_add(1);
        return Ok(());
    }
    Err(Malformed {
        message: format!("the release line carries `{word}` here").into(),
        span: span_from(tokens, *position),
    })
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
            message: format!("the withdrawal line carries nothing after its `{SIG}`").into(),
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

fn public(token: Option<&Token>) -> Result<Public, Malformed> {
    Public::parse(field(token, BY)?).map_err(|error| Malformed {
        message: error.to_string().into(),
        span: token.map_or_else(Span::none, |token| token.span),
    })
}

fn version(token: Option<&Token>, line: &str) -> Result<ManifestVersion, Malformed> {
    let spelling = field(token, line)?;
    ManifestVersion::parse(spelling).map_err(|_| Malformed {
        message: format!("the {line} version `{spelling}` is dotted numeric segments").into(),
        span: token.map_or_else(Span::none, |token| token.span),
    })
}

fn field<'token>(token: Option<&'token Token>, name: &str) -> Result<&'token str, Malformed> {
    token.map(Token::spelling).ok_or_else(|| Malformed {
        message: format!("the line ends before its `{name}` field").into(),
        span: Span::none(),
    })
}

fn digest(token: Option<&Token>) -> Result<ContentDigest, Malformed> {
    let spelling = field(token, TREE)?;
    spelling
        .strip_prefix(DIGEST_PREFIX)
        .ok_or_else(|| Malformed {
            message: format!("the tree is `{DIGEST_PREFIX}` followed by hexadecimal").into(),
            span: token.map_or_else(Span::none, |token| token.span),
        })?
        .parse()
        .map_err(|_| Malformed {
            message: format!("`{spelling}` is not a digest").into(),
            span: token.map_or_else(Span::none, |token| token.span),
        })
}

fn signature(token: Option<&Token>) -> Result<Detached, Malformed> {
    Detached::parse(field(token, SIG)?).map_err(|error| Malformed {
        message: error.to_string().into(),
        span: token.map_or_else(Span::none, |token| token.span),
    })
}

fn number(token: Option<&Token>, name: &str) -> Result<u64, Malformed> {
    field(token, name)?.parse().map_err(|_| Malformed {
        message: format!("the {name} is a plain count").into(),
        span: token.map_or_else(Span::none, |token| token.span),
    })
}

/// The span from `position`'s token to the line's end: everything after
/// the last well-formed field is suspect together.
fn span_from(tokens: &[Token], position: usize) -> Span {
    let start = tokens
        .get(position)
        .map_or_else(Span::none, |token| token.span);
    let end = tokens.last().map_or(start, |last| last.span);
    Span::new(start.start, end.end)
}

fn quoted(text: &str) -> String {
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

fn tokenize(line: &str, base: ByteOffset) -> Result<Vec<Token>, Malformed> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_release() -> Release {
        let publisher = Signing::from_seed([3; ed25519_dalek::SECRET_KEY_LENGTH]);
        Release::signed(
            ManifestVersion::parse("1.2.0").unwrap(),
            Box::new([ModuleRequirement {
                subject: ModuleSubject::parse("example/dep").unwrap(),
                range: VersionRange::Between {
                    lower: VersionBound {
                        version: ManifestVersion::parse("1.2").unwrap(),
                        inclusive: true,
                    },
                    upper: VersionBound {
                        version: ManifestVersion::parse("2.0").unwrap(),
                        inclusive: false,
                    },
                },
            }]),
            Pin::at_subpath("abc123", "nested/module"),
            ContentDigest::of_bytes(b"tree"),
            &publisher,
            1_757_000_000,
        )
    }

    #[test]
    fn release_lines_round_trip() {
        let release = signed_release();
        let rendered = release.render();
        let parsed = match Line::parse(&rendered, ByteOffset(0)).expect("the line parses") {
            Line::Release(parsed) => parsed,
            other => unreachable!("a release line parses as one: {other:?}"),
        };
        assert_eq!(parsed, release);
        assert_eq!(parsed.render(), rendered);
    }

    #[test]
    fn a_re_spelled_line_with_equal_values_is_equal() {
        let release = signed_release();
        let spaced = release.render().replace(" sig ", "  sig  ");
        let parsed = match Line::parse(&spaced, ByteOffset(0)).expect("the line parses") {
            Line::Release(parsed) => parsed,
            _ => unreachable!("a release line parses as one"),
        };
        assert_eq!(parsed, release);
    }

    #[test]
    fn key_sets_round_trip_and_refuse_trailing_lines() {
        let root = Signing::from_seed([1; ed25519_dalek::SECRET_KEY_LENGTH]);
        let publisher = Signing::from_seed([3; ed25519_dalek::SECRET_KEY_LENGTH]);
        let key_set = KeySet::from_claims(
            2,
            1,
            BTreeSet::from([publisher.public()]),
            root.sign(b"contents"),
        );
        let unsigned = key_set.render_unsigned();
        let signed = KeySet::from_claims(
            2,
            1,
            BTreeSet::from([publisher.public()]),
            root.sign(unsigned.as_bytes()),
        );
        let rendered = signed.render();
        assert_eq!(KeySet::parse(&rendered, ByteOffset(0)), Ok(signed));
        let trailing = format!("{rendered}extra\n");
        assert!(KeySet::parse(&trailing, ByteOffset(0)).is_err());
    }

    #[test]
    fn range_tokens_round_trip_over_the_constructor_set() {
        let version = |spelling: &str| ManifestVersion::parse(spelling).unwrap();
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
                text: requirement_range(&range),
                span: Span::none(),
            };
            assert_eq!(parse_requirement_range(&token).as_ref(), Ok(&range));
        }
    }

    #[test]
    fn the_digest_prefix_is_the_hashers_name() {
        assert_eq!(DIGEST_PREFIX, format!("{}:", pith_ids::DIGEST_ALGORITHM));
    }
}
