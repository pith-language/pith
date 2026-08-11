//! Landlock filesystem confinement for the sandboxed executor (decision 0028).
//!
//! # `unsafe`
//!
//! This module is one of the two sanctioned `unsafe` sites in the executor
//! crate (decision 0016). The crate root denies `unsafe_code`; this module
//! allows it, and every `unsafe` block will carry a `// SAFETY:` comment naming
//! the `landlock_*` syscall it enables.
//!
//! # Current state
//!
//! rustix 1.1.4 does not wrap the landlock syscalls, and `landlock(2)` is
//! intricate enough that a correct, reviewed implementation is a dedicated
//! increment. Until that lands, [`landlock_installed`] reports `false` and the
//! executor reports [`pith_engine::AccessVerification::Unverified`] (or
//! `Observed` when seccomp alone is installed) rather than claiming path
//! confinement it did not install. This keeps the
//! [`pith_engine::AccessVerification::Prevented`] claim honest: it is reported
//! only when *both* layers are installed.

#![allow(
    unsafe_code,
    reason = "landlock setup is a sanctioned foreign-function boundary per decision 0016; every unsafe block names the syscall it enables"
)]

/// Whether the landlock ruleset is installed by this build.
///
/// `false` today. The executor uses this to decide what
/// [`pith_engine::AccessVerification`] to report, so the contract stays honest.
pub(super) const fn landlock_installed() -> bool {
    false
}
