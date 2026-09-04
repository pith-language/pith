//! Names and string literals: where the quoted form is taken and where the
//! bare one is.

use super::Printer;

impl<'a> Printer<'a> {
    /// A name as the grammar spells it: bare when it is one identifier
    /// token, quoted when it is not. The two elaborate to identical bytes,
    /// so this choice moves no digest.
    pub(super) fn name(&mut self, name: &str) {
        if is_identifier(name) {
            self.out.push_str(name);
        } else {
            self.quoted(name);
        }
    }

    /// A string literal, carrying the two escapes the lexer reads and no
    /// others.
    pub(super) fn quoted(&mut self, text: &str) {
        self.out.push('"');
        for character in text.chars() {
            match character {
                '"' => self.out.push_str("\\\""),
                '\\' => self.out.push_str("\\\\"),
                other => self.out.push(other),
            }
        }
        self.out.push('"');
    }
}

/// Whether the name is one identifier token, by the lexer's own rule: an
/// identifier's shape, and not one of the keywords the parser's `name`
/// positions refuse. Everything else prints quoted, which is the form the
/// notation calls primary.
fn is_identifier(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && characters.all(|rest| rest.is_ascii_alphanumeric() || rest == '_')
        && !crate::lex::KEYWORDS.contains(&name)
}
