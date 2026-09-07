//! The registry's token half: the shared tokenizer, and the span helper
//! the line parsers use for "everything after here is suspect" refusals.

pub use pith_diag::text::Refusal as Malformed;
pub(crate) use pith_diag::text::{Token, tokenize};

use pith_diag::Span;

/// The span from `position`'s token to the line's end: everything after
/// the last well-formed field is suspect together.
pub(crate) fn span_from(tokens: &[Token], position: usize) -> Span {
    let start = tokens
        .get(position)
        .map_or_else(Span::none, |token| token.span);
    let end = tokens.last().map_or(start, |last| last.span);
    Span::new(start.start, end.end)
}
