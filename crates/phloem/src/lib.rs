//! Package management over the pith kernel.
//!
//! Phloem defines package identities, source bindings, constraints,
//! resolution, lock documents, substitutions, builds, and development
//! environments. Filesystem and network adapters remain outside engine rule
//! evaluation.

pub(crate) mod archive;
pub mod build;
pub(crate) mod codec;
pub mod constraint;
pub mod declarations;
pub mod description;
pub mod document;
pub mod environment;
pub mod forge;
pub mod identity;
pub mod lock;
pub mod lockfile;
pub mod lockpublish;
pub(crate) mod locktext;
pub mod preference;
pub mod registry;
pub mod resolution;
pub mod resolve;
pub mod search;
pub mod source;
pub mod substitution;
pub mod universe;
pub mod witness;

use pith_diag::{Diag, DiagnosticSink, Severity, Span, StableCode};

/// The stable code carried by phloem diagnostics.
pub(crate) const PHLOEM_CODE: u32 = 9004;

/// Creates an error diagnostic containing `message`.
pub(crate) fn diag(message: impl Into<Box<str>>) -> DiagnosticSink {
    let mut sink = DiagnosticSink::new();
    sink.push(Diag::new(
        Severity::Error,
        StableCode(PHLOEM_CODE),
        Span::none(),
        message,
    ));
    sink
}
