//! Packaging over the pith kernel.
//!
//! `phloem` is the first-party package library (docs/foundation/name.md): the
//! tissue that carries the products of builds outward, as packages. This
//! slice prototypes decisions 0039 and 0040 over the constructors 0026
//! landed: a package is named by declaration inside a domain, a package
//! version adds coordinates in the comparison the domain declares, a
//! description is a record value, a source binding is a declared sum with
//! typed payloads, a lock entry binds coordinates to the content identity of
//! the resolved source with the origin as evidence rather than as identity,
//! constraints are values over the domain's ordering, and resolution is a
//! host-rule computation in the graph whose answer carries its explanation.
//!
//! phloem is a peer of `xylem` and a consumer of it, never a wrapper
//! (decisions 0009, 0039): it produces build requests against interfaces
//! xylem already declares, and a build with no package defined anywhere works
//! without this crate. The lock's file format and the development
//! environment are later records in the same milestone; nothing here
//! anticipates them.

pub(crate) mod codec;
pub mod constraint;
pub mod description;
pub mod identity;
pub mod lock;
pub mod preference;
pub mod request;
pub mod resolution;
pub mod resolve;
pub mod search;
pub mod source;
pub mod universe;

use pith_diag::{Diag, DiagnosticSink, Severity, Span, StableCode};

/// The stable code every phloem diagnostic carries. xylem's rules diagnose
/// under 9002; packaging failures get their own so a report names the library
/// the failure came from.
pub(crate) const PHLOEM_CODE: u32 = 9004;

/// An error diagnostic carrying `message`, in the shape the kernel's rule
/// bodies hand back to the engine.
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
