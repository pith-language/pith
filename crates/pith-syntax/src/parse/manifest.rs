//! The manifest document's productions. The parser is the source grammar's,
//! lexed in the manifest document context: identifiers there may spell
//! subjects and hyphenated names as single tokens, and every clause but
//! `use` is a singleton the parse settles immediately.

use pith_diag::{ByteOffset, Span};
use pith_hir::{
    DependencySource, FrontendCode, ManifestDomain, ManifestMember, ManifestModule,
    ManifestRegistry, ManifestUse, ManifestVersion, ManifestWorkspace, ModuleSubject,
    ParsedManifest, RootKey, SubjectSegment, SurfaceComment, VersionBound, VersionRange,
};

use crate::lex::{SOURCE_ITEMS, TokenKind, error};
use crate::parse::Parser;

impl Parser<'_> {
    pub(crate) fn manifest_document(&mut self) -> ParsedManifest {
        let mut module = None;
        let mut workspace = None;
        let mut uses = Vec::new();
        let mut registries = Vec::new();
        let mut domains = Vec::new();
        let mut comments = Vec::new();
        while self.peek().kind != TokenKind::End {
            let start = self.position;
            if self.peek().kind == TokenKind::LineComment {
                let trailing = self.comment_is_trailing();
                let span = self.take().span;
                comments.push(SurfaceComment { span, trailing });
                continue;
            }
            let outcome = match (self.peek().kind, self.lexeme(self.peek())) {
                (TokenKind::Ident, "module") => {
                    self.take_singleton("module", &mut module, Self::module_clause);
                    Some(())
                }
                (TokenKind::Ident, "workspace") => {
                    self.take_singleton("workspace", &mut workspace, Self::workspace_clause);
                    Some(())
                }
                (TokenKind::Ident, "use") => self.use_clause(&uses).map(|clause| uses.push(clause)),
                (TokenKind::Ident, "registry") => self
                    .registry_clause(&registries)
                    .map(|clause| registries.push(clause)),
                (TokenKind::Ident, "domain") => {
                    self.domain_clause().map(|clause| domains.push(clause))
                }
                _ => {
                    let token = self.take();
                    let (code, message) = if token.kind == TokenKind::Ident
                        && SOURCE_ITEMS.contains(&self.lexeme(&token))
                    {
                        (
                            FrontendCode::WrongDocument,
                            format!(
                                "`{}` opens a source clause, and a manifest holds none; source \
                                 files live under `src/`",
                                self.lexeme(&token)
                            ),
                        )
                    } else {
                        (
                            FrontendCode::UnexpectedToken,
                            format!(
                                "expected `module`, `workspace`, `use`, `registry`, or \
                                 `domain`, found {}",
                                self.describe(&token)
                            ),
                        )
                    };
                    self.diagnostics
                        .push(error(code, token.span, message, self.source));
                    Some(())
                }
            };
            if outcome.is_none() {
                self.skip_to_manifest_item();
            }
            if self.position == start {
                self.take();
            }
        }
        if module.is_none() {
            self.diagnostics.push(error(
                FrontendCode::UnexpectedToken,
                Span::point(ByteOffset(self.previous_end())),
                "a manifest declares exactly one `module domain/name version` clause",
                self.source,
            ));
        }
        ParsedManifest {
            module,
            workspace,
            uses: uses.into(),
            registries: registries.into(),
            domains: domains.into(),
            comments: comments.into(),
        }
    }

    /// Parse a singleton clause into `slot`, keeping the first and
    /// diagnosing any later one at its opening keyword.
    fn take_singleton<T>(
        &mut self,
        name: &str,
        slot: &mut Option<T>,
        clause: fn(&mut Self) -> Option<T>,
    ) {
        let keyword = self.peek().span;
        if let Some(parsed) = clause(self) {
            if slot.is_some() {
                self.diagnostics.push(error(
                    FrontendCode::DuplicateManifestClause,
                    keyword,
                    format!("a manifest declares at most one `{name}` clause"),
                    self.source,
                ));
            } else {
                *slot = Some(parsed);
            }
        }
    }

    fn module_clause(&mut self) -> Option<ManifestModule> {
        let keyword = self.take();
        let (subject, subject_span) = self.subject("the module's `domain/name` subject")?;
        let version = self.version()?;
        Some(ManifestModule {
            subject,
            subject_span,
            version,
            span: keyword.span,
        })
    }

    fn workspace_clause(&mut self) -> Option<ManifestWorkspace> {
        let keyword = self.take();
        self.expect(TokenKind::LBrace, "`{` opening the workspace")?;
        self.expect_ident("members", "`members` naming the member list")?;
        self.expect(TokenKind::Colon, "`:` after `members`")?;
        let members = self.member_list()?;
        if self.peek().kind == TokenKind::Comma {
            self.take();
        }
        self.expect(TokenKind::RBrace, "`}` closing the workspace")?;
        Some(ManifestWorkspace {
            members: members.into(),
            span: keyword.span,
        })
    }

    fn member_list(&mut self) -> Option<Vec<ManifestMember>> {
        self.expect(TokenKind::LBracket, "`[` opening the member list")?;
        let mut members = Vec::new();
        while self.peek().kind == TokenKind::Str {
            let token = self.take();
            members.push(ManifestMember {
                path: token.text.clone().unwrap_or_default(),
                span: token.span,
            });
            if self.peek().kind != TokenKind::Comma {
                break;
            }
            self.take();
        }
        self.expect(TokenKind::RBracket, "`]` closing the member list")?;
        Some(members)
    }

    fn use_clause(&mut self, prior: &[ManifestUse]) -> Option<ManifestUse> {
        let keyword = self.take();
        let alias = self.name("a dependency alias")?;
        if prior
            .iter()
            .any(|bound| bound.alias.as_ref() == alias.text.as_ref())
        {
            self.diagnostics.push(error(
                FrontendCode::DuplicateBinding,
                alias.span,
                format!("the manifest binds the alias `{}` twice", alias.text),
                self.source,
            ));
            return None;
        }
        self.expect(TokenKind::Eq, "`=` between the alias and the subject")?;
        let (subject, subject_span) = self.subject("the dependency's `domain/name` subject")?;
        let (range, range_span) = self.version_range()?;
        let source = self.dependency_source()?;
        if !range.is_any() && !source.resolves_versions() {
            self.diagnostics.push(error(
                FrontendCode::RangeWithoutResolution,
                range_span,
                format!(
                    "a `{}` dependency names one module, so a range selects nothing; the \
                     acquired manifest's version is checked instead",
                    source.kind()
                ),
                self.source,
            ));
            return None;
        }
        Some(ManifestUse {
            alias: alias.text,
            alias_span: alias.span,
            subject,
            subject_span,
            range,
            range_span,
            source,
            span: keyword.span,
        })
    }

    /// The optional range between a subject and its route. Absent means
    /// [`VersionRange::Any`], spanning the point where a range would start
    /// so a diagnostic about it still lands somewhere readable.
    fn version_range(&mut self) -> Option<(VersionRange, Span)> {
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
    fn range_bound(&mut self) -> Option<VersionBound> {
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

    /// The `from <source>` half of a `use` clause. A clause with no `from`
    /// takes the domain's configured registry, which is the default route
    /// and the only one that selects among versions.
    fn dependency_source(&mut self) -> Option<DependencySource> {
        if !(self.peek().kind == TokenKind::Ident && self.lexeme(self.peek()) == "from") {
            return Some(DependencySource::Registry {
                registry: None,
                span: self.previous_span(),
            });
        }
        let from = self.take();
        if self.peek().kind != TokenKind::Ident {
            self.expected("a source kind after `from`");
            return None;
        }
        let kind = self.take();
        let span = from.span.join(kind.span);
        match self.lexeme(&kind) {
            "path" => self.path_source(span),
            "registry" => self.registry_source(span),
            "git" => self.git_source(span),
            "archive" => self.archive_source(span),
            other => {
                self.diagnostics.push(error(
                    FrontendCode::UnsupportedSource,
                    kind.span,
                    format!(
                        "`{other}` is not a source kind; a dependency comes `from path`, \
                         `from registry`, `from git`, or `from archive`"
                    ),
                    self.source,
                ));
                None
            }
        }
    }

    fn path_source(&mut self, span: Span) -> Option<DependencySource> {
        let token = self.quoted("a quoted path after `from path`")?;
        let path = token.text.clone().unwrap_or_default();
        if path.is_empty() || path.starts_with('/') {
            self.diagnostics.push(error(
                FrontendCode::InvalidPath,
                token.span,
                "a dependency path is relative to the declaring manifest and is neither empty \
                 nor absolute",
                self.source,
            ));
            return None;
        }
        Some(DependencySource::Path {
            path,
            span: span.join(token.span),
        })
    }

    /// `from registry <name>`: the domain's route, named explicitly. The
    /// name is optional, because writing `from registry` alone says only
    /// that the default route was meant.
    fn registry_source(&mut self, span: Span) -> Option<DependencySource> {
        if self.peek().kind != TokenKind::Ident {
            return Some(DependencySource::Registry {
                registry: None,
                span,
            });
        }
        let name = self.take();
        Some(DependencySource::Registry {
            registry: Some(self.lexeme(&name).into()),
            span: span.join(name.span),
        })
    }

    /// `from git "url" revision "..."`, optionally `subpath "..."`. A route
    /// with no revision names no immutable content, so it is refused here
    /// rather than parsed into something acquisition must re-check.
    fn git_source(&mut self, span: Span) -> Option<DependencySource> {
        let url = self.quoted("a quoted repository URL after `from git`")?;
        let revision = self.keyed_string("revision", &span, "a git dependency pins a revision")?;
        let subpath =
            if self.peek().kind == TokenKind::Ident && self.lexeme(self.peek()) == "subpath" {
                self.take();
                Some(self.quoted("a quoted subpath after `subpath`")?)
            } else {
                None
            };
        let end = subpath.as_ref().map_or(revision.span, |token| token.span);
        Some(DependencySource::Git {
            url: url.text.clone().unwrap_or_default(),
            revision: revision.text.clone().unwrap_or_default(),
            subpath: subpath.and_then(|token| token.text.clone()),
            span: span.join(end),
        })
    }

    /// `from archive "url" digest "..."`. The digest is what admits the
    /// bytes, so an archive route without one has nothing to check against.
    fn archive_source(&mut self, span: Span) -> Option<DependencySource> {
        let url = self.quoted("a quoted archive URL after `from archive`")?;
        let digest = self.keyed_string(
            "digest",
            &span,
            "an archive dependency carries the digest its bytes are admitted against",
        )?;
        Some(DependencySource::Archive {
            url: url.text.clone().unwrap_or_default(),
            digest: digest.text.clone().unwrap_or_default(),
            span: span.join(digest.span),
        })
    }

    /// A `keyword "value"` pair a source kind requires. Its absence is the
    /// incomplete-source refusal, reported at the route rather than at
    /// whatever token happened to follow.
    fn keyed_string(
        &mut self,
        keyword: &str,
        span: &Span,
        requirement: &str,
    ) -> Option<crate::lex::Token> {
        if !(self.peek().kind == TokenKind::Ident && self.lexeme(self.peek()) == keyword) {
            self.diagnostics.push(error(
                FrontendCode::IncompleteSource,
                *span,
                format!("{requirement}; write `{keyword} \"...\"`"),
                self.source,
            ));
            return None;
        }
        self.take();
        self.quoted(&format!("a quoted value after `{keyword}`"))
    }

    fn quoted(&mut self, expected: &str) -> Option<crate::lex::Token> {
        if self.peek().kind != TokenKind::Str {
            self.expected(expected);
            return None;
        }
        Some(self.take())
    }

    /// `registry name = "locator" root "algorithm:material"`.
    fn registry_clause(&mut self, prior: &[ManifestRegistry]) -> Option<ManifestRegistry> {
        let keyword = self.take();
        let name = self.name("a registry name")?;
        if prior
            .iter()
            .any(|bound| bound.name.as_ref() == name.text.as_ref())
        {
            self.diagnostics.push(error(
                FrontendCode::DuplicateRegistry,
                name.span,
                format!("the manifest binds the registry `{}` twice", name.text),
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
        Some(ManifestRegistry {
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
    fn domain_clause(&mut self) -> Option<ManifestDomain> {
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
        Some(ManifestDomain {
            domain,
            domain_span: token.span,
            registry: registry.text,
            registry_span: registry.span,
            span: keyword.span,
        })
    }

    /// One `domain/name` subject, validated where it is parsed so a clean
    /// parse cannot hold an invalid one.
    fn subject(&mut self, expected: &str) -> Option<(ModuleSubject, Span)> {
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

    /// A dotted-integer version: one or more `u64` segments, retained as
    /// metadata.
    fn version(&mut self) -> Option<ManifestVersion> {
        let mut segments = vec![self.version_segment()?];
        while self.peek().kind == TokenKind::Dot {
            self.take();
            segments.push(self.version_segment()?);
        }
        ManifestVersion::from_segments(segments).ok()
    }

    fn version_segment(&mut self) -> Option<u64> {
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

    fn skip_to_manifest_item(&mut self) {
        while self.peek().kind != TokenKind::End {
            if self.peek().kind == TokenKind::LineComment || self.is_manifest_item_start() {
                return;
            }
            self.take();
        }
    }

    fn is_manifest_item_start(&self) -> bool {
        self.peek().kind == TokenKind::Ident
            && crate::lex::MANIFEST_CLAUSES.contains(&self.lexeme(self.peek()))
    }
}
