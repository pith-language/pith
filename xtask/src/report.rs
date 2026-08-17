use std::fmt;
use std::path::PathBuf;

pub(crate) struct Report {
    name: &'static str,
    success_message: String,
    diagnostics: Vec<Diagnostic>,
}

impl Report {
    pub(crate) fn new(name: &'static str, success_message: impl Into<String>) -> Self {
        Self {
            name,
            success_message: success_message.into(),
            diagnostics: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub(crate) fn extend(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        self.diagnostics.extend(diagnostics);
    }

    pub(crate) fn sort_diagnostics(&mut self) {
        self.diagnostics.sort();
    }

    pub(crate) fn is_success(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub(crate) fn name(&self) -> &'static str {
        self.name
    }

    pub(crate) fn success_message(&self) -> &str {
        &self.success_message
    }

    pub(crate) fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Diagnostic {
    path: PathBuf,
    line: Option<usize>,
    message: String,
}

impl Diagnostic {
    pub(crate) fn file(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            line: None,
            message: message.into(),
        }
    }

    pub(crate) fn line(path: impl Into<PathBuf>, line: usize, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            line: Some(line),
            message: message.into(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.path.display())?;
        if let Some(line) = self.line {
            write!(formatter, ":{line}")?;
        }
        write!(formatter, ": {}", self.message)
    }
}
