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
//! This module installs `no_new_privs` and then hands to [`crate::sys_landlock`]
//! to install the path-confinement ruleset. The full deny-by-default BPF seccomp
//! allowlist (decision 0028) is the next increment: it is substantial systems
//! code that needs a dedicated review and a test fixture that runs a child
//! which issues a forbidden syscall and is killed. Until that lands,
//! [`seccomp_filter_installed`] reports `false`, so the executor reports
//! [`pith_engine::AccessVerification::Observed`] (landlock installed, seccomp
//! absent) rather than `Prevented`, which 0028 reserves for both layers.

#![allow(
    unsafe_code,
    reason = "seccomp setup is a sanctioned foreign-function boundary per decision 0016; every unsafe block names the syscall it enables"
)]

use std::io;

use crate::sys_landlock::{SandboxPaths, restrict_to};

/// `PR_SET_NO_NEW_PRIVS` constant from `<linux/prctl.h>`. The value is a stable
/// kernel ABI constant; `rustix` does not wrap it in 1.1.4.
const PR_SET_NO_NEW_PRIVS: i32 = 38;

/// Whether the full deny-by-default seccomp filter is installed by this build.
///
/// `false` today: only `no_new_privs` is set. The executor uses this to decide
/// what [`pith_engine::AccessVerification`] to report, so the contract stays honest.
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
/// `fork` and `execve`. The hook sets `no_new_privs` and then installs the
/// landlock ruleset over `paths`. This is the single place the executor reaches
/// tokio's `unsafe` `pre_exec` surface, kept inside the sanction module so the
/// process driver itself stays `unsafe`-free (decision 0028: "There is no
/// `unsafe` in staging, capture, or the process driver").
pub(super) fn register_sandbox_hook(command: &mut tokio::process::Command, paths: SandboxPaths) {
    let hook = move || child_sandbox_hook(&paths);
    // SAFETY: `pre_exec` is unsafe because the hook runs in a forked child
    // where only async-signal-safe operations are permitted. The hook we
    // register sets `no_new_privs` via a single `prctl(2)` and then installs the
    // landlock ruleset via `landlock_*` syscalls and `openat`, all of which are
    // async-signal-safe. The captured `SandboxPaths` holds pre-built NUL-
    // terminated strings, so the hook allocates nothing. See tokio's
    // `CommandExt::pre_exec` documentation.
    unsafe {
        command.pre_exec(hook);
    }
}

/// The async-signal-safe function the child runs after `fork` and before
/// `execve`. Sets `no_new_privs`, then installs the landlock ruleset. Must be
/// async-signal-safe: no allocations, no locks, no stdio, only direct syscalls.
/// The `SandboxPaths` it takes are pre-built by the parent so nothing here
/// allocates.
fn child_sandbox_hook(paths: &SandboxPaths) -> io::Result<()> {
    set_no_new_privs()?;
    restrict_to(paths)
}

unsafe extern "C" {
    // `prctl(2)`: variadic in the kernel, but the glibc wrapper is declared with
    // five fixed arguments. We declare only the signature we use.
    fn prctl(option: i32, arg2: usize, arg3: usize, arg4: usize, arg5: usize) -> i32;
}
