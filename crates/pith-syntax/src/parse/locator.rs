//! The vocabulary of an input's locator and the registry and domain
//! clauses that route it: subjects, versions, ranges, and the quoted
//! values they carry.

use pith_diag::Span;
use pith_hir::{
    FrontendCode, InputLocator, ModuleSubject, ModuleVersion, ProjectDomain, ProjectRegistry,
    RootKey, SubjectSegment, VersionBound, VersionRange,
};

use super::Parser;
use crate::lex::{Token, TokenKind, error};

impl Parser<'_> {
    pub(super) fn locator(&mut self) -> Option<InputLocator> {
        if self.peek().kind != TokenKind::Ident {
            self.expected("a locator: `path`, `git`, `archive`, or a `domain/name` subject");
            return None;
        }
        let token = self.peek().clone();
        let span = token.span;
        match self.lexeme(&token) {
            "path" => {
                self.take();
                let path = self.quoted("a quoted path after `path`")?;
                self.check_relative(&path, "an input path")?;
                Some(InputLocator::Path {
                    path: path.text.clone().unwrap_or_default(),
                    span: span.join(path.span),
                })
            }
            "git" => {
                self.take();
                let url = self.quoted("a quoted repository URL after `git`")?;
                let at = self.keyed_string("at", "a git locator pins its revision with `at`")?;
                let subpath = if self.peek().kind == TokenKind::Ident
                    && self.lexeme(self.peek()) == "subpath"
                {
                    self.take();
                    Some(self.quoted("a quoted subpath after `subpath`")?)
                } else {
                    None
                };
                let end = subpath.as_ref().map_or(at.span, |token| token.span);
                Some(InputLocator::Git {
                    url: url.text.clone().unwrap_or_default(),
                    revision: at.text.clone().unwrap_or_default(),
                    subpath: subpath.and_then(|token| token.text.clone()),
                    span: span.join(end),
                })
            }
            "archive" => {
                self.take();
                let url = self.quoted("a quoted archive URL after `archive`")?;
                let digest = self.keyed_string(
                    "digest",
                    "an archive locator carries the digest its bytes are admitted against",
                )?;
                Some(InputLocator::Archive {
                    url: url.text.clone().unwrap_or_default(),
                    digest: digest.text.clone().unwrap_or_default(),
                    span: span.join(digest.span),
                })
            }
            _ => self.registry_locator(),
        }
    }

    /// `subject [range] [from registry [name]]`: the registry locator, the
    /// only one that names its subject, because a registry serves by
    /// subject.
    pub(super) fn registry_locator(&mut self) -> Option<InputLocator> {
        let (subject, subject_span) = self.subject("the input's `domain/name` subject")?;
        let (range, range_span) = self.version_range()?;
        let mut registry = None;
        if self.peek().kind == TokenKind::Ident && self.lexeme(self.peek()) == "from" {
            self.take();
            self.expect_ident("registry", "`from registry`, the one route a subject names")?;
            if self.peek().kind == TokenKind::Ident && !is_reserved(self.lexeme(self.peek())) {
                let name = self.take();
                registry = Some(self.lexeme(&name).into());
            }
        }
        Some(InputLocator::Registry {
            subject,
            subject_span,
            range,
            range_span,
            registry,
            span: subject_span.join(self.previous_span()),
        })
    }

    /// The optional `with { ... }` and `grant { ... }` suffixes. Both are
    /// parsed to their extent and refused, naming the suffix.
    pub(super) fn input_suffix(&mut self) {
        for (word, what) in [("with", "arguments"), ("grant", "a grant")] {
            if self.peek().kind == TokenKind::Ident && self.lexeme(self.peek()) == word {
                let keyword = self.take();
                self.skip_to_entry_end();
                self.diagnostics.push(error(
                    FrontendCode::UnsupportedInput,
                    keyword.span,
                    format!("`{word}` passes {what} to an input, and {what} are not served"),
                    self.source,
                ));
            }
        }
    }
}

impl Parser<'_> {
    pub(super) fn registry_clause(&mut self, prior: &[ProjectRegistry]) -> Option<ProjectRegistry> {
        let keyword = self.take();
        let name = self.name("a registry name")?;
        if prior
            .iter()
            .any(|bound| bound.name.as_ref() == name.text.as_ref())
        {
            self.diagnostics.push(error(
                FrontendCode::DuplicateRegistry,
                name.span,
                format!("the inputs block binds the registry `{}` twice", name.text),
                self.source,
            ));
            return None;
        }
        self.expect(
            TokenKind::Eq,
            "`=` between the registry name and its locator",
        )?;
        let locator = self.quoted("a quoted registry locator")?;
        if !(self.peek().kind == TokenKind::Ident && self.lexeme(self.peek()) == "root") {
            self.diagnostics.push(error(
                FrontendCode::InvalidRootKey,
                keyword.span,
                "a registry binding pins the root key that vouches for its domain key sets; \
                 write `root \"algorithm:material\"`",
                self.source,
            ));
            return None;
        }
        self.take();
        let key = self.quoted("a quoted root key after `root`")?;
        let spelling = key.text.clone().unwrap_or_default();
        let root_key = match RootKey::parse(&spelling) {
            Ok(parsed) => parsed,
            Err(reason) => {
                self.diagnostics.push(error(
                    FrontendCode::InvalidRootKey,
                    key.span,
                    reason.to_string(),
                    self.source,
                ));
                return None;
            }
        };
        Some(ProjectRegistry {
            name: name.text,
            name_span: name.span,
            locator: locator.text.clone().unwrap_or_default(),
            locator_span: locator.span,
            root_key,
            root_key_span: key.span,
            span: keyword.span,
        })
    }

    /// `domain <domain> from <registry>`: the whole routing rule, written
    /// once per domain. Duplicates are a load-time refusal, because a user
    /// binding and a project binding for one domain is a collision resolved
    /// by precedence rather than by parse order.
    pub(super) fn domain_clause(&mut self) -> Option<ProjectDomain> {
        let keyword = self.take();
        if self.peek().kind != TokenKind::Ident {
            self.expected("a domain segment");
            return None;
        }
        let token = self.take();
        let domain = match SubjectSegment::parse(self.lexeme(&token)) {
            Ok(segment) => segment,
            Err(reason) => {
                self.diagnostics.push(error(
                    FrontendCode::InvalidSubject,
                    token.span,
                    reason.to_string(),
                    self.source,
                ));
                return None;
            }
        };
        self.expect_ident("from", "`from` naming the domain's registry")?;
        let registry = self.name("the registry answering for the domain")?;
        Some(ProjectDomain {
            domain,
            domain_span: token.span,
            registry: registry.text,
            registry_span: registry.span,
            span: keyword.span,
        })
    }

    /// One `domain/name` subject, validated where it is parsed so a clean
    /// parse cannot hold an invalid one.
    pub(super) fn subject(&mut self, expected: &str) -> Option<(ModuleSubject, Span)> {
        if self.peek().kind != TokenKind::Ident {
            self.expected(expected);
            return None;
        }
        let token = self.take();
        match ModuleSubject::parse(self.lexeme(&token)) {
            Ok(subject) => Some((subject, token.span)),
            Err(reason) => {
                self.diagnostics.push(error(
                    FrontendCode::InvalidSubject,
                    token.span,
                    reason.to_string(),
                    self.source,
                ));
                None
            }
        }
    }

    /// The optional range between a subject and its route. Absent means
    /// [`VersionRange::Any`], spanning the point where a range would start
    /// so a diagnostic about it still lands somewhere readable.
    pub(super) fn version_range(&mut self) -> Option<(VersionRange, Span)> {
        let start = self.peek().span;
        let range = match self.peek().kind {
            TokenKind::Int => VersionRange::Exactly(self.version()?),
            TokenKind::Ident if self.lexeme(self.peek()) == "any" => {
                self.take();
                VersionRange::Any
            }
            TokenKind::Gt | TokenKind::GtEq => {
                let lower = self.range_bound()?;
                if self.peek().kind != TokenKind::Comma {
                    VersionRange::AtLeast(lower)
                } else {
                    self.take();
                    let upper = self.range_bound()?;
                    if !matches!(
                        upper.version.cmp(&lower.version),
                        std::cmp::Ordering::Greater
                    ) {
                        self.diagnostics.push(error(
                            FrontendCode::InvalidRange,
                            start,
                            format!(
                                "the range is empty: no version is both at least {} and at \
                                 most {}",
                                lower.version, upper.version
                            ),
                            self.source,
                        ));
                        return None;
                    }
                    VersionRange::Between { lower, upper }
                }
            }
            TokenKind::Lt | TokenKind::LtEq => VersionRange::AtMost(self.range_bound()?),
            _ => VersionRange::Any,
        };
        Some((range, start.join(self.previous_span())))
    }

    /// One `>= 1.2` style bound. The comparison written decides the edge; a
    /// bound with no comparison is not a bound.
    pub(super) fn range_bound(&mut self) -> Option<VersionBound> {
        let token = self.take();
        let inclusive = match token.kind {
            TokenKind::GtEq | TokenKind::LtEq => true,
            TokenKind::Gt | TokenKind::Lt => false,
            _ => {
                self.diagnostics.push(error(
                    FrontendCode::InvalidRange,
                    token.span,
                    format!(
                        "a range bound opens with `>=`, `>`, `<=`, or `<`, not {}",
                        self.describe(&token)
                    ),
                    self.source,
                ));
                return None;
            }
        };
        Some(VersionBound {
            version: self.version()?,
            inclusive,
        })
    }

    /// A dotted-integer version: one or more `u64` segments, retained as
    /// metadata.
    pub(super) fn version(&mut self) -> Option<ModuleVersion> {
        let mut segments = vec![self.version_segment()?];
        while self.peek().kind == TokenKind::Dot {
            self.take();
            segments.push(self.version_segment()?);
        }
        ModuleVersion::from_segments(segments).ok()
    }

    pub(super) fn version_segment(&mut self) -> Option<u64> {
        if self.peek().kind != TokenKind::Int {
            let token = self.take();
            self.diagnostics.push(error(
                FrontendCode::InvalidVersion,
                token.span,
                format!(
                    "expected a version segment of decimal digits, found {}",
                    self.describe(&token)
                ),
                self.source,
            ));
            return None;
        }
        let token = self.take();
        let digits = self.lexeme(&token);
        match digits.parse::<u64>() {
            Ok(segment) => Some(segment),
            Err(_) => {
                self.diagnostics.push(error(
                    FrontendCode::InvalidVersion,
                    token.span,
                    format!("`{digits}` overflows a version segment"),
                    self.source,
                ));
                None
            }
        }
    }

    /// A `keyword "value"` pair a locator requires. Its absence is the
    /// incomplete-locator refusal, reported at the locator rather than at
    /// whatever token happened to follow.
    pub(super) fn keyed_string(&mut self, keyword: &str, requirement: &str) -> Option<Token> {
        if !(self.peek().kind == TokenKind::Ident && self.lexeme(self.peek()) == keyword) {
            let span = self.previous_span();
            self.diagnostics.push(error(
                FrontendCode::IncompleteSource,
                span,
                format!("{requirement}; write `{keyword} \"...\"`"),
                self.source,
            ));
            return None;
        }
        self.take();
        self.quoted(&format!("a quoted value after `{keyword}`"))
    }

    pub(super) fn quoted(&mut self, expected: &str) -> Option<Token> {
        if self.peek().kind != TokenKind::Str {
            self.expected(expected);
            return None;
        }
        Some(self.take())
    }

    /// A path that must name a file relative to the project: neither empty
    /// nor absolute.
    pub(super) fn check_relative(&mut self, token: &Token, what: &str) -> Option<()> {
        let path = token.text.as_deref().unwrap_or_default();
        if path.is_empty() || path.starts_with('/') {
            self.diagnostics.push(error(
                FrontendCode::InvalidPath,
                token.span,
                format!(
                    "{what} is relative to the declaring project and is neither empty nor \
                     absolute"
                ),
                self.source,
            ));
            return None;
        }
        Some(())
    }

    /// Consume tokens to the end of the current inputs-block entry: the
    /// comma, or the block's closing brace, at no bracket depth.
    pub(super) fn skip_to_entry_end(&mut self) {
        let mut depth = 0_usize;
        while self.peek().kind != TokenKind::End {
            match self.peek().kind {
                TokenKind::LParen | TokenKind::LBrace | TokenKind::LBracket => {
                    depth = depth.saturating_add(1);
                }
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace if depth > 0 => {
                    depth = depth.saturating_sub(1);
                }
                TokenKind::RBrace | TokenKind::Comma => return,
                _ => {}
            }
            self.take();
        }
    }
}

fn is_reserved(spelling: &str) -> bool {
    crate::lex::KEYWORDS.contains(&spelling) || crate::lex::HEADER_CLAUSES.contains(&spelling)
}
