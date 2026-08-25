//! The text the spellings are written into: indentation, blocks, joined
//! lists, and the one span lookup documentation needs.

use pith_diag::Span;

use super::Printer;

/// One level of indentation, two spaces, everywhere.
const INDENT: &str = "  ";

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
