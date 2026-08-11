//! Landlock filesystem confinement for the sandboxed executor (decisions 0028,
//! 0030).
//!
//! The ruleset is deny-by-default over the scratch root, the declared
//! executable, and each declared toolchain closure path.
//!
//! # `unsafe`
//!
//! This module is one of the two sanctioned `unsafe` sites in the executor
//! crate (decision 0016). The crate root denies `unsafe_code`; this module
//! allows it. Only the three landlock syscalls are reached raw, because rustix
//! 1.1.4 does not wrap them; `openat` goes through rustix.
//!
//! # Where this runs
//!
//! [`restrict_to`] runs in the child's `pre_exec` hook, between `fork` and
//! `execve`, so it must be async-signal-safe. That is why [`SandboxPaths`]
//! holds NUL-terminated strings built by the parent: the hook allocates
//! nothing.

#![allow(
    unsafe_code,
    reason = "landlock setup is a sanctioned foreign-function boundary per decision 0016; every unsafe block names the syscall it enables"
)]

use std::ffi::{CStr, CString};
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use rustix::fs::{CWD, FileType, Mode, OFlags, openat};

// Landlock syscall numbers on Linux x86_64/aarch64 (stable kernel ABI). rustix
// 1.1.4 does not wrap these, so they are declared directly below.
const SYS_LANDLOCK_CREATE_RULESET: i64 = 444;
const SYS_LANDLOCK_ADD_RULE: i64 = 445;
const SYS_LANDLOCK_RESTRICT_SELF: i64 = 446;

const LANDLOCK_CREATE_RULESET_VERSION: usize = 1;
const LANDLOCK_RULE_PATH_BENEATH: usize = 1;

// Filesystem access rights from <linux/landlock.h>, grouped by the ABI version
// that introduced them.
const LANDLOCK_ACCESS_FS_EXECUTE: u64 = 1 << 0;
const LANDLOCK_ACCESS_FS_WRITE_FILE: u64 = 1 << 1;
const LANDLOCK_ACCESS_FS_READ_FILE: u64 = 1 << 2;
const LANDLOCK_ACCESS_FS_READ_DIR: u64 = 1 << 3;
const LANDLOCK_ACCESS_FS_REMOVE_DIR: u64 = 1 << 4;
const LANDLOCK_ACCESS_FS_REMOVE_FILE: u64 = 1 << 5;
const LANDLOCK_ACCESS_FS_MAKE_CHAR: u64 = 1 << 6;
const LANDLOCK_ACCESS_FS_MAKE_DIR: u64 = 1 << 7;
const LANDLOCK_ACCESS_FS_MAKE_REG: u64 = 1 << 8;
const LANDLOCK_ACCESS_FS_MAKE_SOCK: u64 = 1 << 9;
const LANDLOCK_ACCESS_FS_MAKE_FIFO: u64 = 1 << 10;
const LANDLOCK_ACCESS_FS_MAKE_BLOCK: u64 = 1 << 11;
const LANDLOCK_ACCESS_FS_MAKE_SYM: u64 = 1 << 12;
// ABI v2.
const LANDLOCK_ACCESS_FS_REFER: u64 = 1 << 13;
// ABI v3.
const LANDLOCK_ACCESS_FS_TRUNCATE: u64 = 1 << 14;
// ABI v5. (v4 adds network rights, which this ruleset does not handle.)
const LANDLOCK_ACCESS_FS_IOCTL_DEV: u64 = 1 << 15;

/// The highest landlock ABI whose filesystem rights this module names. Rights a
/// newer kernel adds go unhandled, and unhandled rights are permitted.
const HIGHEST_KNOWN_ABI: u32 = 5;

/// Whether the landlock ruleset is installed by this build. The executor reports
/// [`pith_engine::AccessVerification`] from this and its seccomp counterpart.
pub(super) const fn landlock_installed() -> bool {
    true
}

/// Every filesystem right the running kernel defines, capped at
/// [`HIGHEST_KNOWN_ABI`]. Serves as both the ruleset's handled mask and the
/// grant on the scratch root, where the action has full authority.
const fn all_access_fs(abi: u32) -> u64 {
    let abi = if abi > HIGHEST_KNOWN_ABI {
        HIGHEST_KNOWN_ABI
    } else {
        abi
    };
    let mut mask = LANDLOCK_ACCESS_FS_EXECUTE
        | LANDLOCK_ACCESS_FS_WRITE_FILE
        | LANDLOCK_ACCESS_FS_READ_FILE
        | LANDLOCK_ACCESS_FS_READ_DIR
        | LANDLOCK_ACCESS_FS_REMOVE_DIR
        | LANDLOCK_ACCESS_FS_REMOVE_FILE
        | LANDLOCK_ACCESS_FS_MAKE_CHAR
        | LANDLOCK_ACCESS_FS_MAKE_DIR
        | LANDLOCK_ACCESS_FS_MAKE_REG
        | LANDLOCK_ACCESS_FS_MAKE_SOCK
        | LANDLOCK_ACCESS_FS_MAKE_FIFO
        | LANDLOCK_ACCESS_FS_MAKE_BLOCK
        | LANDLOCK_ACCESS_FS_MAKE_SYM;
    if abi >= 2 {
        mask |= LANDLOCK_ACCESS_FS_REFER;
    }
    if abi >= 3 {
        mask |= LANDLOCK_ACCESS_FS_TRUNCATE;
    }
    if abi >= 5 {
        mask |= LANDLOCK_ACCESS_FS_IOCTL_DEV;
    }
    mask
}

/// Rights on the declared executable and each closure path. No write right: the
/// toolchain lives in the host filesystem, shared with every other action that
/// declares it.
const fn read_execute_access() -> u64 {
    LANDLOCK_ACCESS_FS_EXECUTE | LANDLOCK_ACCESS_FS_READ_FILE | LANDLOCK_ACCESS_FS_READ_DIR
}

/// The rights landlock accepts on a rule whose path is not a directory. Granting
/// a directory-only right on a file fails the rule with `EINVAL`, and a closure
/// names both: a store directory is a directory, a shared object is a file.
const fn file_access_fs(abi: u32) -> u64 {
    let mut mask =
        LANDLOCK_ACCESS_FS_EXECUTE | LANDLOCK_ACCESS_FS_WRITE_FILE | LANDLOCK_ACCESS_FS_READ_FILE;
    if abi >= 3 {
        mask |= LANDLOCK_ACCESS_FS_TRUNCATE;
    }
    if abi >= 5 {
        mask |= LANDLOCK_ACCESS_FS_IOCTL_DEV;
    }
    mask
}

/// The paths the child may touch, as NUL-terminated bytes so the `pre_exec` hook
/// allocates nothing.
pub(super) struct SandboxPaths {
    scratch_root: CString,
    /// The declared executable followed by each closure path.
    read_execute: Box<[CString]>,
}

impl SandboxPaths {
    /// The executable is granted from `executable` itself, so a caller never has
    /// to repeat it in `closure_paths`.
    ///
    /// # Errors
    /// Returns `Err` when a path contains a NUL byte and so cannot become a C
    /// string, which fails the sandbox closed before any child is forked.
    pub(super) fn new(
        scratch_root: &Path,
        executable: &str,
        closure_paths: &[Box<str>],
    ) -> io::Result<Self> {
        let mut read_execute = Vec::with_capacity(closure_paths.len().saturating_add(1));
        read_execute.push(c_path(executable.as_bytes())?);
        for path in closure_paths {
            read_execute.push(c_path(path.as_bytes())?);
        }
        Ok(Self {
            scratch_root: c_path(scratch_root.as_os_str().as_bytes())?,
            read_execute: read_execute.into_boxed_slice(),
        })
    }
}

fn c_path(bytes: &[u8]) -> io::Result<CString> {
    CString::new(bytes).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
}

/// Install the landlock ruleset on the calling thread. Fails closed: an `Err`
/// here aborts the exec rather than running the child unconfined.
///
/// # Errors
/// Returns the kernel's `errno` when landlock is unavailable, a ruleset cannot
/// be created, a declared path cannot be opened, or `landlock_restrict_self` is
/// denied.
pub(super) fn restrict_to(paths: &SandboxPaths) -> io::Result<()> {
    let abi = abi_version()?;
    let handled = all_access_fs(abi);
    let ruleset = create_ruleset(handled)?;
    add_path_rule(ruleset.as_fd(), &paths.scratch_root, handled, abi)?;
    for path in &paths.read_execute {
        add_path_rule(ruleset.as_fd(), path, read_execute_access(), abi)?;
    }
    restrict_self(ruleset.as_fd())
}

/// Probe the running kernel's highest supported landlock ABI version. Returns
/// `Err` when landlock is entirely unavailable (`ENOSYS`) or the call is
/// rejected, which fails the sandbox closed.
fn abi_version() -> io::Result<u32> {
    // SAFETY: `landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION)`
    // is the documented probe: a NULL attr with zero size and the VERSION flag
    // returns the highest supported ABI version as a positive integer, taking no
    // pointers and mutating no state. The trailing zero arguments pad to the
    // fixed arity this module declares for `syscall`.
    let rc = unsafe {
        syscall(
            SYS_LANDLOCK_CREATE_RULESET,
            0,
            0,
            LANDLOCK_CREATE_RULESET_VERSION,
            0,
            0,
        )
    };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    u32::try_from(rc).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))
}

/// Create a deny-by-default ruleset governing `handled_access_fs`.
fn create_ruleset(handled_access_fs: u64) -> io::Result<OwnedFd> {
    let attr = LandlockRulesetAttr { handled_access_fs };
    let attr_ptr = (&raw const attr).cast::<u8>();
    // SAFETY: `landlock_create_ruleset(&attr, size_of::<attr>(), 0)` reads the
    // ruleset attribute from a valid pointer to a fully-initialized struct of
    // the size passed alongside it, and returns a ruleset fd or a negative
    // errno. `attr_ptr` points at `attr`, which outlives the call.
    let rc = unsafe {
        syscall(
            SYS_LANDLOCK_CREATE_RULESET,
            attr_ptr as usize,
            size_of::<LandlockRulesetAttr>(),
            0,
            0,
            0,
        )
    };
    owned_fd_from_rc(rc)
}

/// Add a `LANDLOCK_RULE_PATH_BENEATH` rule granting `allowed_access` on the
/// hierarchy rooted at `path`, narrowed to what the path's type accepts.
fn add_path_rule(
    ruleset: BorrowedFd<'_>,
    path: &CStr,
    allowed_access: u64,
    abi: u32,
) -> io::Result<()> {
    // Symlinks are followed, so `/bin/sh` names the hierarchy it resolves to.
    let parent = openat(CWD, path, OFlags::PATH | OFlags::CLOEXEC, Mode::empty())?;
    let allowed_access = if is_directory(parent.as_fd())? {
        allowed_access
    } else {
        allowed_access & file_access_fs(abi)
    };
    add_rule(ruleset, parent.as_fd(), allowed_access)
}

fn is_directory(fd: BorrowedFd<'_>) -> io::Result<bool> {
    let stat = rustix::fs::fstat(fd)?;
    Ok(FileType::from_raw_mode(stat.st_mode) == FileType::Directory)
}

/// `landlock_add_rule(ruleset_fd, LANDLOCK_RULE_PATH_BENEATH, &attr, 0)`.
fn add_rule(
    ruleset: BorrowedFd<'_>,
    parent: BorrowedFd<'_>,
    allowed_access: u64,
) -> io::Result<()> {
    let attr = LandlockPathBeneathAttr {
        allowed_access,
        parent_fd: parent.as_raw_fd(),
    };
    let attr_ptr = (&raw const attr).cast::<u8>();
    // SAFETY: `landlock_add_rule` reads the path-beneath attribute from a valid
    // pointer to the packed struct the kernel expects for this rule type.
    // `attr_ptr` points at `attr`, which outlives the call, and the descriptors
    // it names are open for the duration.
    let rc = unsafe {
        syscall(
            SYS_LANDLOCK_ADD_RULE,
            fd_arg(ruleset),
            LANDLOCK_RULE_PATH_BENEATH,
            attr_ptr as usize,
            0,
            0,
        )
    };
    unit_from_rc(rc)
}

/// `landlock_restrict_self(ruleset_fd, 0)`: bind the ruleset to the calling
/// thread. Irrevocable, and survives `execve`.
fn restrict_self(ruleset: BorrowedFd<'_>) -> io::Result<()> {
    // SAFETY: `landlock_restrict_self(ruleset_fd, 0)` takes a valid ruleset file
    // descriptor and no flags, and mutates only the calling thread's
    // restrictions.
    let rc = unsafe { syscall(SYS_LANDLOCK_RESTRICT_SELF, fd_arg(ruleset), 0, 0, 0, 0) };
    unit_from_rc(rc)
}

/// A descriptor as a syscall argument. Always non-negative here, so the
/// widening is exact.
fn fd_arg(fd: BorrowedFd<'_>) -> usize {
    fd.as_raw_fd() as usize
}

fn unit_from_rc(rc: i64) -> io::Result<()> {
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn owned_fd_from_rc(rc: i64) -> io::Result<OwnedFd> {
    let raw = i32::try_from(rc).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `raw` is a fresh, non-negative descriptor the kernel just returned
    // from `landlock_create_ruleset`. Nothing else holds it, so transferring
    // ownership to `OwnedFd` is sound and gives it a single closer.
    Ok(unsafe { OwnedFd::from_raw_fd(raw) })
}

/// `landlock_ruleset_attr`. `handled_access_net` (ABI v4+) is omitted; the
/// kernel treats fields beyond the size passed alongside the pointer as zero.
#[repr(C)]
struct LandlockRulesetAttr {
    handled_access_fs: u64,
}

/// `landlock_path_beneath_attr`, packed per `<linux/landlock.h>`:
/// `{ u64 allowed_access; s32 parent_fd; }`.
#[repr(C, packed)]
struct LandlockPathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

unsafe extern "C" {
    /// `syscall(2)` is variadic in C. A fixed-arity declaration is sound because
    /// the SysV ABI passes the leading integer arguments in the same registers
    /// regardless of trailing variadicness.
    fn syscall(number: i64, arg1: usize, arg2: usize, arg3: usize, arg4: usize, arg5: usize)
    -> i64;
}
