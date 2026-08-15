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
//! Writing and reading are caller effects at 0003's boundary. No rule sees
//! a path; the engine computes the value, and whoever drives it renders
//! and writes. Publication follows 0024's content discipline: a flushed
//! temporary file renamed into place, so a crash cannot leave a torn lock.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;

use pith_diag::PithResult;
use pith_ids::{ContentDigest, ContentId};

use crate::diag;
use crate::document::Lock;
use crate::identity::{DomainIdentity, PackageIdentity, PackageVersion};
use crate::lock::{LockEntry, Origin};
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

const REGISTRY: &str = "registry";
const FORGE: &str = "forge";
const LOCAL_PATH: &str = "local-path";

const SHA256: &str = "sha256:";
const HEX_LEN: usize = 64;

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
    let _ = writeln!(out, "{VERSION_SCHEME} {}", token(&lock.scheme));
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
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming the line, what was expected, and
/// what was found, for every form this format does not accept.
pub fn parse(text: &str) -> PithResult<Lock> {
    let mut header = Header::default();
    let mut entries = Vec::new();
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
            BIND => entries.push(bind_entry(&tokens, number)?),
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
    Ok(Lock::new(resolver, scheme, universe, preferences, entries))
}

/// Write the lock to `path`, through a flushed temporary file renamed into
/// place on the publication discipline the content store uses (0024). A
/// caller-side effect: no rule reaches this.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming the path and the failure when the
/// file cannot be written or renamed.
pub fn write(lock: &Lock, path: &Path) -> PithResult<()> {
    let text = render(lock);
    let temporary = temporary_path(path);
    let outcome = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    outcome.map_err(|error| io_diag("writing the lock", path, &error))
}

/// Read the lock at `path` and parse it. A caller-side effect, the read
/// side of the same boundary.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming the path and the failure when the
/// file cannot be read, and the parse diagnostics when it is not a lock.
pub fn read(path: &Path) -> PithResult<Lock> {
    let text =
        std::fs::read_to_string(path).map_err(|error| io_diag("reading the lock", path, &error))?;
    parse(&text)
}

fn io_diag(what: &str, path: &Path, error: &std::io::Error) -> pith_diag::DiagnosticSink {
    diag(format!("{what} at {} failed: {error}", path.display()))
}

fn temporary_path(path: &Path) -> std::path::PathBuf {
    let name = path.file_name().map_or_else(
        || std::ffi::OsString::from("pith.lock"),
        |name| name.to_os_string(),
    );
    let mut temporary = path.with_file_name(name);
    temporary.as_mut_os_string().push(".tmp");
    temporary
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
        let preference = match found {
            "newest" => Preference::Newest,
            "oldest" => Preference::Oldest,
            other => {
                return Err(diag(format!(
                    "line {number}: `{other}` is not a declared preference; expected \
                     newest or oldest"
                )));
            }
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

fn bind_line(entry: &LockEntry) -> String {
    let (kind, location) = origin_parts(&entry.origin);
    format!(
        "{BIND} {} {} {} {} {SHA256}{} {kind} {}",
        token(entry.package.identity().domain().as_str()),
        token(entry.package.identity().name()),
        token(entry.package.version()),
        features_token(&entry.features),
        entry.source.digest(),
        token(location),
    )
}

fn origin_parts(origin: &Origin) -> (&'static str, &str) {
    match origin {
        Origin::Registry(location) => (REGISTRY, location),
        Origin::Forge(location) => (FORGE, location),
        Origin::LocalPath(path) => (LOCAL_PATH, path),
    }
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
    let origin = match kind.as_str() {
        REGISTRY => Origin::Registry(location.clone().into()),
        FORGE => Origin::Forge(location.clone().into()),
        LOCAL_PATH => Origin::LocalPath(location.clone().into()),
        other => {
            return Err(diag(format!(
                "line {number}: `{other}` is not an origin kind; expected {REGISTRY}, \
                 {FORGE}, or {LOCAL_PATH}"
            )));
        }
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

fn features_token(features: &[Box<str>]) -> String {
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

fn parse_features(text: &str, number: usize) -> PithResult<Box<[Box<str>]>> {
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

/// One text field in its written spelling: bare when it contains none of
/// the reserved characters, quoted with backslash escapes otherwise.
fn token(text: &str) -> String {
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

fn is_bare(text: &str) -> bool {
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
fn tokenize(line: &str) -> Result<Vec<String>, String> {
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
    // Consume the opening quote.
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
            for character in chars.by_ref() {
                match character {
                    '}' => break,
                    digit if digit.is_ascii_hexdigit() => hex.push(digit),
                    other => return Err(format!("`{other}` is not a hexadecimal digit")),
                }
            }
            u32::from_str_radix(&hex, 16)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| format!("`\\u{{{hex}}}` is not a unicode scalar value"))
        }
        other => Err(format!("`\\{other}` is not an escape this format defines")),
    }
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

/// A digest in its written spelling — `sha256:` followed by 64 hexadecimal
/// digits — decoded to the identity it names. Uppercase digits parse and
/// re-render lowercase, which is one instance of the canonicalization every
/// parseable text gets.
fn parse_digest(text: &str, field: &str, number: usize) -> PithResult<ContentId> {
    let Some(hex) = text.strip_prefix("sha256:") else {
        return Err(diag(format!(
            "line {number}: {field} carried `{text}` rather than a `sha256:`-prefixed digest"
        )));
    };
    let Some(nibbles) = hex
        .chars()
        .map(|character| {
            character
                .to_digit(16)
                .and_then(|digit| u8::try_from(digit).ok())
        })
        .collect::<Option<Vec<u8>>>()
    else {
        return Err(diag(format!(
            "line {number}: {field} carried `{hex}`, which is not hexadecimal"
        )));
    };
    let wrong_length = || {
        diag(format!(
            "line {number}: {field} carried {} hexadecimal digits rather than {HEX_LEN}",
            nibbles.len()
        ))
    };
    if nibbles.len() != HEX_LEN {
        return Err(wrong_length());
    }
    let collected: Vec<u8> = nibbles
        .chunks_exact(2)
        .map(|pair| pair.iter().fold(0u8, |byte, nibble| (byte << 4) | nibble))
        .collect();
    let bytes: [u8; 32] = collected
        .as_slice()
        .try_into()
        .map_err(|_| wrong_length())?;
    Ok(ContentId::from_digest(ContentDigest::from_bytes(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::diff;
    use tempfile::TempDir;

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
        let binds: Vec<&str> = text
            .lines()
            .filter(|line| line.starts_with(BIND))
            .collect();
        let Some((first, second)) = binds.first().zip(binds.get(1)) else {
            unreachable!("the fixture holds two entries");
        };
        assert!(
            first.contains("openssl") && second.contains("zlib"),
            "the file's line order puts openssl before zlib: {binds:?}"
        );
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
    fn writing_and_reading_round_trip_through_a_file() {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("pith.lock");
        write(&lock(), &path).unwrap();
        assert!(path.exists());
        assert!(
            !directory.path().join("pith.lock.tmp").exists(),
            "the temporary file is renamed away"
        );
        assert_eq!(read(&path).unwrap(), lock());
        let error = read(&directory.path().join("absent.lock")).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("absent.lock") && d.message.0.contains("reading")),
            "the diagnostic names the path and the operation: {error:?}"
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
        );
        assert_ne!(render(&base), render(&moved));
        let unchanged = Lock::new(
            base.resolver.clone(),
            base.scheme.clone(),
            base.universe,
            base.preferences.clone(),
            base.entries.iter().rev().cloned().collect::<Vec<_>>(),
        );
        assert_eq!(render(&base), render(&unchanged));
        assert_eq!(diff(&base, &moved).changes.len(), 1);
    }
}
