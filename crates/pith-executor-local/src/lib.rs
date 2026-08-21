//! The first-party sandboxed local executor (decision 0028).
//!
//! Implements [`pith_engine::Executor`] for Linux by staging declared inputs
//! into a private scratch root, fork/execing the executable confined by landlock
//! and a seccomp allowlist (when installed — see `sys_landlock` and
//! `sys_seccomp`), capturing declared outputs after exit, and reporting
//! [`pith_engine::AccessVerification`] from what was actually installed rather
//! than a convention.
//!
//! # Platform
//!
//! Linux only. Building this crate on another target produces an empty crate:
//! the whole module is `cfg(target_os = "linux")`. Decision 0006 commits pith to
//! Linux first, and landlock and seccomp are Linux facilities.
//!
//! # `unsafe`
//!
//! The crate root denies `unsafe_code`. The workspace lints
//! `undocumented_unsafe_blocks = "deny"` and
//! `multiple_unsafe_ops_per_block = "deny"` are in force everywhere. `unsafe`
//! appears only in two named modules — `sys_landlock` and `sys_seccomp` —
//! each block justified by a `// SAFETY:` comment naming the foreign syscall it
//! enables, exactly as decision 0016 sanctions for sandbox setup. There is no
//! `unsafe` in staging, capture, or the process driver.

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

/// A diagnostic this executor produces for an adapter-specific failure that has
/// no dedicated `EngineCode`. Uses the opaque `StableCode(1211)` form the
/// engine's own executor-adapter tests established for pass-through adapter
/// errors.
pub(crate) const EXECUTOR_ADAPTER_CODE: StableCode = StableCode(1211);

/// Build a single-diagnostic [`DiagnosticSink`] carrying an executor adapter
/// error. Every executor failure path goes through here so the error shape is
/// uniform.
pub(crate) fn executor_diag(message: impl Into<Box<str>>) -> DiagnosticSink {
    executor_diag_as(EXECUTOR_ADAPTER_CODE, message)
}

/// The same diagnostic shape under a code the engine defines, for an adapter
/// failure that has one. The run bound (decision 0059) is the first: a child
/// killed at its deadline refuses with the bound's code so the engine can
/// record what it stopped rather than what broke.
pub(crate) fn executor_diag_as(code: StableCode, message: impl Into<Box<str>>) -> DiagnosticSink {
    let diag = Diag::new(Severity::Error, code, Span::none(), message);
    let mut sink = DiagnosticSink::new();
    sink.push(diag);
    sink
}
