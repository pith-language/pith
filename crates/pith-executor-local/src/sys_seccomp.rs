//! Seccomp confinement for the sandboxed executor (decision 0028).
//!
//! # `unsafe`
//!
//! This module is one of the two sanctioned `unsafe` sites in the executor
//! crate (decision 0016: "`unsafe` is reserved for genuine foreign-function
//! boundaries where the host cannot express the operation: sandbox setup,
//! syscall interception"). The crate root denies `unsafe_code`; this module
//! allows it, and every `unsafe` block below carries a `// SAFETY:` comment
//! naming the foreign operation it enables.
//!
//! # Current state
//!
//! Today this module installs only [`set_no_new_privs`], which is the
//! prerequisite for any seccomp filter and is a single, well-understood
//! `prctl(2)` call. The full deny-by-default BPF allowlist (decision 0028) is
//! the next increment: it is substantial systems code that needs a dedicated
//! review and a test fixture that runs a child which issues a forbidden syscall
//! and is killed. Until that lands, [`seccomp_filter_installed`] reports
//! `false`, and the executor reports [`AccessVerification::Unverified`] rather
//! than claiming confinement it did not install.

#![allow(
    unsafe_code,
    reason = "seccomp setup is a sanctioned foreign-function boundary per decision 0016; every unsafe block names the syscall it enables"
)]

use std::io;

/// `PR_SET_NO_NEW_PRIVS` constant from `<linux/prctl.h>`. The value is a stable
/// kernel ABI constant; `rustix` does not wrap it in 1.1.4.
const PR_SET_NO_NEW_PRIVS: i32 = 38;

/// Whether the full deny-by-default seccomp filter is installed by this build.
///
/// `false` today: only `no_new_privs` is set. The executor uses this to decide
/// what [`AccessVerification`] to report, so the contract stays honest.
pub(super) const fn seccomp_filter_installed() -> bool {
    false
}

/// Set `PR_SET_NO_NEW_PRIVS` on the calling thread. This must be done before
/// installing a seccomp filter (the kernel requires it unless the caller is
/// already privileged) and is a permanent, irrevocable property of the process.
///
/// Intended to run in a `pre_exec` hook between `fork` and `execve`.
///
/// # Errors
/// Returns the kernel's `errno` if the `prctl` fails.
pub(super) fn set_no_new_privs() -> io::Result<()> {
    // SAFETY: `prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0)` takes no pointers and
    // mutates only the calling thread's flags. `PR_SET_NO_NEW_PRIVS` is a stable,
    // documented kernel constant; the second argument is the nonzero "set"
    // value. The remaining three arguments are unused for this operation and
    // passed as zero per the man page. The call is safe to make from a
    // `pre_exec` hook (single-threaded, post-fork, pre-exec).
    let rc = unsafe { prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Register the sandbox-setup hook on `command`, to run in the child between
/// `fork` and `execve`. This is the single place the executor reaches tokio's
/// `unsafe` `pre_exec` surface, kept inside the sanctioned module so the process
/// driver itself stays `unsafe`-free (decision 0028: "There is no `unsafe` in
/// staging, capture, or the process driver").
pub(super) fn register_sandbox_hook(command: &mut tokio::process::Command) {
    // SAFETY: `pre_exec` is unsafe because the hook runs in a forked child
    // where only async-signal-safe operations are permitted. The hook we
    // register (`set_no_new_privs`) performs a single `prctl(2)` syscall, which
    // is async-signal-safe, and nothing else. No allocations, no locks, no
    // stdio. See tokio's `CommandExt::pre_exec` documentation.
    unsafe {
        command.pre_exec(child_sandbox_hook);
    }
}

/// The async-signal-safe function the child runs after `fork` and before
/// `execve`. Today this sets `no_new_privs`; it will also install the seccomp
/// filter and landlock ruleset once those land. Must be async-signal-safe: no
/// allocations, no locks, no stdio — only direct syscalls.
fn child_sandbox_hook() -> io::Result<()> {
    set_no_new_privs()
}

unsafe extern "C" {
    // `prctl(2)`: variadic in the kernel, but the glibc wrapper is declared with
    // five fixed arguments. We declare only the signature we use.
    fn prctl(option: i32, arg2: usize, arg3: usize, arg4: usize, arg5: usize) -> i32;
}
