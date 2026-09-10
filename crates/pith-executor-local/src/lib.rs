//! The first-party sandboxed local executor.
//!
//! Implements [`pith_engine::Executor`] for Linux by staging declared inputs
//! into a private scratch root, fork/execing the executable confined by
//! landlock and a seccomp allowlist (see `sys_landlock` and `sys_seccomp`),
//! capturing declared outputs after exit, and reporting
//! [`pith_engine::AccessVerification`] from what was actually installed.
//!
//! Linux only: the whole crate is `cfg(target_os = "linux")`. `unsafe` is
//! denied at the crate root and allowed only in `sys_landlock` and
//! `sys_seccomp`, where every block carries a `// SAFETY:` comment naming the
//! syscall it enables.

#![cfg(target_os = "linux")]
#![deny(unsafe_code)]
#![forbid(missing_docs)]

mod capture;
mod process;
mod stage;
mod sys_landlock;
mod sys_seccomp;

pub use process::LocalExecutor;

use pith_diag::{Diag, DiagnosticSink, Severity, Span, StableCode};

/// Stable code for an adapter failure with no dedicated `EngineCode`: the
/// opaque `StableCode(1211)` form the engine's executor-adapter tests
/// established for pass-through adapter errors.
pub(crate) const EXECUTOR_ADAPTER_CODE: StableCode = StableCode(1211);

/// A single-diagnostic [`DiagnosticSink`] carrying an executor adapter error,
/// the shape every executor failure path uses.
pub(crate) fn executor_diag(message: impl Into<Box<str>>) -> DiagnosticSink {
    executor_diag_as(EXECUTOR_ADAPTER_CODE, message)
}

/// The same shape under a code the engine defines, such as the run bound: a
/// child killed at its deadline refuses with the bound's code so the engine
/// records what stopped it.
pub(crate) fn executor_diag_as(code: StableCode, message: impl Into<Box<str>>) -> DiagnosticSink {
    let diag = Diag::new(Severity::Error, code, Span::none(), message);
    let mut sink = DiagnosticSink::new();
    sink.push(diag);
    sink
}
