//! Packaging over the pith kernel.
//!
//! `phloem` is the first-party package library (docs/foundation/name.md): the
//! tissue that carries the products of builds outward, as packages. This
//! slice prototypes decisions 0039 through 0043 over the constructors 0026
//! landed: a package is named by declaration inside a domain, a package
//! version adds coordinates in the comparison the domain declares, a
//! description is a record value, a source binding is a declared sum with
//! typed payloads, a lock entry binds coordinates to the content identity of
//! the resolved source with the origin as evidence rather than as identity,
//! constraints are values over the domain's ordering, resolution is a
//! host-rule computation in the graph whose answer carries its explanation,
//! a written lock is a text projection of the lock document that the
//! caller writes at the effect boundary, a prebuilt binary stands in for
//! a realization only through an admission test over that lock's binding,
//! and a development environment is a value over the lock — one resolution
//! plus the realization coordinates it declares and the substitutions it
//! served — whose materialization is a projection and whose entering is a
//! caller effect that does not exist yet.

pub(crate) mod codec;
pub mod constraint;
pub mod description;
pub mod document;
pub mod environment;
pub mod identity;
pub mod lock;
pub mod lockfile;
pub mod lockpublish;
pub(crate) mod locktext;
pub mod preference;
pub mod request;
pub mod resolution;
pub mod resolve;
pub mod search;
pub mod source;
pub mod substitution;
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
