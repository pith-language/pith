use std::process::ExitCode;

use pith_diag::Diag;
use pith_query::{FailureKind, QueryError};

pub struct Failure {
    kind: FailureKind,
    message: Box<str>,
    diagnostics: Box<[Diag]>,
}

impl Failure {
    pub fn user(message: impl Into<Box<str>>) -> Self {
        Self::new(FailureKind::User, message)
    }

    pub fn internal(message: impl Into<Box<str>>) -> Self {
        Self::new(FailureKind::Internal, message)
    }

    fn new(kind: FailureKind, message: impl Into<Box<str>>) -> Self {
        Self {
            kind,
            message: message.into(),
            diagnostics: Box::new([]),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn diagnostics(&self) -> &[Diag] {
        &self.diagnostics
    }

    #[cfg(test)]
    pub fn kind(&self) -> FailureKind {
        self.kind
    }

    pub fn exit_code(&self) -> ExitCode {
        match self.kind {
            FailureKind::User => ExitCode::from(1),
            FailureKind::Internal => ExitCode::from(2),
        }
    }
}

impl From<QueryError> for Failure {
    fn from(error: QueryError) -> Self {
        Self {
            kind: error.kind(),
            message: error.message().into(),
            diagnostics: error.diagnostics().into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_error_keeps_its_kind_across_the_driver_boundary() {
        assert_eq!(
            Failure::from(QueryError::user("no such file")).kind(),
            FailureKind::User
        );
        assert_eq!(
            Failure::from(QueryError::internal("the store is corrupt")).kind(),
            FailureKind::Internal
        );
    }
}
