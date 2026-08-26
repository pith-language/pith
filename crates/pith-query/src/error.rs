use pith_diag::Diag;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FailureKind {
    User,
    Internal,
}

#[derive(Debug)]
pub struct QueryError {
    kind: FailureKind,
    message: Box<str>,
    diagnostics: Box<[Diag]>,
}

impl QueryError {
    #[must_use]
    pub fn user(message: impl Into<Box<str>>) -> Self {
        Self {
            kind: FailureKind::User,
            message: message.into(),
            diagnostics: Box::new([]),
        }
    }

    #[must_use]
    pub fn internal(message: impl Into<Box<str>>) -> Self {
        Self {
            kind: FailureKind::Internal,
            message: message.into(),
            diagnostics: Box::new([]),
        }
    }

    #[must_use]
    pub fn with_diagnostics(mut self, diagnostics: impl Into<Box<[Diag]>>) -> Self {
        self.diagnostics = diagnostics.into();
        self
    }

    #[must_use]
    pub const fn kind(&self) -> FailureKind {
        self.kind
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[Diag] {
        &self.diagnostics
    }
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for QueryError {}
