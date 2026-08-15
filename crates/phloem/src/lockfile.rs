//! The lock's written form: a text projection of the lock document
//! (decision 0041).
//!
//! Render is a total deterministic function of the document, parse inverts
//! render on rendered output, and render canonicalizes whatever parse
//! accepts, so a hand-edited file re-renders to the canonical bytes. The
//! file's own bytes are never canonical and no digest is taken over them:
//! canonical bytes are the value codec's job, and the text's job is to be
//! read in a diff. One binding per line, entries sorted by the line's own
//! bytes, header directives that conflict atomically when the universe, the
//! preferences, the scheme, or the resolver moved — go.sum's line shape,
//! with the pinning go.sum refuses.
//!
//! The character-level grammar lives in `locktext`; publishing the file to
//! the filesystem lives in `lockpublish`, on 0003's side of the effect
//! boundary. This module is the line codec between them.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use pith_diag::PithResult;
use pith_ids::ContentId;

use crate::diag;
use crate::document::Lock;
use crate::identity::{DomainIdentity, PackageIdentity, PackageVersion};
use crate::lock::{LockEntry, Origin};
use crate::locktext::{
    SHA256, features_token, parse_digest, parse_features, token as text_token, tokenize,
};
use crate::preference::{Preference, PreferenceList};

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

/// The document as its canonical text: header directives, a blank line,
/// then one binding line per entry sorted by the line's own bytes, LF line
/// endings, and a trailing newline.
///
/// The file's order is not the value's. The document holds its entries in
/// the canonical order of their value encodings, which length-prefixes
/// every string and would order names by length before their first
/// character; the file orders by the line's own bytes so adjacent packages
/// stay adjacent in a diff (0041). The value's order is the digest's
/// business, the file's is the diff's, and parse normalizes the file back
/// into the value's order.
#[must_use]
pub fn render(lock: &Lock) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{LOCK_VERSION} {LOCK_FILE_VERSION}");
    let _ = writeln!(out, "{RESOLVER} {SHA256}{}", lock.resolver);
    let _ = writeln!(out, "{VERSION_SCHEME} {}", text_token(&lock.scheme));
    let _ = writeln!(out, "{UNIVERSE} {SHA256}{}", lock.universe.digest());
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

/// Parse the written form back into a document. Blank lines and comments
/// are skipped, header directives are accepted in any order, and entries
/// are normalized into the canonical order, so everything parse accepts is
/// re-rendered to the canonical bytes.
///
/// The entries are a set over package identities: a byte-identical line a
/// union merge repeated collapses, and a second, different binding for a
/// package already bound is refused, because that is the one conflict a
/// union merge cannot represent and a person has to resolve.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming the line, what was expected, and
/// what was found, for every form this format does not accept.
pub fn parse(text: &str) -> PithResult<Lock> {
    let mut header = Header::default();
    let mut seen: BTreeMap<PackageIdentity, (usize, LockEntry)> = BTreeMap::new();
    let mut version_seen = false;
    for (number, raw) in text
        .lines()
        .enumerate()
        .map(|(index, line)| (index.saturating_add(1), line))
    {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let tokens = tokenize(line).map_err(|message| diag(format!("line {number}: {message}")))?;
        let Some(first) = tokens.first() else {
            continue;
        };
        let directive = first.as_str();
        if !version_seen {
            if tokens.first().map(String::as_str) == Some(LOCK_VERSION) {
                version_seen = true;
                header.lock_version(&tokens, number)?;
                continue;
            }
            return Err(diag(format!(
                "line {number}: expected `{LOCK_VERSION} {LOCK_FILE_VERSION}` as the first \
                 line, found `{line}`"
            )));
        }
        match directive {
            RESOLVER => header.resolver(&tokens, number)?,
            VERSION_SCHEME => header.scheme(&tokens, number)?,
            UNIVERSE => header.universe(&tokens, number)?,
            PREFERENCE => header.preference(&tokens, number)?,
            BIND => record_binding(&mut seen, bind_entry(&tokens, number)?, number)?,
            other => {
                return Err(diag(format!(
                    "line {number}: `{other}` is not a lock directive; expected one of \
                     {RESOLVER}, {VERSION_SCHEME}, {UNIVERSE}, {PREFERENCE}, {BIND}"
                )));
            }
        }
    }
    if !version_seen {
        return Err(diag(format!(
            "the lock carried no `{LOCK_VERSION} {LOCK_FILE_VERSION}` first line"
        )));
    }
    let (resolver, scheme, universe, preferences) = header.finish()?;
    let entries: Vec<LockEntry> = seen.into_values().map(|(_, entry)| entry).collect();
    Lock::new(resolver, scheme, universe, preferences, entries)
}

/// Fold one parsed binding into the entries-so-far. A binding whose package
/// is not yet bound is kept; a byte-identical repeat collapses; anything
/// else over an already-bound package is the union-merge conflict.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming the package, both line numbers,
/// and both versions when the package is already bound differently.
fn record_binding(
    seen: &mut BTreeMap<PackageIdentity, (usize, LockEntry)>,
    entry: LockEntry,
    number: usize,
) -> PithResult<()> {
    let identity = entry.package.identity().clone();
    match seen.entry(identity.clone()) {
        std::collections::btree_map::Entry::Vacant(slot) => {
            slot.insert((number, entry));
        }
        std::collections::btree_map::Entry::Occupied(slot) => {
            let (previous_line, previous) = slot.get();
            if previous != &entry {
                return Err(diag(format!(
                    "line {number}: the lock binds `{}` in `{}` twice; line {previous_line} \
                     bound version {}, and this line binds version {}: two selections of \
                     one package is the conflict a union merge cannot represent",
                    identity.name(),
                    identity.domain().as_str(),
                    previous.package.version(),
                    entry.package.version(),
                )));
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
    fn lock_version(&mut self, tokens: &[String], number: usize) -> PithResult<()> {
        let found = singleton(tokens, LOCK_VERSION, number)?;
        let version = found
            .parse::<u32>()
            .map_err(|_| diag(format!("line {number}: `{found}` is not a lock version")))?;
        if version != LOCK_FILE_VERSION {
            return Err(diag(format!(
                "line {number}: the lock names format version {version}, and this reader \
                 understands only {LOCK_FILE_VERSION}; the format was changed after this \
                 reader was built"
            )));
        }
        Ok(())
    }

    fn resolver(&mut self, tokens: &[String], number: usize) -> PithResult<()> {
        let found = singleton(tokens, RESOLVER, number)?;
        self.if_absent(RESOLVER, number)?;
        self.resolver = Some(
            parse_digest(found, RESOLVER, number)?
                .digest()
                .to_string()
                .into(),
        );
        Ok(())
    }

    fn scheme(&mut self, tokens: &[String], number: usize) -> PithResult<()> {
        let found = singleton(tokens, VERSION_SCHEME, number)?;
        self.if_absent(VERSION_SCHEME, number)?;
        self.scheme = Some(found.into());
        Ok(())
    }

    fn universe(&mut self, tokens: &[String], number: usize) -> PithResult<()> {
        let found = singleton(tokens, UNIVERSE, number)?;
        self.if_absent(UNIVERSE, number)?;
        self.universe = Some(parse_digest(found, UNIVERSE, number)?);
        Ok(())
    }

    fn preference(&mut self, tokens: &[String], number: usize) -> PithResult<()> {
        let found = singleton(tokens, PREFERENCE, number)?;
        let Some(preference) = Preference::from_name(found) else {
            return Err(diag(format!(
                "line {number}: `{found}` is not a declared preference; expected newest \
                 or oldest"
            )));
        };
        self.preferences.push(preference);
        Ok(())
    }

    fn if_absent(&self, directive: &str, number: usize) -> PithResult<()> {
        let present = match directive {
            RESOLVER => self.resolver.is_some(),
            VERSION_SCHEME => self.scheme.is_some(),
            UNIVERSE => self.universe.is_some(),
            _ => false,
        };
        if present {
            return Err(diag(format!(
                "line {number}: the `{directive}` directive appears twice; a lock carries \
                 it once"
            )));
        }
        Ok(())
    }

    fn finish(self) -> PithResult<(Box<str>, Box<str>, ContentId, PreferenceList)> {
        let resolver = self.resolver.ok_or_else(|| missing(RESOLVER))?;
        let scheme = self.scheme.ok_or_else(|| missing(VERSION_SCHEME))?;
        let universe = self.universe.ok_or_else(|| missing(UNIVERSE))?;
        Ok((
            resolver,
            scheme,
            universe,
            PreferenceList(self.preferences.into()),
        ))
    }
}

fn missing(directive: &'static str) -> pith_diag::DiagnosticSink {
    diag(format!("the lock carried no `{directive}` directive"))
}

fn singleton<'a>(tokens: &'a [String], directive: &str, number: usize) -> PithResult<&'a str> {
    match tokens {
        [_, value] => Ok(value.as_str()),
        _ => Err(diag(format!(
            "line {number}: the `{directive}` directive takes one value; found {}",
            tokens.len().saturating_sub(1)
        ))),
    }
}

/// One binding in its written spelling: the fields a witnessed line
/// carries: domain, name, version, features, and the bound content
/// identity. The transparency log's leaf record is this line's bytes, so
/// the log and the file share one spelling for one binding (0044).
#[must_use]
pub fn binding_line(entry: &LockEntry) -> String {
    format!(
        "{BIND} {} {} {} {} {SHA256}{}",
        text_token(entry.package.identity().domain().as_str()),
        text_token(entry.package.identity().name()),
        text_token(entry.package.version()),
        features_token(&entry.features),
        entry.source.digest(),
    )
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

fn bind_entry(tokens: &[String], number: usize) -> PithResult<LockEntry> {
    let [_, domain, name, version, features, source, kind, location] = tokens else {
        return Err(diag(format!(
            "line {number}: a `{BIND}` line carries domain, name, version, features, \
             source, origin kind, and origin location; found {} tokens",
            tokens.len().saturating_sub(1)
        )));
    };
    let features = parse_features(features, number)?;
    let source = parse_digest(source, "the bind source", number)?;
    let Some(origin) = Origin::from_kind(kind.as_str(), location.clone()) else {
        return Err(diag(format!(
            "line {number}: `{kind}` is not an origin kind; expected registry, forge, or \
             local-path"
        )));
    };
    Ok(LockEntry::new(
        PackageVersion::new(
            PackageIdentity::declare(DomainIdentity::new(domain.clone()), name.clone()),
            version.clone(),
        ),
        features,
        source,
        origin,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::diff;

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
        // The value orders entries by canonical encoding, whose length
        // prefixes put `zlib` (four bytes of name) before `openssl`
        // (seven). The file orders by the line's own bytes so adjacent
        // packages stay adjacent in a diff, which reverses the two. Both
        // orders are deterministic; only the file's is this one.
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
        assert_eq!(parse(&text).unwrap(), written);
        assert_eq!(
            render(&parse(&text).unwrap()),
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
        assert_eq!(parse(&text).unwrap(), quoted);
    }

    #[test]
    fn a_hand_edited_file_normalizes_to_the_canonical_bytes() {
        // A merge artifact: directives reordered, a comment added, the bind
        // lines reversed, and one digest spelled in uppercase. Everything
        // here parses, and re-rendering it produces the canonical bytes.
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
        let parsed = parse(&hand).unwrap();
        assert_eq!(render(&parsed), canonical);
        assert_eq!(parsed, lock());
    }

    #[test]
    fn a_union_merge_binding_one_package_twice_is_refused_naming_both_lines() {
        // Two branches each moved openssl; a union driver concatenated both
        // lines. The file is a set over package identities, so this is the
        // one conflict a union cannot represent: refused with both line
        // numbers and both versions, for a person to resolve.
        let canonical = render(&lock());
        let moved = canonical
            .lines()
            .find(|line| line.contains("openssl"))
            .unwrap()
            .replacen("1.1.1", "1.1.2", 1);
        let merged = format!("{canonical}{moved}\n");
        let error = parse(&merged).unwrap_err();
        let message = error
            .iter()
            .next()
            .map(|diagnostic| diagnostic.message.0.to_string())
            .unwrap_or_default();
        assert!(
            message.contains("twice")
                && message.contains("1.1.1")
                && message.contains("1.1.2")
                && message.contains("line 7")
                && message.contains("line 9"),
            "the diagnostic names both lines and both versions: {message}"
        );
    }

    #[test]
    fn a_union_merge_moving_a_feature_set_is_refused_the_same_way() {
        // The set is over package identities, not feature sets: two feature
        // selections of one package are as unrepresentable as two versions,
        // and the document's own constructor agrees (see `document`).
        let canonical = render(&lock());
        let moved = canonical
            .lines()
            .find(|line| line.contains("openssl"))
            .unwrap()
            .replacen("[shared,zlib]", "[static]", 1);
        let merged = format!("{canonical}{moved}\n");
        let error = parse(&merged).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.0.contains("twice")),
            "the diagnostic names the conflict: {error:?}"
        );
    }

    #[test]
    fn a_byte_identical_line_a_union_merge_repeated_collapses() {
        // The union of two branches that added the same binding yields the
        // same line twice; that is one resolution recorded twice, not two.
        let canonical = render(&lock());
        let repeated = canonical
            .lines()
            .find(|line| line.starts_with(BIND))
            .unwrap()
            .to_string();
        let merged = format!("{canonical}{repeated}\n");
        assert_eq!(parse(&merged).unwrap(), lock());
    }

    #[test]
    fn a_wrong_format_version_is_refused_naming_the_found_version() {
        let text = render(&lock()).replacen("lock-version 1", "lock-version 7", 1);
        let error = parse(&text).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("7") && d.message.0.contains("understands")),
            "the diagnostic names the found version: {error:?}"
        );
    }

    #[test]
    fn a_first_line_that_is_not_the_version_line_is_refused() {
        let text: String = render(&lock())
            .lines()
            .filter(|line| !line.starts_with("lock-version"))
            .collect::<Vec<_>>()
            .join("\n");
        let error = parse(&text).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("expected `lock-version")),
            "the diagnostic names what the first line had to be: {error:?}"
        );
        let empty = parse("").unwrap_err();
        assert!(
            empty
                .iter()
                .any(|d| d.message.0.contains("carried no `lock-version")),
            "an empty file names the missing first line: {empty:?}"
        );
    }

    #[test]
    fn an_unknown_directive_is_refused_naming_it() {
        let text = format!("lock-version 1\nfog sha256:00\n{}", render(&lock()));
        let error = parse(&text).unwrap_err();
        assert!(
            error.iter().any(
                |d| d.message.0.contains("fog") && d.message.0.contains("not a lock directive")
            ),
            "the diagnostic names the directive: {error:?}"
        );
    }

    #[test]
    fn a_missing_directive_is_refused_naming_it() {
        let text: String = render(&lock())
            .lines()
            .filter(|line| !line.starts_with("universe "))
            .collect::<Vec<_>>()
            .join("\n");
        let error = parse(&text).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.0.contains("no `universe`")),
            "the diagnostic names the missing directive: {error:?}"
        );
    }

    #[test]
    fn a_bad_digest_is_refused_naming_the_field_and_the_text() {
        let text = render(&lock()).replace(
            &format!("universe sha256:{}", lock().universe.digest()),
            "universe sha256:not-hex",
        );
        let error = parse(&text).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("not hexadecimal") && d.message.0.contains("line ")),
            "the diagnostic names the field, the text, and the line: {error:?}"
        );
    }

    #[test]
    fn a_bind_line_with_the_wrong_token_count_is_refused() {
        let truncated = format!(
            "lock-version 1\nresolver sha256:{}\nversion-scheme numeric-segments\nuniverse \
             sha256:{}\npreference newest\n\nbind pithpkgs zlib 1.3",
            lock().resolver,
            lock().universe.digest(),
        );
        let error = parse(&truncated).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("bind") && d.message.0.contains("tokens")),
            "the diagnostic names the line and the count: {error:?}"
        );
    }

    #[test]
    fn an_unknown_origin_kind_is_refused() {
        let text = render(&lock()).replace(" registry pkgs", " mirror pkgs");
        let error = parse(&text).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("mirror") && d.message.0.contains("origin kind")),
            "the diagnostic names the kind and the expected ones: {error:?}"
        );
    }

    #[test]
    fn an_unterminated_quote_is_refused_naming_the_line() {
        let text = format!("{}\"never closed", render(&lock()));
        let error = parse(&text).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.0.contains("line ")),
            "the diagnostic carries the line number: {error:?}"
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
