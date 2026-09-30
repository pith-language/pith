//! The project header's productions: the clauses that precede a project
//! file's declarations. The parser is the declaration grammar's, over tokens
//! lexed in the header's region, where subjects and hyphenated names are
//! single identifiers and `<=`/`>=` are single tokens. The header ends at
//! the first token that opens none of its clauses; the declarations re-lex
//! from that byte under the declaration grammar's token rules.

use pith_diag::Span;
use pith_hir::{
    FrontendCode, InputsBlock, ProjectHeader, ProjectHost, ProjectInclude, ProjectInput,
    ProjectMember, ProjectModule, ProjectWorkspace, SurfaceComment,
};

use crate::lex::{TokenKind, error};
use crate::parse::Parser;

/// Where a header clause stands in the header's fixed order, so a clause
/// that arrives after one it precedes is refused by name.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Stage {
    Module = 0,
    Inputs = 1,
    Host = 2,
    Workspace = 3,
    Include = 4,
}

/// The clause word and the stage it opens, in the order the header spells
/// them. One table: the item parser's wrong-document diagnostic and this
/// parser's ordering read the same words.
const ORDERED_CLAUSES: [(&str, Stage); 5] = [
    ("module", Stage::Module),
    ("inputs", Stage::Inputs),
    ("host", Stage::Host),
    ("workspace", Stage::Workspace),
    ("include", Stage::Include),
];

impl Parser<'_> {
    /// Parse the header clauses and return them with the byte offset where
    /// the declarations begin.
    pub(crate) fn project_header(&mut self) -> (ProjectHeader, usize) {
        let mut module = None;
        let mut inputs = InputsBlock {
            span: pith_diag::Span::point(pith_diag::ByteOffset(0)),
            inputs: Vec::new().into(),
            registries: Vec::new().into(),
            domains: Vec::new().into(),
        };
        let mut host = None;
        let mut workspace = None;
        let mut includes = Vec::new();
        let mut stage = Stage::Module;
        let mut saw_inputs = false;
        while self.peek().kind != TokenKind::End {
            if self.peek().kind == TokenKind::LineComment {
                // A comment belongs to the header only when a clause follows
                // it; the comments before the first declaration document
                // that declaration, so the header stops there.
                if !self.comment_precedes_clause() {
                    break;
                }
                let trailing = self.comment_is_trailing();
                let span = self.take().span;
                self.comments.push(SurfaceComment { span, trailing });
                continue;
            }
            let Some((word, clause_stage)) = self.header_clause_word() else {
                break;
            };
            if clause_stage < stage {
                let token = self.take();
                self.diagnostics.push(error(
                    FrontendCode::WrongDocument,
                    token.span,
                    format!(
                        "`{word}` follows the clauses that precede it; the header order is \
                         `module`, `inputs`, `host`, `workspace`, then `include`"
                    ),
                    self.source,
                ));
                self.skip_clause();
                continue;
            }
            stage = clause_stage;
            let keyword = self.peek().span;
            match word {
                "module" => {
                    if let Some(clause) = self.module_clause() {
                        set_singleton(
                            "module",
                            keyword,
                            &mut module,
                            clause,
                            &mut self.diagnostics,
                            self.source,
                        );
                    }
                }
                "inputs" => {
                    if self.inputs_block(&mut inputs).is_some() {
                        saw_inputs = true;
                    }
                }
                "host" => {
                    if let Some(clause) = self.host_clause() {
                        set_singleton(
                            "host",
                            keyword,
                            &mut host,
                            clause,
                            &mut self.diagnostics,
                            self.source,
                        );
                    }
                }
                "workspace" => {
                    if let Some(clause) = self.workspace_clause() {
                        set_singleton(
                            "workspace",
                            keyword,
                            &mut workspace,
                            clause,
                            &mut self.diagnostics,
                            self.source,
                        );
                    }
                }
                "include" => {
                    if let Some(clause) = self.include_clause(&includes) {
                        includes.push(clause);
                    }
                }
                _ => unreachable!("the table names the words the loop matched"),
            }
        }
        if !saw_inputs {
            self.diagnostics.push(error(
                FrontendCode::UnexpectedToken,
                self.peek().span,
                "a project file declares an `inputs` block, and this file holds none; a file \
                 with no header is an include",
                self.source,
            ));
        }
        (
            ProjectHeader {
                module,
                inputs,
                host,
                workspace,
                includes: includes.into(),
            },
            usize::try_from(self.peek().span.start.0).unwrap_or(0),
        )
    }

    fn header_clause_word(&self) -> Option<(&'static str, Stage)> {
        if self.peek().kind != TokenKind::Ident {
            return None;
        }
        ORDERED_CLAUSES
            .iter()
            .find(|(word, _)| *word == self.lexeme(self.peek()))
            .copied()
    }

    /// Whether a header clause follows the run of comments at the cursor.
    fn comment_precedes_clause(&self) -> bool {
        let mut ahead = self.position.saturating_add(1);
        while let Some(token) = self.tokens.get(ahead) {
            match token.kind {
                TokenKind::LineComment => ahead = ahead.saturating_add(1),
                TokenKind::End => return false,
                TokenKind::Ident => {
                    return ORDERED_CLAUSES
                        .iter()
                        .any(|(word, _)| *word == self.lexeme(token));
                }
                _ => return false,
            }
        }
        false
    }

    /// Skip to the next token that could open a header clause, after a
    /// production refused what it read.
    fn skip_clause(&mut self) {
        while self.peek().kind != TokenKind::End
            && self.peek().kind != TokenKind::LineComment
            && self.header_clause_word().is_none()
        {
            self.take();
        }
    }

    fn module_clause(&mut self) -> Option<ProjectModule> {
        let keyword = self.take();
        let (subject, subject_span) = self.subject("the project's `domain/name` subject")?;
        let version = self.version()?;
        Some(ProjectModule {
            subject,
            subject_span,
            version,
            span: keyword.span,
        })
    }

    /// The `inputs { ... }` block: input bindings, registry bindings, and
    /// domain routes, one per line with commas between them.
    fn inputs_block(&mut self, block: &mut InputsBlock) -> Option<()> {
        let keyword = self.take();
        block.span = keyword.span;
        self.expect(TokenKind::LBrace, "`{` opening the inputs block")?;
        let mut inputs = std::mem::take(&mut block.inputs).into_vec();
        let mut registries = std::mem::take(&mut block.registries).into_vec();
        let mut domains = std::mem::take(&mut block.domains).into_vec();
        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::End {
            if self.peek().kind == TokenKind::LineComment {
                let token = self.take();
                self.diagnostics.push(error(
                    FrontendCode::UnexpectedToken,
                    token.span,
                    "a comment sits between the header's clauses; the inputs block holds                      entries only",
                    self.source,
                ));
                continue;
            }
            let parsed = match (self.peek().kind, self.lexeme(self.peek())) {
                (TokenKind::Ident, "registry") => self
                    .registry_clause(&registries)
                    .map(|clause| registries.push(clause)),
                (TokenKind::Ident, "domain") => {
                    self.domain_clause().map(|clause| domains.push(clause))
                }
                _ => self.input_entry(&inputs).map(|input| inputs.push(input)),
            };
            if parsed.is_none() {
                self.skip_to_entry_end();
            }
            if self.peek().kind == TokenKind::Comma {
                self.take();
            }
        }
        self.expect(TokenKind::RBrace, "`}` closing the inputs block")?;
        block.inputs = inputs.into();
        block.registries = registries.into();
        block.domains = domains.into();
        Some(())
    }

    /// One `name = locator` entry. A typed value entry and the `with` and
    /// `grant` suffixes are parsed far enough to name them and refused.
    fn input_entry(&mut self, prior: &[ProjectInput]) -> Option<ProjectInput> {
        let keyword = self.peek().span;
        let name = self.name("an input's name")?;
        if prior
            .iter()
            .any(|bound| bound.name.as_ref() == name.text.as_ref())
        {
            self.diagnostics.push(error(
                FrontendCode::DuplicateBinding,
                name.span,
                format!("the inputs block binds the name `{}` twice", name.text),
                self.source,
            ));
            return None;
        }
        if self.peek().kind == TokenKind::Colon {
            let colon = self.take();
            self.skip_to_entry_end();
            self.diagnostics.push(error(
                FrontendCode::UnsupportedInput,
                colon.span,
                format!(
                    "`{}` annotates a typed value input, and a typed value input is not \
                     served; an input names a project by a locator",
                    name.text
                ),
                self.source,
            ));
            return None;
        }
        self.expect(TokenKind::Eq, "`=` between an input's name and its locator")?;
        let locator = self.locator()?;
        self.input_suffix();
        Some(ProjectInput {
            name: name.text,
            name_span: name.span,
            locator,
            span: keyword.join(self.previous_span()),
        })
    }

    fn host_clause(&mut self) -> Option<ProjectHost> {
        let keyword = self.take();
        let adapter = self.name("a host adapter's name")?;
        if self.peek().kind != TokenKind::Dot {
            self.expected("the artifact's path, written `./...`");
            return None;
        }
        // A path literal is a maximal run of path bytes, so the artifact
        // stops where the file's text does and cannot swallow the clause
        // that follows it.
        let start = usize::try_from(self.peek().span.start.0).unwrap_or(0);
        let text = self.source.source_text();
        let bytes = text.as_bytes();
        let mut end = start;
        while let Some(&byte) = bytes.get(end) {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'/' | b'_' | b'-') {
                end = end.saturating_add(1);
            } else {
                break;
            }
        }
        let artifact = text.get(start..end).unwrap_or_default();
        while self.peek().kind != TokenKind::End
            && usize::try_from(self.peek().span.start.0).unwrap_or(0) < end
        {
            self.take();
        }
        Some(ProjectHost {
            adapter: adapter.text,
            artifact: artifact.into(),
            span: keyword.span,
        })
    }

    fn workspace_clause(&mut self) -> Option<ProjectWorkspace> {
        let keyword = self.take();
        self.expect(TokenKind::LBrace, "`{` opening the workspace")?;
        self.expect_ident("members", "`members` naming the member list")?;
        self.expect(TokenKind::Colon, "`:` after `members`")?;
        let members = self.member_list()?;
        if self.peek().kind == TokenKind::Comma {
            self.take();
        }
        self.expect(TokenKind::RBrace, "`}` closing the workspace")?;
        Some(ProjectWorkspace {
            members: members.into(),
            span: keyword.span,
        })
    }

    fn member_list(&mut self) -> Option<Vec<ProjectMember>> {
        self.expect(TokenKind::LBracket, "`[` opening the member list")?;
        let mut members = Vec::new();
        while self.peek().kind == TokenKind::Str {
            let token = self.take();
            members.push(ProjectMember {
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

    fn include_clause(&mut self, prior: &[ProjectInclude]) -> Option<ProjectInclude> {
        let keyword = self.take();
        let path = self.quoted("a quoted path after `include`")?;
        let spelling = path.text.clone().unwrap_or_default();
        if spelling.is_empty() || spelling.starts_with('/') || spelling.starts_with("./") {
            self.diagnostics.push(error(
                FrontendCode::InvalidPath,
                path.span,
                "an include path names its file relative to the project file, without a \
                 leading `./` and neither empty nor absolute",
                self.source,
            ));
            return None;
        }
        if prior
            .iter()
            .any(|bound| bound.path.as_ref() == spelling.as_ref())
        {
            self.diagnostics.push(error(
                FrontendCode::DuplicateBinding,
                path.span,
                format!("the project includes `{spelling}` twice"),
                self.source,
            ));
            return None;
        }
        Some(ProjectInclude {
            path: spelling,
            span: keyword.span,
        })
    }
}

fn set_singleton<T>(
    name: &str,
    keyword: Span,
    slot: &mut Option<T>,
    parsed: T,
    diagnostics: &mut Vec<pith_diag::Diag>,
    source: &std::sync::Arc<pith_diag::SourceFile>,
) {
    if slot.is_some() {
        diagnostics.push(error(
            FrontendCode::DuplicateClause,
            keyword,
            format!("a project declares at most one `{name}` clause"),
            source,
        ));
    } else {
        *slot = Some(parsed);
    }
}

#[cfg(test)]
mod tests {
    use super::ORDERED_CLAUSES;

    /// The lexer's wrong-document diagnostic and this parser's ordering
    /// spell the same words in the same order, so a clause added to one
    /// table and not the other fails here rather than passing silently.
    #[test]
    fn the_header_clause_tables_are_one_list() {
        let ordered = ORDERED_CLAUSES
            .iter()
            .map(|(word, _)| *word)
            .collect::<Vec<_>>();
        assert_eq!(ordered.as_slice(), crate::lex::HEADER_CLAUSES);
    }
}
