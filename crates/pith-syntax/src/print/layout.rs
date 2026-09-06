//! The text the spellings are written into: indentation, blocks, joined
//! lists, and the one span lookup documentation needs.

use pith_diag::Span;

use super::Printer;

/// One level of indentation, two spaces, everywhere.
pub(super) const INDENT: &str = "  ";

impl<'a> Printer<'a> {
    /// The start of a line, at the current indentation.
    pub(super) fn newline(&mut self) {
        self.out.push('\n');
        for _ in 0..self.indent {
            self.out.push_str(INDENT);
        }
    }

    /// The brace-bounded block a construct's lines sit inside, one indent
    /// deeper than the construct's own line.
    pub(super) fn open_block(&mut self) {
        self.out.push_str(" {");
        self.indent = self.indent.saturating_add(1);
    }

    pub(super) fn close_block(&mut self) {
        self.indent = self.indent.saturating_sub(1);
        self.newline();
        self.out.push('}');
    }

    /// Each item separated by `separator`, which the last item does not
    /// carry.
    pub(super) fn joined<T>(&mut self, items: &[T], separator: &str, write: fn(&mut Self, &T)) {
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                self.out.push_str(separator);
            }
            write(self, item);
        }
    }
}

/// The text one span covers, when it covers any.
pub(super) fn slice(text: &str, span: Span) -> &str {
    let start = usize::try_from(span.start.0).unwrap_or(0);
    let end = usize::try_from(span.end.0).unwrap_or(0);
    text.get(start..end).unwrap_or("")
}

/// The item-and-comment spacing both printers share: when a blank line
/// separates two clauses, when a comment hangs where an item will follow, and
/// the canonical spelling of a comment line. The spellings themselves stay
/// with each document's printer.
pub(super) struct Spacing {
    written: bool,
    pending_comment: bool,
}

impl Spacing {
    pub(super) const fn new() -> Self {
        Self {
            written: false,
            pending_comment: false,
        }
    }

    /// Before an item: two newlines after the previous item, one after a
    /// pending comment.
    pub(super) fn begin_item(&mut self, out: &mut String) {
        if self.pending_comment {
            out.push('\n');
            self.pending_comment = false;
        } else if self.written {
            out.push_str("\n\n");
        }
        self.written = true;
    }

    /// Before a leading comment: the spacing of an item, with the next item
    /// or comment continuing on the following line.
    pub(super) fn begin_leading_comment(&mut self, out: &mut String) {
        self.begin_item(out);
        self.pending_comment = true;
    }

    /// A comment on the item's own line, indented to trail it. Returns
    /// whether the comment could trail; a first comment or one after another
    /// comment leads instead.
    pub(super) fn trailing_comment(&mut self, out: &mut String) -> bool {
        if !self.written || self.pending_comment {
            return false;
        }
        out.push_str("  ");
        true
    }

    /// The canonical comment: `--` and the trimmed comment text.
    pub(super) fn comment(out: &mut String, text: &str) {
        let comment = text.strip_prefix("--").unwrap_or_default().trim();
        out.push_str("--");
        if !comment.is_empty() {
            out.push(' ');
            out.push_str(comment);
        }
    }

    /// The final newline of a nonempty document.
    pub(super) fn finish(&mut self, out: &mut String) {
        if self.written {
            out.push('\n');
        }
    }
}
