//! Text encoding for lock documents.
//!
//! Rendering is deterministic and parsing accepts the rendered form plus
//! equivalent non-canonical spellings. Every parse refusal carries a span
//! into the parsed text and the source file itself, so a reader renders the
//! line and the field rather than a number baked into prose. Character-level
//! token handling lives in `text`; filesystem publication lives in
//! `publish`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

use pith_diag::{
    ByteOffset, Diag, DiagnosticSink, FileLine, PithResult, Severity, SourceFile, SourceId, Span,
    StableCode,
};
use pith_ids::ContentId;

use super::document::Lock;
use super::text::{
    BLAKE3, Token, features_token, parse_digest, parse_features, token as text_token, tokenize,
};
use crate::identity::{DomainIdentity, PackageIdentity, PackageVersion};
use crate::lock::{Binding, LockEntry, Origin};
use crate::preference::{Preference, PreferenceList};
use crate::text_diag;

/// The lock file's format version, pinned at 1 on the same terms the state
/// store pins its own: nothing is released, and a format change breaks the
/// file rather than migrating it.
pub const LOCK_FILE_VERSION: u32 = 1;

const LOCK_VERSION: &str = "lock-version";
const RESOLVER: &str = "resolver";
const VERSION_SCHEME: &str = "version-scheme";
const UNIVERSE: &str = "universe";
const PREFERENCE: &str = "preference";
const BIND: &str = "bind";

/// Renders a lock document as canonical text with byte-sorted binding lines.
#[must_use]
pub fn render(lock: &Lock) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{LOCK_VERSION} {LOCK_FILE_VERSION}");
    let _ = writeln!(out, "{RESOLVER} {BLAKE3}{}", lock.resolver);
    let _ = writeln!(out, "{VERSION_SCHEME} {}", text_token(&lock.scheme));
    let _ = writeln!(out, "{UNIVERSE} {BLAKE3}{}", lock.universe.digest());
    for preference in lock.preferences.0.iter() {
        let _ = writeln!(out, "{PREFERENCE} {}", preference.name());
    }
    out.push('\n');
    let mut lines: Vec<String> = lock.entries.iter().map(bind_line).collect();
    lines.sort();
    for line in lines {
        let _ = writeln!(out, "{line}");
    }
    out
}

/// Parses lock text and normalizes entries into canonical value order. The
/// label names the text in diagnostics; a reader that parsed a file passes
/// its path.
///
/// # Errors
/// Returns a diagnostic attached to the parsed text, its span selecting the
/// field or line that was refused.
pub fn parse(label: &str, text: &str) -> PithResult<Lock> {
    let file = Arc::new(SourceFile::new(SourceId::from_raw(0), label, text));
    parse_file(&file)
}

fn parse_file(file: &Arc<SourceFile>) -> PithResult<Lock> {
    let mut header = Header::default();
    let mut seen: BTreeMap<PackageIdentity, (Span, LockEntry)> = BTreeMap::new();
    let mut version_seen = false;
    for line in file.lines() {
        let tokens = tokenize(line.text, line.span.start)
            .map_err(|refusal| text_diag(file, refusal.span, refusal.message))?;
        let Some(first) = tokens.first() else {
            continue;
        };
        let directive = first.text.as_str();
        if !version_seen {
            if directive == LOCK_VERSION {
                version_seen = true;
                header.lock_version(file, &tokens, line)?;
                continue;
            }
            return Err(text_diag(
                file,
                line.span,
                format!(
                    "expected `{LOCK_VERSION} {LOCK_FILE_VERSION}` as the first line, found \
                     `{}`",
                    line.text.trim()
                ),
            ));
        }
        match directive {
            RESOLVER => header.resolver(file, &tokens, line)?,
            VERSION_SCHEME => header.scheme(file, &tokens, line)?,
            UNIVERSE => header.universe(file, &tokens, line)?,
            PREFERENCE => header.preference(file, &tokens, line)?,
            BIND => record_binding(file, &mut seen, bind_entry(file, &tokens, line)?, line)?,
            other => {
                return Err(text_diag(
                    file,
                    first.span,
                    format!(
                        "`{other}` is not a lock directive; expected one of {RESOLVER}, \
                         {VERSION_SCHEME}, {UNIVERSE}, {PREFERENCE}, {BIND}"
                    ),
                ));
            }
        }
    }
    if !version_seen {
        return Err(text_diag(
            file,
            Span::point(ByteOffset(0)),
            format!("the lock carried no `{LOCK_VERSION} {LOCK_FILE_VERSION}` first line"),
        ));
    }
    let (resolver, scheme, universe, preferences) = header.finish(file)?;
    let entries: Vec<LockEntry> = seen.into_values().map(|(_, entry)| entry).collect();
    Lock::new(resolver, scheme, universe, preferences, entries)
}

/// Adds a parsed binding, collapsing identical duplicates.
///
/// # Errors
/// Returns a diagnostic when the package is already bound differently: the
/// span selects this line, and a note selects the earlier one.
fn record_binding(
    file: &Arc<SourceFile>,
    seen: &mut BTreeMap<PackageIdentity, (Span, LockEntry)>,
    entry: LockEntry,
    line: FileLine,
) -> PithResult<()> {
    let identity = entry.package.identity().clone();
    match seen.entry(identity.clone()) {
        std::collections::btree_map::Entry::Vacant(slot) => {
            slot.insert((line.span, entry));
        }
        std::collections::btree_map::Entry::Occupied(slot) => {
            let (previous_span, previous) = slot.get();
            if previous != &entry {
                let mut sink = DiagnosticSink::new();
                sink.push(
                    Diag::new(
                        Severity::Error,
                        StableCode(crate::PHLOEM_CODE),
                        line.span,
                        format!(
                            "the lock binds `{}` in `{}` twice: the first line bound version \
                             {}, and this line binds version {}: two selections of one package \
                             is the conflict a union merge cannot represent",
                            identity.name(),
                            identity.domain().as_str(),
                            previous.package.version(),
                            entry.package.version(),
                        ),
                    )
                    .with_source(Arc::clone(file))
                    .with_note(
                        *previous_span,
                        format!("the first binding, version {}", previous.package.version()),
                    ),
                );
                return Err(sink);
            }
        }
    }
    Ok(())
}

#[derive(Default)]
struct Header {
    resolver: Option<Box<str>>,
    scheme: Option<Box<str>>,
    universe: Option<ContentId>,
    preferences: Vec<Preference>,
}

impl Header {
    fn lock_version(
        &mut self,
        file: &Arc<SourceFile>,
        tokens: &[Token],
        line: FileLine,
    ) -> PithResult<()> {
        let found = singleton(file, tokens, LOCK_VERSION, line)?;
        let version = found.text.parse::<u32>().map_err(|_| {
            text_diag(
                file,
                found.span,
                format!("`{}` is not a lock version", found.text),
            )
        })?;
        if version != LOCK_FILE_VERSION {
            return Err(text_diag(
                file,
                found.span,
                format!(
                    "the lock names format version {version}, and this reader understands only \
                     {LOCK_FILE_VERSION}; the format was changed after this reader was built"
                ),
            ));
        }
        Ok(())
    }

    fn resolver(
        &mut self,
        file: &Arc<SourceFile>,
        tokens: &[Token],
        line: FileLine,
    ) -> PithResult<()> {
        let found = singleton(file, tokens, RESOLVER, line)?;
        self.if_absent(file, line, RESOLVER)?;
        self.resolver = Some(
            parse_digest(found, RESOLVER)
                .map_err(|refusal| text_diag(file, refusal.span, refusal.message))?
                .digest()
                .to_string()
                .into(),
        );
        Ok(())
    }

    fn scheme(
        &mut self,
        file: &Arc<SourceFile>,
        tokens: &[Token],
        line: FileLine,
    ) -> PithResult<()> {
        let found = singleton(file, tokens, VERSION_SCHEME, line)?;
        self.if_absent(file, line, VERSION_SCHEME)?;
        self.scheme = Some(found.text.as_str().into());
        Ok(())
    }

    fn universe(
        &mut self,
        file: &Arc<SourceFile>,
        tokens: &[Token],
        line: FileLine,
    ) -> PithResult<()> {
        let found = singleton(file, tokens, UNIVERSE, line)?;
        self.if_absent(file, line, UNIVERSE)?;
        self.universe = Some(
            parse_digest(found, UNIVERSE)
                .map_err(|refusal| text_diag(file, refusal.span, refusal.message))?,
        );
        Ok(())
    }

    fn preference(
        &mut self,
        file: &Arc<SourceFile>,
        tokens: &[Token],
        line: FileLine,
    ) -> PithResult<()> {
        let found = singleton(file, tokens, PREFERENCE, line)?;
        let Some(preference) = Preference::from_name(found.text.as_str()) else {
            return Err(text_diag(
                file,
                found.span,
                format!(
                    "`{}` is not a declared preference; expected newest or oldest",
                    found.text
                ),
            ));
        };
        self.preferences.push(preference);
        Ok(())
    }

    fn if_absent(&self, file: &Arc<SourceFile>, line: FileLine, directive: &str) -> PithResult<()> {
        let present = match directive {
            RESOLVER => self.resolver.is_some(),
            VERSION_SCHEME => self.scheme.is_some(),
            UNIVERSE => self.universe.is_some(),
            _ => false,
        };
        if present {
            return Err(text_diag(
                file,
                line.span,
                format!("the `{directive}` directive appears twice; a lock carries it once"),
            ));
        }
        Ok(())
    }

    fn finish(
        self,
        file: &Arc<SourceFile>,
    ) -> PithResult<(Box<str>, Box<str>, ContentId, PreferenceList)> {
        let resolver = self.resolver.ok_or_else(|| missing(file, RESOLVER))?;
        let scheme = self.scheme.ok_or_else(|| missing(file, VERSION_SCHEME))?;
        let universe = self.universe.ok_or_else(|| missing(file, UNIVERSE))?;
        Ok((
            resolver,
            scheme,
            universe,
            PreferenceList(self.preferences.into()),
        ))
    }
}

fn missing(file: &Arc<SourceFile>, directive: &'static str) -> DiagnosticSink {
    text_diag(
        file,
        Span::point(ByteOffset(0)),
        format!("the lock carried no `{directive}` directive"),
    )
}

fn singleton<'a>(
    file: &Arc<SourceFile>,
    tokens: &'a [Token],
    directive: &str,
    line: FileLine,
) -> PithResult<&'a Token> {
    match tokens {
        [_, value] => Ok(value),
        _ => Err(text_diag(
            file,
            line.span,
            format!(
                "the `{directive}` directive takes one value; found {}",
                tokens.len().saturating_sub(1)
            ),
        )),
    }
}

/// Renders a binding line shared by lock files and transparency-log leaves.
#[must_use]
pub fn binding_line(entry: &LockEntry) -> String {
    format!(
        "{BIND} {} {} {} {} {BLAKE3}{}",
        text_token(entry.package.identity().domain().as_str()),
        text_token(entry.package.identity().name()),
        text_token(entry.package.version()),
        features_token(&entry.features),
        entry.source.digest(),
    )
}

/// Parse the binding shape shared by a lock line and a log leaf, attaching
/// the leaf's file to every refusal.
pub(crate) fn parse_binding_line(source: &Arc<SourceFile>, line: &FileLine) -> PithResult<Binding> {
    let tokens = tokenize(line.text, line.span.start)
        .map_err(|refusal| text_diag(source, refusal.span, refusal.message))?;
    parse_binding_tokens(source, &tokens, line.span)
}

fn parse_binding_tokens(
    source: &Arc<SourceFile>,
    tokens: &[Token],
    line_span: Span,
) -> PithResult<Binding> {
    let [directive, domain, name, version, features, digest] = tokens else {
        return Err(text_diag(
            source,
            line_span,
            format!(
                "a `{BIND}` binding carries domain, name, version, features, and source; found \
                 {} tokens",
                tokens.len().saturating_sub(1)
            ),
        ));
    };
    if directive.text != BIND {
        return Err(text_diag(
            source,
            directive.span,
            format!("a binding starts with `{BIND}`, found `{}`", directive.text),
        ));
    }
    let mut features = parse_features(features)
        .map_err(|refusal| text_diag(source, refusal.span, refusal.message))?;
    features.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    Ok(Binding {
        package: PackageVersion::new(
            PackageIdentity::declare(
                DomainIdentity::new(domain.text.as_str()),
                name.text.as_str(),
            ),
            version.text.as_str(),
        ),
        features,
        source: parse_digest(digest, "bind source")
            .map_err(|refusal| text_diag(source, refusal.span, refusal.message))?,
    })
}

/// One entry's whole written line: the binding, then where it was
/// resolved from. The origin rides outside the binding because the log
/// witnesses what the coordinates resolve to, not where any one client
/// read it.
fn bind_line(entry: &LockEntry) -> String {
    format!(
        "{} {} {}",
        binding_line(entry),
        entry.origin.kind(),
        text_token(entry.origin.location()),
    )
}

fn bind_entry(source: &Arc<SourceFile>, tokens: &[Token], line: FileLine) -> PithResult<LockEntry> {
    let [_, _, _, _, _, _, kind, location] = tokens else {
        return Err(text_diag(
            source,
            line.span,
            format!(
                "a `{BIND}` line carries domain, name, version, features, source, origin kind, \
                 and origin location; found {} tokens",
                tokens.len().saturating_sub(1)
            ),
        ));
    };
    let binding_tokens = match tokens.get(..6) {
        Some(binding_tokens) => binding_tokens,
        None => unreachable!("the binding prefix exists in the eight-token pattern"),
    };
    let binding = parse_binding_tokens(source, binding_tokens, line.span)?;
    let Some(origin) = Origin::from_kind(kind.text.as_str(), location.text.clone()) else {
        return Err(text_diag(
            source,
            kind.span,
            format!(
                "`{}` is not an origin kind; expected registry, forge, or local-path",
                kind.text
            ),
        ));
    };
    Ok(LockEntry::new(
        binding.package,
        binding.features,
        binding.source,
        origin,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::diff;

    fn select(source: &str, span: Span) -> Option<&str> {
        source.get(span.start.0 as usize..span.end.0 as usize)
    }

    fn lock() -> Lock {
        let universe = ContentId::of_blob(b"universe");
        Lock::new(
            crate::resolve::resolver_revision_hex(),
            Box::from("numeric-segments"),
            universe,
            PreferenceList(Box::new([Preference::Newest])),
            vec![
                LockEntry::new(
                    PackageVersion::new(
                        PackageIdentity::declare(DomainIdentity::new("pithpkgs"), "zlib"),
                        "1.3",
                    ),
                    [] as [&str; 0],
                    ContentId::of_blob(b"zlib-1.3.tar"),
                    Origin::Registry("pkgs.pith-lang.org".into()),
                ),
                LockEntry::new(
                    PackageVersion::new(
                        PackageIdentity::declare(DomainIdentity::new("pithpkgs"), "openssl"),
                        "1.1.1",
                    ),
                    ["shared", "zlib"],
                    ContentId::of_blob(b"openssl-1.1.1.tar"),
                    Origin::Forge("git.pith-lang.org/openssl".into()),
                ),
            ],
        )
        .unwrap()
    }

    #[test]
    fn entries_are_ordered_by_the_line_bytes_not_the_canonical_encoding() {
        let written = lock();
        assert_eq!(
            written.entries.first().unwrap().package.identity().name(),
            "zlib",
            "the value's canonical order puts the shorter name first"
        );
        let text = render(&written);
        let binds: Vec<&str> = text.lines().filter(|line| line.starts_with(BIND)).collect();
        let Some((first, second)) = binds.first().zip(binds.get(1)) else {
            unreachable!("the fixture holds two entries");
        };
        assert!(
            first.contains("openssl") && second.contains("zlib"),
            "the file's line order puts openssl before zlib: {binds:?}"
        );
    }

    #[test]
    fn a_preferences_written_name_reads_back_through_from_name() {
        for preference in [Preference::Newest, Preference::Oldest] {
            assert_eq!(Preference::from_name(preference.name()), Some(preference));
        }
        assert_eq!(Preference::from_name("heaviest"), None);
    }

    #[test]
    fn the_written_form_round_trips() {
        let written = lock();
        let text = render(&written);
        assert!(text.starts_with("lock-version 1\n"));
        assert_eq!(parse("pith.lock", &text).unwrap(), written);
        assert_eq!(
            render(&parse("pith.lock", &text).unwrap()),
            text,
            "render is deterministic"
        );
    }

    #[test]
    fn a_quoted_field_round_trips() {
        let mut quoted = lock();
        quoted.entries = Box::new([LockEntry::new(
            PackageVersion::new(
                PackageIdentity::declare(DomainIdentity::new("odd domain"), "name with spaces"),
                "1.0",
            ),
            ["feature, comma", "quote\"inside"],
            ContentId::of_blob(b"odd"),
            Origin::LocalPath("a path/with spaces".into()),
        )]);
        let text = render(&quoted);
        assert!(
            text.contains('"'),
            "the reserved characters force quoting: {text}"
        );
        assert_eq!(parse("pith.lock", &text).unwrap(), quoted);
    }

    #[test]
    fn a_hand_edited_file_normalizes_to_the_canonical_bytes() {
        let canonical = render(&lock());
        let directive = |prefix: &str| {
            canonical
                .lines()
                .find(|line| line.starts_with(prefix))
                .unwrap()
                .to_string()
        };
        let binds: Vec<&str> = canonical
            .lines()
            .filter(|l| l.starts_with("bind"))
            .collect();
        let (second_bind, first_bind) = (
            binds.last().copied().unwrap(),
            binds.first().copied().unwrap(),
        );
        let upper_universe = {
            let universe = directive("universe");
            let (prefix, digest) = universe.split_once(':').unwrap();
            format!("{prefix}:{}", digest.to_uppercase())
        };
        let hand = format!(
            "{}\n# merged by hand\n\n{}\n{}\n{}\n{}\n\n{}\n{}\n",
            directive("lock-version"),
            directive("preference"),
            directive("resolver"),
            upper_universe,
            directive("version-scheme"),
            second_bind,
            first_bind,
        );
        assert_ne!(hand, canonical);
        let parsed = parse("pith.lock", &hand).unwrap();
        assert_eq!(render(&parsed), canonical);
        assert_eq!(parsed, lock());
    }

    #[test]
    fn a_union_merge_binding_one_package_twice_is_refused_naming_both_lines() {
        let canonical = render(&lock());
        let moved = canonical
            .lines()
            .find(|line| line.contains("openssl"))
            .unwrap()
            .replacen("1.1.1", "1.1.2", 1);
        let merged = format!("{canonical}{moved}\n");
        let error = parse("pith.lock", &merged).unwrap_err();
        let diagnostic = error.iter().next().unwrap();
        let message = diagnostic.message.0.to_string();
        assert!(
            message.contains("twice") && message.contains("1.1.1") && message.contains("1.1.2"),
            "the diagnostic names the conflict and both versions: {message}"
        );
        assert_eq!(
            select(&merged, diagnostic.span),
            Some(moved.trim_end()),
            "the span selects the second binding line"
        );
        let note = diagnostic.notes.first().unwrap();
        assert_eq!(
            select(&merged, note.span),
            Some(
                canonical
                    .lines()
                    .find(|line| line.contains("openssl"))
                    .unwrap()
            ),
            "the note's span selects the first binding line"
        );
    }

    #[test]
    fn a_union_merge_moving_a_feature_set_is_refused_the_same_way() {
        let canonical = render(&lock());
        let moved = canonical
            .lines()
            .find(|line| line.contains("openssl"))
            .unwrap()
            .replacen("[shared,zlib]", "[static]", 1);
        let merged = format!("{canonical}{moved}\n");
        let error = parse("pith.lock", &merged).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.0.contains("twice")),
            "the diagnostic names the conflict: {error:?}"
        );
    }

    #[test]
    fn a_byte_identical_line_a_union_merge_repeated_collapses() {
        let canonical = render(&lock());
        let repeated = canonical
            .lines()
            .find(|line| line.starts_with(BIND))
            .unwrap()
            .to_string();
        let merged = format!("{canonical}{repeated}\n");
        assert_eq!(parse("pith.lock", &merged).unwrap(), lock());
    }

    #[test]
    fn a_wrong_format_version_is_refused_naming_the_found_version() {
        let text = render(&lock()).replacen("lock-version 1", "lock-version 7", 1);
        let error = parse("pith.lock", &text).unwrap_err();
        let diagnostic = error.iter().next().unwrap();
        assert!(
            diagnostic.message.0.contains("7") && diagnostic.message.0.contains("understands"),
            "the diagnostic names the found version: {error:?}"
        );
        assert_eq!(
            select(&text, diagnostic.span),
            Some("7"),
            "the span selects the version field"
        );
    }

    #[test]
    fn a_first_line_that_is_not_the_version_line_is_refused() {
        let text: String = render(&lock())
            .lines()
            .filter(|line| !line.starts_with("lock-version"))
            .collect::<Vec<_>>()
            .join("\n");
        let error = parse("pith.lock", &text).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("expected `lock-version")),
            "the diagnostic names what the first line had to be: {error:?}"
        );
        let empty = parse("pith.lock", "").unwrap_err();
        assert!(
            empty
                .iter()
                .any(|d| d.message.0.contains("carried no `lock-version")),
            "an empty file names the missing first line: {empty:?}"
        );
    }

    #[test]
    fn an_unknown_directive_is_refused_naming_it() {
        let text = format!("lock-version 1\nfog blake3:00\n{}", render(&lock()));
        let error = parse("pith.lock", &text).unwrap_err();
        assert!(
            error.iter().any(
                |d| d.message.0.contains("fog") && d.message.0.contains("not a lock directive")
            ),
            "the diagnostic names the directive: {error:?}"
        );
        assert!(
            error.iter().any(|d| select(&text, d.span) == Some("fog")),
            "the span selects the directive's own field: {error:?}"
        );
    }

    #[test]
    fn a_missing_directive_is_refused_naming_it() {
        let text: String = render(&lock())
            .lines()
            .filter(|line| !line.starts_with("universe "))
            .collect::<Vec<_>>()
            .join("\n");
        let error = parse("pith.lock", &text).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.0.contains("no `universe`")),
            "the diagnostic names the missing directive: {error:?}"
        );
    }

    #[test]
    fn a_bad_digest_is_refused_with_the_field_and_its_span() {
        let text = render(&lock()).replace(
            &format!("universe blake3:{}", lock().universe.digest()),
            "universe blake3:not-hex",
        );
        let error = parse("pith.lock", &text).unwrap_err();
        let diagnostic = error.iter().next().unwrap();
        assert!(
            diagnostic.message.0.contains("not hexadecimal")
                && diagnostic.message.0.contains("universe"),
            "the diagnostic names the field and the complaint: {error:?}"
        );
        assert_eq!(
            select(&text, diagnostic.span),
            Some("blake3:not-hex"),
            "the span selects the digest field, prefix included"
        );
    }

    #[test]
    fn a_bind_line_with_the_wrong_token_count_is_refused() {
        let truncated = format!(
            "lock-version 1\nresolver blake3:{}\nversion-scheme numeric-segments\nuniverse \
             blake3:{}\npreference newest\n\nbind pithpkgs zlib 1.3",
            lock().resolver,
            lock().universe.digest(),
        );
        let error = parse("pith.lock", &truncated).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("bind") && d.message.0.contains("tokens")),
            "the diagnostic names the line and the count: {error:?}"
        );
    }

    #[test]
    fn an_unknown_origin_kind_is_refused_at_its_own_field() {
        let text = render(&lock()).replace(" registry pkgs", " mirror pkgs");
        let error = parse("pith.lock", &text).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("mirror") && d.message.0.contains("origin kind")),
            "the diagnostic names the kind and the expected ones: {error:?}"
        );
        assert!(
            error
                .iter()
                .any(|d| select(&text, d.span) == Some("mirror")),
            "the span selects the origin-kind field: {error:?}"
        );
    }

    #[test]
    fn an_unterminated_quote_is_refused_spanning_the_rest_of_the_line() {
        let text = format!("{}\"never closed", render(&lock()));
        let error = parse("pith.lock", &text).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| select(&text, d.span) == Some("\"never closed")),
            "the refusal spans from the opening quote to the end of the line: {error:?}"
        );

        let text = render(&lock()).replace(
            "version-scheme numeric-segments",
            r#"version-scheme "\u{41"#,
        );
        let error = parse("pith.lock", &text).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.0.contains("never closed")),
            "a unicode escape requires its closing brace: {error:?}"
        );
    }

    #[test]
    fn the_written_form_spells_its_digest_algorithm() {
        let text = render(&lock());
        assert!(
            text.contains("blake3:"),
            "every digest field names the algorithm that hashed it: {text}"
        );
        assert!(
            !text.contains("sha256"),
            "no field claims an algorithm the kernel does not hash with: {text}"
        );
    }

    #[test]
    fn a_digest_spelling_another_algorithm_is_refused_naming_the_expected_one() {
        // A lock written by an earlier tree of this workspace spelled the
        // prefix `sha256:` over the same blake3 bytes. the bytes hash one
        // way whatever the line claims, so the claim is what moves: the read
        // refuses it and names the expected spelling, and under 0048 the
        // pre-release answer is to re-render, with no format version change.
        let universe = format!("blake3:{}", lock().universe.digest());
        let spelled = format!("sha256:{}", lock().universe.digest());
        let text = render(&lock()).replace(&universe, &spelled);
        let error = parse("pith.lock", &text).unwrap_err();
        let diagnostic = error.iter().next().unwrap();
        assert!(
            diagnostic.message.0.contains("blake3:"),
            "the refusal names the expected spelling: {error:?}"
        );
        assert_eq!(
            select(&text, diagnostic.span),
            Some(spelled.as_str()),
            "the span selects the mislabeled field"
        );
    }

    #[test]
    fn the_file_moves_when_an_input_moves_and_not_otherwise() {
        let base = lock();
        let moved = Lock::new(
            base.resolver.clone(),
            base.scheme.clone(),
            ContentId::of_blob(b"moved-universe"),
            base.preferences.clone(),
            base.entries.to_vec(),
        )
        .unwrap();
        assert_ne!(render(&base), render(&moved));
        let unchanged = Lock::new(
            base.resolver.clone(),
            base.scheme.clone(),
            base.universe,
            base.preferences.clone(),
            base.entries.iter().rev().cloned().collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(render(&base), render(&unchanged));
        assert_eq!(diff(&base, &moved).changes.len(), 1);
    }
}
