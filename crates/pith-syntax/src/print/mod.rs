//! The canonical spelling of a parsed module. Printing is a pure function of
//! the surface, the printed text re-parses to the same surface, and formatting
//! takes part in no canonical encoding.
//!
//! The spelling is split the way the grammar is: this file holds the module
//! and its items, and one sibling file per tier below — layout, names,
//! types, requests, and expressions.

mod expression;
mod items;
mod layout;
mod names;
mod request;
mod types;

use pith_diag::{SourceFile, Span};
use pith_hir::{
    ParsedSurface, SurfaceAbout, SurfaceComment, SurfaceDeclaration, SurfaceEntry, SurfaceImport,
    SurfaceLocal, SurfaceRule,
};

use layout::slice;

/// The canonical text of `surface`, whose documentation comments are sliced
/// from `source`.
#[must_use]
pub fn print(surface: &ParsedSurface, source: &SourceFile) -> String {
    let mut printer = Printer {
        surface,
        text: source.source_text(),
        out: String::new(),
        indent: 0,
        written: false,
        pending_comment: false,
    };
    printer.module();
    printer.out
}

struct Printer<'a> {
    surface: &'a ParsedSurface,
    text: &'a str,
    out: String,
    indent: usize,
    written: bool,
    pending_comment: bool,
}

enum ModuleEvent<'a> {
    Comment(&'a SurfaceComment),
    Import(&'a SurfaceImport),
    Declaration(&'a SurfaceDeclaration),
    Rule(&'a SurfaceRule),
    Local(&'a SurfaceLocal),
    Entry(&'a SurfaceEntry),
    About(&'a SurfaceAbout),
}

impl ModuleEvent<'_> {
    fn start(&self) -> u32 {
        match self {
            Self::Comment(comment) => comment.span.start.0,
            Self::Import(import) => import.span.start.0,
            Self::Declaration(declaration) => declaration.name_span.start.0,
            Self::Rule(rule) => rule.span.start.0,
            Self::Local(local) => local.span.start.0,
            Self::Entry(entry) => entry.span.start.0,
            Self::About(about) => about.span.start.0,
        }
    }
}

impl<'a> Printer<'a> {
    /// Top-level items and comments remain in source order. This keeps comments
    /// attached to the text they describe without requiring a token-preserving
    /// concrete syntax tree.
    fn module(&mut self) {
        let mut events = Vec::new();
        events.extend(self.surface.comments.iter().map(ModuleEvent::Comment));
        events.extend(self.surface.imports.iter().map(ModuleEvent::Import));
        events.extend(
            self.surface
                .declarations
                .iter()
                .map(ModuleEvent::Declaration),
        );
        events.extend(self.surface.rules.iter().map(ModuleEvent::Rule));
        events.extend(self.surface.locals.iter().map(ModuleEvent::Local));
        events.extend(self.surface.entries.iter().map(ModuleEvent::Entry));
        events.extend(self.surface.about.iter().map(ModuleEvent::About));
        events.sort_by_key(ModuleEvent::start);

        for event in events {
            match event {
                ModuleEvent::Comment(comment) if comment.trailing => {
                    self.trailing_comment(comment.span)
                }
                ModuleEvent::Comment(comment) => self.leading_comment(comment.span),
                ModuleEvent::Import(import) => self.item(|printer| printer.import(import)),
                ModuleEvent::Declaration(declaration) => {
                    self.item(|printer| printer.declaration(declaration))
                }
                ModuleEvent::Rule(rule) => self.item(|printer| printer.rule(rule)),
                ModuleEvent::Local(local) => self.item(|printer| printer.local(local)),
                ModuleEvent::Entry(entry) => self.item(|printer| printer.entry(entry)),
                ModuleEvent::About(about) => self.item(|printer| printer.about(about)),
            }
        }
        if self.written {
            self.out.push('\n');
        }
    }

    fn item(&mut self, spell: impl FnOnce(&mut Self)) {
        if self.pending_comment {
            self.out.push('\n');
            self.pending_comment = false;
        } else if self.written {
            self.out.push_str("\n\n");
        }
        self.written = true;
        spell(self);
    }

    fn leading_comment(&mut self, span: Span) {
        if self.pending_comment {
            self.out.push('\n');
        } else if self.written {
            self.out.push_str("\n\n");
        }
        self.written = true;
        self.pending_comment = true;
        self.comment(span);
    }

    fn trailing_comment(&mut self, span: Span) {
        if !self.written || self.pending_comment {
            self.leading_comment(span);
            return;
        }
        self.out.push_str("  ");
        self.comment(span);
    }

    fn comment(&mut self, span: Span) {
        let comment = slice(self.text, span)
            .strip_prefix("--")
            .unwrap_or_default()
            .trim();
        self.out.push_str("--");
        if !comment.is_empty() {
            self.out.push(' ');
            self.out.push_str(comment);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::print;
    use crate::parse;
    use pith_diag::{SourceFile, SourceId};
    use std::sync::Arc;

    /// The canonical text of `text`, which must parse without a diagnostic.
    fn canonical(text: &str) -> String {
        let source = Arc::new(SourceFile::new(
            SourceId::from_raw(0),
            "test.pi",
            text.trim_start_matches('\n'),
        ));
        let (surface, diagnostics) = parse(&source);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        print(&surface, &source)
    }

    #[test]
    fn the_canonical_spelling_is_flat() {
        let canonical = canonical(
            "\nimport   xylem\n\n\nnominal X=Text\nsum  S = | a | b(Int)\nsum One = only\n\
             pure rule \"r\"(x: Int,y) -> Int = host\n",
        );
        assert_eq!(
            canonical,
            "import xylem\n\nnominal X = Text\n\nsum S =\n  | a\n  | b(Int)\n\nsum One = only\n\n\
             pure rule r(x: Int, y) -> Int = host\n"
        );
    }

    #[test]
    fn a_written_body_takes_the_block_form() {
        let canonical = canonical(
            "pure rule twice(x: Int) -> Int = { let doubled : Int = ask Int (x) let y = doubled \
             twice(doubled) }",
        );
        assert_eq!(
            canonical,
            "pure rule twice(x: Int) -> Int = {\n  let doubled : Int = ask Int (x)\n  let y = \
             doubled\n  twice(doubled)\n}\n"
        );
    }

    #[test]
    fn the_five_requests_keep_their_keywords() {
        let canonical = canonical(
            "pure rule r(Unit) -> Int = {\n  \
             let a : Int = ask Int (1)\n  \
             let (b, c) = ask all (ask Int (a), ask Text (\"x\"))\n  \
             let d : List<Int> = ask all [for x in [a] { if x == 1 } (x)]\n  \
             let e : Int = ask (2)\n  \
             bytes of \"tail\"\n\
             }",
        );
        assert_eq!(
            canonical,
            "pure rule r(Unit) -> Int = {\n  \
             let a : Int = ask Int (1)\n  \
             let (b, c) = ask all (ask Int (a), ask Text (\"x\"))\n  \
             let d : List<Int> = ask all Int [for x in [a] { if x == 1 } (x)]\n  \
             let e : Int = ask Int (2)\n  \
             bytes of \"tail\"\n\
             }\n"
        );
    }

    #[test]
    fn precedence_reprints_only_the_parentheses_it_needs() {
        assert_eq!(
            canonical("pure rule r(Unit) -> Int = { (1 + 2) * 3 }"),
            "pure rule r(Unit) -> Int = {\n  (1 + 2) * 3\n}\n"
        );
        assert_eq!(
            canonical("pure rule r(Unit) -> Int = { 1 + (2 * 3) }"),
            "pure rule r(Unit) -> Int = {\n  1 + 2 * 3\n}\n"
        );
        assert_eq!(
            canonical("pure rule r(Unit) -> Bool = { 1 == 2 != true }"),
            "pure rule r(Unit) -> Bool = {\n  1 == 2 != true\n}\n"
        );
    }

    #[test]
    fn a_record_in_a_condition_keeps_the_parentheses_the_parser_demands() {
        assert_eq!(
            canonical("pure rule r(Unit) -> Int = { if ({a: 1} == r(2)) { 3 } else { 4 } }"),
            "pure rule r(Unit) -> Int = {\n  if ({a: 1}) == r(2) {\n    3\n  } else {\n    4\n  }\n\
             }\n"
        );
    }

    #[test]
    fn the_control_forms_take_the_block_form() {
        let canonical = canonical(
            "pure rule m(s: Shape) -> Int = { match s { circle(r) { r } point { 0 } } }\n\n\
             pure rule i(s: Shape) -> Int = { if s == point { 1 } else if s == circle(2) { 2 } else { 3 } }\n\n\
             pure rule f(xs: List<Int>) -> Int = { fold xs from 0 { (x, acc) -> acc + x } }",
        );
        assert_eq!(
            canonical,
            "pure rule m(s: Shape) -> Int = {\n  \
             match s {\n    circle(r) {\n      r\n    }\n    point {\n      0\n    }\n  }\n\
             }\n\n\
             pure rule i(s: Shape) -> Int = {\n  \
             if s == point {\n    1\n  } else if s == circle(2) {\n    2\n  } else {\n    3\n  }\n\
             }\n\n\
             pure rule f(xs: List<Int>) -> Int = {\n  \
             fold xs from 0 {\n    (x, acc) -> acc + x\n  }\n\
             }\n"
        );
    }

    #[test]
    fn names_the_identifier_grammar_cannot_spell_print_quoted() {
        let canonical = canonical(
            "nominal \"expected-owner\" = Text\n\
             type quoted = {\"expected-owner\": Text, plain: Int}\n\
             pure rule \"hyphen-ated\"(\"expected-owner\") -> Text = host",
        );
        assert_eq!(
            canonical,
            "nominal \"expected-owner\" = Text\n\n\
             type quoted = {\"expected-owner\": Text, plain: Int}\n\n\
             pure rule \"hyphen-ated\"(\"expected-owner\") -> Text = host\n"
        );
    }

    #[test]
    fn string_literals_keep_only_their_two_escapes() {
        assert_eq!(
            canonical("pure rule r(Unit) -> Text = { \"a\\\"b\\\\c\" }"),
            "pure rule r(Unit) -> Text = {\n  \"a\\\"b\\\\c\"\n}\n"
        );
    }

    #[test]
    fn documentation_and_metadata_reprint_canonically() {
        let canonical = canonical(
            "--   spaced comment  \n-- second line\nnominal X = Text\n\n\
             about { description: \"the notation\" , maintainers: [\"karol\", \"ren\"] , }",
        );
        assert_eq!(
            canonical,
            "-- spaced comment\n-- second line\nnominal X = Text\n\n\
             about {\n  description: \"the notation\",\n  maintainers: [\"karol\", \"ren\"],\n}\n"
        );
    }

    #[test]
    fn entries_locals_and_elided_heads_reprint_explicitly() {
        let canonical =
            canonical("let subject : Text = ask (\"which\")\n\nentry dev : Text = ask (subject)");
        assert_eq!(
            canonical,
            "let subject : Text = ask Text (\"which\")\n\nentry dev : Text = ask Text (subject)\n"
        );
    }

    #[test]
    fn printing_is_idempotent_over_every_canonical_text_above() {
        let samples = [
            "import xylem\n\nnominal X = Text\n",
            "sum S =\n  | a\n  | b(Int)\n",
            "pure rule twice(x: Int) -> Int = {\n  let doubled : Int = ask Int (x)\n  \
             twice(doubled)\n}\n",
            "pure rule r(Unit) -> Int = {\n  if ({a: 1}) == r(2) {\n    3\n  } else {\n    4\n  }\n\
             }\n",
            "about {\n  description: \"the notation\",\n}\n",
            "entry dev : Text = ask Text (\"which\")\n",
        ];
        for sample in samples {
            let once = canonical(sample);
            assert_eq!(&once, sample, "the sample was not canonical");
            assert_eq!(canonical(&once), once, "printing printed text moved it");
        }
    }
}
