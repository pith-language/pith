//! Parsing the depfile a discovery pass captured.
//!
//! `cc -MM -MF deps.d source.c` writes one make rule: a target token naming
//! what would be built, then the prerequisites — the files the preprocessor
//! actually opened. GCC wraps long lines with a backslash followed by a
//! newline, so the parser joins continuations before splitting on whitespace
//! and drops the leading target token.
//!
//! The output is canonical: sorted and deduplicated, with a leading `./`
//! stripped. Canonical form is what makes the discovered set usable as a
//! request input — two sources that include the same headers in a different
//! order discover the same value, and the same source discovered twice derives
//! the same compile request.
//!
//! Prerequisites with spaces in their names arrive make-escaped (`\ `), which
//! this parser does not undo: the header universe is declared in terms of
//! ordinary paths, a path containing a space fails to resolve against it, and
//! the compile refuses at plan time rather than guessing.

use std::collections::BTreeSet;

use pith_core::Value;

use crate::types;

/// Parse depfile bytes into the canonical discovered header set: sorted,
/// deduplicated include paths, dropping the make target and joining
/// continuations. `None` when the bytes are not UTF-8, which a compiler's own
/// output being non-UTF-8 is a fact about the tool that belongs in a
/// diagnostic, not a substitution here.
#[must_use]
pub fn parse(bytes: &[u8]) -> Option<Box<[Box<str>]>> {
    let text = std::str::from_utf8(bytes).ok()?;
    let joined = join_continuations(text);
    let paths: BTreeSet<Box<str>> = joined
        .split_whitespace()
        .skip(1)
        .map(strip_dot_slash)
        .collect();
    Some(paths.into_iter().collect())
}

/// The discovered header set as a graph value: a `List<Text>` in the canonical
/// form [`parse`] produces.
#[must_use]
pub fn discovered_value(paths: &[Box<str>]) -> Value {
    types::headers(paths.iter().map(|path| path.as_ref()))
}

/// Replace backslash-newline (and backslash-CRLF) continuations with a space,
/// which is what make does before splitting a prerequisite list.
fn join_continuations(text: &str) -> String {
    text.replace("\\\r\n", " ").replace("\\\n", " ")
}

/// `./answer.h` and `answer.h` name the same file to the preprocessor, and GCC
/// emits whichever spelling the source used. One canonical form for both.
fn strip_dot_slash(path: &str) -> Box<str> {
    let mut stripped = path;
    while let Some(rest) = stripped.strip_prefix("./") {
        stripped = rest;
    }
    stripped.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_paths(text: &str) -> Box<[Box<str>]> {
        parse(text.as_bytes()).unwrap_or_else(|| unreachable!("the fixture depfile is UTF-8"))
    }

    #[test]
    fn the_target_token_is_dropped_and_prerequisites_kept() {
        let paths = parse_paths("source.o: source.c answer.h\n");
        assert_eq!(paths.as_ref(), ["answer.h".into(), "source.c".into()]);
    }

    #[test]
    fn continuations_are_joined_before_splitting() {
        let paths = parse_paths("source.o: source.c \\\n  lib/first.h \\\n  lib/second.h\n");
        assert_eq!(
            paths.as_ref(),
            [
                "lib/first.h".into(),
                "lib/second.h".into(),
                "source.c".into()
            ]
        );
    }

    #[test]
    fn duplicates_collapse_and_the_result_is_sorted() {
        // The include graph can name one header from several sites; the
        // canonical set records it once, in one order.
        let paths = parse_paths("source.o: source.c b.h a.h b.h a.h\n");
        assert_eq!(
            paths.as_ref(),
            ["a.h".into(), "b.h".into(), "source.c".into()]
        );
    }

    #[test]
    fn a_source_with_no_includes_discovers_only_itself() {
        // The source is itself the first prerequisite. The compile action
        // stages the source separately, so it filters its own path out when
        // resolving; the parser does not, because which path is "the source"
        // is the caller's fact.
        let paths = parse_paths("source.o: source.c\n");
        assert_eq!(paths.as_ref(), ["source.c".into()]);
    }

    #[test]
    fn a_leading_dot_slash_is_canonicalized_away() {
        let paths = parse_paths("source.o: ./answer.h\n");
        assert_eq!(paths.as_ref(), ["answer.h".into()]);
    }

    #[test]
    fn an_empty_depfile_parses_to_an_empty_set() {
        assert_eq!(parse_paths("\n").as_ref(), [].as_ref());
    }

    #[test]
    fn non_utf8_bytes_are_refused_rather_than_lossily_decoded() {
        assert_eq!(parse(&[0xff, 0xfe]), None);
    }

    #[test]
    fn the_discovered_value_carries_the_paths_in_order() {
        let value = discovered_value(&["a.h".into(), "b.h".into()]);
        assert_eq!(
            value,
            Value::List(
                vec![Value::Text("a.h".into()), Value::Text("b.h".into())].into_boxed_slice()
            )
        );
        assert!(value.is_type(&types::headers_type()));
    }
}
