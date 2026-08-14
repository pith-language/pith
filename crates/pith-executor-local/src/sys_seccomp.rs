//! Seccomp syscall confinement for the sandboxed executor (decision 0028).
//!
//! # `unsafe`
//!
//! This module is one of the two sanctioned `unsafe` sites in the executor
//! crate (decision 0016: "`unsafe` is reserved for genuine foreign-function
//! boundaries where the host cannot express the operation: sandbox setup,
//! syscall interception"). The crate root denies `unsafe_code`; this module
//! allows it, and every `unsafe` block below carries a `// SAFETY:` comment
//! naming the foreign operation it enables. Two blocks remain: loading the
//! filter, and registering the hook that loads it.
//!
//! # What is installed
//!
//! The deny-by-default BPF allowlist decision 0028 describes, widened by the
//! measurement its unresolved section records: the original fifteen-entry list
//! named a third of the forty-five syscalls a traced compile issues, and the
//! missing part was structural (process creation, because the driver is a
//! supervisor). The list below starts from that measured union and from the
//! fixtures that run real children through this executor. Every entry is named
//! and justified; an addition needs a concrete failure it fixes, never a
//! broadening for its own sake.
//!
//! `socket` is allowed only for `AF_UNIX`, which is the form 0028's unresolved
//! section names for the local name-service lookup glibc performs during an
//! ordinary compile. `connect` is allowed wholesale because no socket outside
//! `AF_UNIX` can exist to connect once `socket(2)` itself is filtered. Egress
//! beyond the local socket remains the network-namespace design's question.
//!
//! # Where the numbers come from
//!
//! Syscall numbers, the `seccomp_data` layout, the BPF opcodes, and the filter
//! ABI constants all come from `libc`, and none may be written out here. A
//! number that disagrees with the architecture still compiles: it denies the
//! syscall its name claims and grants whatever else holds the slot. The kill
//! that follows reaches the reader as `SIGSYS` from a child whose stderr the
//! executor has already taken, so nothing points back at the number.
//!
//! # Where this runs
//!
//! The filter is installed in the child's `pre_exec` hook after
//! `no_new_privs` and after the landlock ruleset, matching 0028's ordering.
//! The BPF program is built by the parent before the fork (see
//! [`SeccompFilter`]), so the hook itself allocates nothing.

#![allow(
    unsafe_code,
    reason = "seccomp setup is a sanctioned foreign-function boundary per decision 0016; every unsafe block names the syscall it enables"
)]

use std::io;

use crate::sys_landlock::{SandboxPaths, restrict_to};

/// Whether the deny-by-default seccomp filter is installed by this build.
///
/// `true` where the filter is compiled in: Linux x86_64, the platform whose
/// syscall numbers [`ALLOWED_SYSCALLS`] carries. Elsewhere the executor cannot
/// report better than [`pith_engine::AccessVerification::Observed`], which is
/// the honest reading, since the filter this module describes is absent there.
pub(super) const fn seccomp_filter_installed() -> bool {
    cfg!(target_arch = "x86_64")
}

/// Set the calling thread's `no_new_privs` attribute. This must be done before
/// installing a seccomp filter (the kernel requires it unless the caller is
/// already privileged) and is a permanent, irrevocable property of the process.
///
/// Intended to run in a `pre_exec` hook between `fork` and `execve`, where
/// rustix's raw-syscall backend is what makes the call allocation-free.
///
/// # Errors
/// Returns the kernel's `errno` if the `prctl` fails.
fn set_no_new_privs() -> io::Result<()> {
    rustix::thread::set_no_new_privs(true)
        .map_err(|errno| io::Error::from_raw_os_error(errno.raw_os_error()))
}

/// The seccomp BPF program, compiled before the fork so the child's
/// `pre_exec` hook allocates nothing. A unit on platforms the filter does not
/// target, where [`SeccompFilter::install`] is a no-op and
/// [`seccomp_filter_installed`] answers `false`.
pub(super) struct SeccompFilter {
    #[cfg(target_arch = "x86_64")]
    program: Box<[libc::sock_filter]>,
}

impl SeccompFilter {
    /// Compile the allowlist into the BPF program the kernel will load. Safe
    /// code throughout: this runs in the parent, before any fork.
    #[must_use]
    pub(super) fn build() -> Self {
        Self {
            #[cfg(target_arch = "x86_64")]
            program: build_program().into(),
        }
    }

    /// Load the filter onto the calling thread. Fails closed: an `Err` here
    /// aborts the exec rather than running the child unconfined.
    ///
    /// # Errors
    /// Returns the kernel's `errno` when `seccomp(2)` rejects the program.
    pub(super) fn install(&self) -> io::Result<()> {
        #[cfg(target_arch = "x86_64")]
        {
            let len = u16::try_from(self.program.len()).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "the seccomp program exceeds the kernel's u16 instruction count",
                )
            })?;
            // `sock_fprog::filter` is `*mut` in the C declaration; the kernel
            // copies the program in and never writes through it.
            let fprog = libc::sock_fprog {
                len,
                filter: self.program.as_ptr().cast_mut(),
            };
            // SAFETY: `seccomp(SECCOMP_SET_MODE_FILTER, 0, &fprog)` loads the
            // BPF program `fprog` names into the calling thread's syscall
            // filter. `fprog` is fully initialized, its `filter` pointer names
            // the program [`SeccompFilter::build`] compiled, and both outlive
            // the call; the flags argument is zero, the only value this module
            // uses. The kernel requires `no_new_privs`, which the hook sets
            // before reaching here.
            let rc = unsafe {
                libc::syscall(
                    libc::SYS_seccomp,
                    libc::SECCOMP_SET_MODE_FILTER,
                    0,
                    &raw const fprog,
                )
            };
            if rc == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            Ok(())
        }
    }
}

/// Register the sandbox-setup hook on `command`, to run in the child between
/// `fork` and `execve`. The hook sets `no_new_privs`, installs the landlock
/// ruleset over `paths`, then loads the seccomp allowlist, in the order
/// decision 0028 fixes. This is the single place the executor reaches tokio's
/// `unsafe` `pre_exec` surface, kept inside the sanction module so the
/// process driver itself stays `unsafe`-free (decision 0028: "There is no
/// `unsafe` in staging, capture, or the process driver").
pub(super) fn register_sandbox_hook(command: &mut tokio::process::Command, paths: SandboxPaths) {
    let filter = SeccompFilter::build();
    let hook = move || child_sandbox_hook(&paths, &filter);
    // SAFETY: `pre_exec` is unsafe because the hook runs in a forked child
    // where only async-signal-safe operations are permitted. The hook we
    // register sets `no_new_privs` via a single `prctl(2)`, installs the
    // landlock ruleset via the `landlock_*` syscalls and `openat`, and loads
    // the seccomp program via `seccomp(2)`, all of which are
    // async-signal-safe. The `SandboxPaths` and the compiled `SeccompFilter`
    // it closes over are pre-built by the parent, so the hook allocates
    // nothing. See tokio's `CommandExt::pre_exec` documentation.
    unsafe {
        command.pre_exec(hook);
    }
}

/// The async-signal-safe function the child runs after `fork` and before
/// `execve`. Sets `no_new_privs`, installs the landlock ruleset, then loads
/// the seccomp filter. Must be async-signal-safe: no allocations, no locks, no
/// stdio, only direct syscalls. Everything it touches is pre-built by the
/// parent so nothing here allocates.
fn child_sandbox_hook(paths: &SandboxPaths, filter: &SeccompFilter) -> io::Result<()> {
    set_no_new_privs()?;
    restrict_to(paths)?;
    filter.install()
}

/// Expand a list of `libc` syscall constants into `(number, name)` pairs. The
/// name is the constant's own spelling, so the two cannot disagree.
#[cfg(target_arch = "x86_64")]
macro_rules! allowlist {
    ($($syscall:ident),+ $(,)?) => {
        &[$((libc::$syscall, stringify!($syscall))),+]
    };
}

/// The syscalls a build action may issue on x86_64. Deny-by-default: an entry
/// here is permission, and anything absent is `SIGSYS`.
///
/// Grouped by the role the syscall plays, every entry justified. The discipline
/// 0028 fixes is that the list is drawn from measurement, with each addition
/// naming the concrete need that produced it. A different
/// architecture needs its own measured list rather than a translation of this
/// one.
#[cfg(target_arch = "x86_64")]
const ALLOWED_SYSCALLS: &[(libc::c_long, &str)] = allowlist![
    // Byte and file-descriptor I/O, the floor for any program. `newfstatat`
    // is the form glibc actually issues where 0028's original list wrote
    // `fstat`; `statx` is what modern coreutils probe with.
    SYS_read,
    SYS_write,
    SYS_close,
    SYS_openat,
    SYS_fstat,
    SYS_stat,
    SYS_lstat,
    SYS_newfstatat,
    SYS_statx,
    SYS_lseek,
    SYS_pread64,
    // Coreutils advise the kernel to drop the cache behind an input they have
    // finished reading (`wc`, `cat`); the call is harmless to deny only if the
    // caller survives it, and a filter kills instead.
    SYS_fadvise64,
    SYS_fcntl,
    SYS_dup,
    SYS_dup2,
    SYS_dup3,
    // The driver is a supervisor: `cc` execs `cc1` and `as`, `collect2` execs
    // `ld`, and the shell fixtures pipeline their children. This is 0028's
    // structural finding, without which a compile dies at its first real step.
    SYS_clone,
    SYS_clone3,
    SYS_vfork,
    SYS_execve,
    SYS_wait4,
    SYS_pipe,
    SYS_pipe2,
    // Memory. `madvise` is glibc arena trimming, and `sysinfo` is how gcc sizes
    // its own heuristics against the host's total memory.
    SYS_mmap,
    SYS_munmap,
    SYS_mprotect,
    SYS_brk,
    SYS_madvise,
    SYS_sysinfo,
    // Signals and exit. clang installs an alternate signal stack so its crash
    // handler can run on a blown stack; gcc never asked for one.
    SYS_sigaltstack,
    SYS_rt_sigaction,
    SYS_rt_sigprocmask,
    SYS_rt_sigreturn,
    SYS_exit,
    SYS_exit_group,
    // glibc thread startup, named by 0028's trace: every dynamically linked
    // binary issues these before `main`.
    SYS_arch_prctl,
    SYS_set_tid_address,
    SYS_set_robust_list,
    SYS_rseq,
    SYS_prlimit64,
    SYS_futex,
    SYS_getpid,
    SYS_getppid,
    SYS_getpgrp,
    // Path probing: the driver's include search and the fixtures' shell
    // preambles.
    SYS_access,
    SYS_faccessat,
    SYS_faccessat2,
    SYS_readlink,
    SYS_getcwd,
    SYS_getdents64,
    // A recursive walk holds each directory open and moves by descriptor. The
    // ruleset confines what those descriptors can reach either way.
    SYS_fchdir,
    // `isatty` and friends: a child probes its stdio descriptors even when
    // they are pipes, and dies at startup if the probe is fatal.
    SYS_ioctl,
    // File creation where the fixtures write trees: `mkdir -p`, `chmod`, temp
    // files under TMPDIR, and cleanup of what they made. Coreutils `mkdir`
    // issues the plain form, not the `at` one.
    SYS_mkdir,
    SYS_mkdirat,
    SYS_chmod,
    SYS_fchmodat,
    SYS_fchmod,
    SYS_unlink,
    SYS_unlinkat,
    // clang writes its output to a temporary and renames it into place, so a
    // partial file never looks like a finished one.
    SYS_rename,
    // Randomness for temp names: glibc's `mkstemp` family draws from
    // `getrandom` and dies rather than falling back when the call is filtered.
    SYS_getrandom,
    // The shell's own housekeeping, and the identity queries it makes at
    // startup: `sh` asks for its uid/gid before running anything, and checks
    // real against effective to find out whether it is setuid. It asks for the
    // group triple straight after the user one, so the two travel together.
    SYS_umask,
    SYS_getuid,
    SYS_getgid,
    SYS_geteuid,
    SYS_getegid,
    SYS_getresuid,
    SYS_getresgid,
    // `sh` reads the kernel release at startup, and the reply says nothing
    // about the filesystem the ruleset confines.
    SYS_uname,
    // Timing: `sleep` and every clock probe. `clock_nanosleep` is the modern
    // form glibc issues where 0028's original list wrote `nanosleep`.
    SYS_clock_gettime,
    SYS_clock_nanosleep,
    SYS_nanosleep,
    // The shell's `times` and gcc's own accounting both read the resource
    // usage the kernel already tracks for them.
    SYS_getrusage,
    // clang arms a watchdog around its own work. Denying it would kill the
    // compile over a timer it never intended to fire.
    SYS_alarm,
    // Local sockets: glibc's name-service switch opens an `AF_UNIX` stream to
    // the nscd door during an ordinary compile (0028's measurement). `connect`
    // follows because no other socket can exist to connect.
    SYS_connect,
];

/// The syscalls allowed only for a particular first argument. Each is emitted
/// after [`ALLOWED_SYSCALLS`] as a self-contained block.
///
/// `socket(2)` and `prctl(2)` both take their first argument as an `int`, so
/// the kernel discards the high half of the register; comparing the low word is
/// the same test the kernel applies.
#[cfg(target_arch = "x86_64")]
const ARGUMENT_FILTERED_SYSCALLS: &[ArgumentFiltered] = &[
    // A build action reaching for a non-local socket is trying to leave the
    // machine, which `NetworkPolicy::Deny` promises it cannot. Refusing that
    // with an errno would let it fall back and carry on quietly, so it dies.
    ArgumentFiltered {
        number: libc::SYS_socket,
        name: "SYS_socket",
        argument: libc::AF_UNIX as u32,
        refusal: libc::SECCOMP_RET_KILL_PROCESS,
    },
    // `prctl` is a multiplexer over several dozen operations, some of which
    // change how the process is traced or what privileges it can gain.
    // Coreutils set their own process name at startup, which is the one
    // operation the fixtures need. They then ask for `PR_SET_MM`, which needs
    // `CAP_SYS_RESOURCE` and so already fails for an ordinary caller, a
    // best-effort call whose failure the caller handles. Killing it would make
    // the sandbox stricter than the kernel it stands in for, so the unnamed
    // operations get the `EPERM` an unprivileged process would have seen.
    ArgumentFiltered {
        number: libc::SYS_prctl,
        name: "SYS_prctl",
        argument: libc::PR_SET_NAME as u32,
        refusal: errno_action(libc::EPERM),
    },
];

/// One entry of [`ARGUMENT_FILTERED_SYSCALLS`]: a syscall the filter admits
/// only for a single value of its first argument, and the action it takes when
/// the argument is something else.
#[cfg(target_arch = "x86_64")]
struct ArgumentFiltered {
    number: libc::c_long,
    #[allow(
        dead_code,
        reason = "the name is how the unit tests report a failing entry; the filter itself compares numbers"
    )]
    name: &'static str,
    argument: u32,
    refusal: libc::c_uint,
}

/// The filter action that fails a call with `errno` instead of killing the
/// process. The errno travels in the action word's low half.
#[cfg(target_arch = "x86_64")]
const fn errno_action(errno: libc::c_int) -> libc::c_uint {
    libc::SECCOMP_RET_ERRNO | (errno as libc::c_uint & libc::SECCOMP_RET_DATA)
}

/// `AUDIT_ARCH_X86_64` from `<linux/audit.h>`: `EM_X86_64` widened by the
/// 64-bit and little-endian flags. libc does not export the audit
/// architecture constants, and this is the one value the filter needs.
#[cfg(target_arch = "x86_64")]
const AUDIT_ARCH_X86_64: u32 = 0xC000_003E;

/// Offsets of the `struct seccomp_data` fields the filter loads. `args` is an
/// array, so its offset is that of `args[0]`, the first syscall argument.
#[cfg(target_arch = "x86_64")]
const SECCOMP_NR_OFFSET: u32 = std::mem::offset_of!(libc::seccomp_data, nr) as u32;
#[cfg(target_arch = "x86_64")]
const SECCOMP_ARCH_OFFSET: u32 = std::mem::offset_of!(libc::seccomp_data, arch) as u32;
#[cfg(target_arch = "x86_64")]
const SECCOMP_ARG0_OFFSET: u32 = std::mem::offset_of!(libc::seccomp_data, args) as u32;

/// `BPF_LD | BPF_W | BPF_ABS`: load a word at an offset into `seccomp_data`.
#[cfg(target_arch = "x86_64")]
const BPF_LD_W_ABS: u16 = (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16;
/// `BPF_JMP | BPF_JEQ | BPF_K`: compare the accumulator to a constant.
#[cfg(target_arch = "x86_64")]
const BPF_JMP_JEQ_K: u16 = (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16;
/// `BPF_RET | BPF_K`: end the program on a constant action.
#[cfg(target_arch = "x86_64")]
const BPF_RET_K: u16 = (libc::BPF_RET | libc::BPF_K) as u16;

/// Assemble the deny-by-default program: an architecture gate, the plain
/// allowlist, the argument-filtered entries, and a kill on everything else.
#[cfg(target_arch = "x86_64")]
fn build_program() -> Vec<libc::sock_filter> {
    // A call from another architecture is killed, since these numbers were
    // never measured for one.
    let mut program = vec![
        load(SECCOMP_ARCH_OFFSET),
        compare(AUDIT_ARCH_X86_64, 1, 0),
        ret(libc::SECCOMP_RET_KILL_PROCESS),
        load(SECCOMP_NR_OFFSET),
    ];
    for &(number, _name) in ALLOWED_SYSCALLS {
        program.push(compare(syscall_word(number), 0, 1));
        program.push(ret(libc::SECCOMP_RET_ALLOW));
    }
    for entry in ARGUMENT_FILTERED_SYSCALLS {
        // Each block reloads `nr` so it does not depend on what the block
        // before it left in the accumulator: a preceding argument test that
        // failed leaves an argument word there, and comparing that against a
        // syscall number would allow a call nobody named.
        program.push(load(SECCOMP_NR_OFFSET));
        program.push(compare(syscall_word(entry.number), 0, 4));
        program.push(load(SECCOMP_ARG0_OFFSET));
        program.push(compare(entry.argument, 0, 1));
        program.push(ret(libc::SECCOMP_RET_ALLOW));
        program.push(ret(entry.refusal));
    }
    program.push(ret(libc::SECCOMP_RET_KILL_PROCESS));
    program
}

/// A syscall number as the BPF accumulator sees it. Every number on this
/// architecture fits, and a hypothetical one that did not would compare
/// against a value no call can hold.
#[cfg(target_arch = "x86_64")]
fn syscall_word(number: libc::c_long) -> u32 {
    u32::try_from(number).unwrap_or(u32::MAX)
}

/// `BPF_LD | BPF_W | BPF_ABS` at `offset` in `seccomp_data`.
#[cfg(target_arch = "x86_64")]
fn load(offset: u32) -> libc::sock_filter {
    libc::sock_filter {
        code: BPF_LD_W_ABS,
        jt: 0,
        jf: 0,
        k: offset,
    }
}

/// `BPF_JMP | BPF_JEQ | BPF_K`: jump `jt` instructions when the accumulator
/// equals `constant`, `jf` when it does not.
#[cfg(target_arch = "x86_64")]
fn compare(constant: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter {
        code: BPF_JMP_JEQ_K,
        jt,
        jf,
        k: constant,
    }
}

/// `BPF_RET | BPF_K` with action `k`.
#[cfg(target_arch = "x86_64")]
fn ret(k: libc::c_uint) -> libc::sock_filter {
    libc::sock_filter {
        code: BPF_RET_K,
        jt: 0,
        jf: 0,
        k,
    }
}

#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    use super::*;

    /// Run `program` over one syscall event the way the kernel would: `arch`
    /// fixed to this platform, `arg0` the first argument, and everything else
    /// zero.
    fn run(program: &[libc::sock_filter], nr: libc::c_long, arg0: u32) -> u32 {
        run_as(program, AUDIT_ARCH_X86_64, nr, arg0)
    }

    /// Run `program` over one syscall event, reading `arch` from the caller so
    /// the architecture gate can be exercised. Returns the action word the
    /// program ends on.
    fn run_as(program: &[libc::sock_filter], arch: u32, nr: libc::c_long, arg0: u32) -> u32 {
        let word = |offset: u32| -> u32 {
            match offset {
                SECCOMP_NR_OFFSET => syscall_word(nr),
                SECCOMP_ARCH_OFFSET => arch,
                SECCOMP_ARG0_OFFSET => arg0,
                _ => 0,
            }
        };
        let mut accumulator = 0u32;
        let mut pc = 0usize;
        while let Some(instruction) = program.get(pc) {
            match instruction.code {
                BPF_LD_W_ABS => accumulator = word(instruction.k),
                BPF_JMP_JEQ_K => {
                    pc = pc.saturating_add(usize::from(if accumulator == instruction.k {
                        instruction.jt
                    } else {
                        instruction.jf
                    }));
                }
                BPF_RET_K => return instruction.k,
                other => unreachable!("the program builder emits one of three opcodes: {other}"),
            }
            pc = pc.saturating_add(1);
        }
        unreachable!("the program always terminates in a RET")
    }

    /// The deny action: the one that makes a forbidden syscall a `SIGSYS`
    /// death for the whole child.
    const KILL: u32 = libc::SECCOMP_RET_KILL_PROCESS;

    #[test]
    fn the_allowlist_allows_its_entries() {
        let program = build_program();
        for &(number, name) in ALLOWED_SYSCALLS {
            assert_eq!(
                run(&program, number, 0),
                libc::SECCOMP_RET_ALLOW,
                "{name} is in the allowlist but the program denies it"
            );
        }
    }

    #[test]
    fn a_syscall_outside_the_list_is_killed() {
        let program = build_program();
        // `ptrace` is absent from the list, and nothing a build action has
        // asked for.
        assert_eq!(run(&program, libc::SYS_ptrace, 0), KILL);
    }

    #[test]
    fn an_argument_filtered_syscall_is_allowed_only_for_its_argument() {
        let program = build_program();
        for entry in ARGUMENT_FILTERED_SYSCALLS {
            assert_eq!(
                run(&program, entry.number, entry.argument),
                libc::SECCOMP_RET_ALLOW,
                "{} denies the argument it is filtered to",
                entry.name
            );
            // Every allowlist number is swept as an argument value, because a
            // block that fell through to the next comparison with the argument
            // word still in the accumulator would admit one of them as that
            // syscall. Three hand-picked domains do not reach that.
            for &(other, other_name) in ALLOWED_SYSCALLS {
                let word = syscall_word(other);
                if word == entry.argument {
                    continue;
                }
                assert_eq!(
                    run(&program, entry.number, word),
                    entry.refusal,
                    "{} mishandled the argument {word} ({other_name}'s number)",
                    entry.name
                );
            }
        }
    }

    #[test]
    fn a_socket_outside_af_unix_dies_and_an_unnamed_prctl_gets_eperm() {
        // The calls each refusal action was chosen for: reaching for the
        // network is a contract violation, and `PR_SET_MM` is a privileged
        // probe whose caller already handles failure.
        let program = build_program();
        assert_eq!(
            run(&program, libc::SYS_socket, libc::AF_INET as u32),
            KILL,
            "an AF_INET socket must not be refused quietly"
        );
        assert_eq!(
            run(&program, libc::SYS_prctl, libc::PR_SET_MM as u32),
            errno_action(libc::EPERM)
        );
    }

    #[test]
    fn a_call_from_another_architecture_is_killed() {
        let program = build_program();
        // The gate reads `arch` before the number, so `read` dies here despite
        // sitting first in the allowlist.
        let other_arch = AUDIT_ARCH_X86_64.wrapping_add(1);
        assert_eq!(run_as(&program, other_arch, libc::SYS_read, 0), KILL);
    }

    #[test]
    fn the_list_names_each_syscall_once() {
        // A duplicate wastes instructions and hides a copied line; the kernel
        // would run the program either way.
        let mut numbers: Vec<libc::c_long> = ALLOWED_SYSCALLS
            .iter()
            .map(|&(n, _)| n)
            .chain(ARGUMENT_FILTERED_SYSCALLS.iter().map(|e| e.number))
            .collect();
        numbers.sort_unstable();
        let count = numbers.len();
        numbers.dedup();
        assert_eq!(
            numbers.len(),
            count,
            "the allowlist repeats a syscall number"
        );
    }

    #[test]
    fn the_program_fits_the_kernel_u16_instruction_limit() {
        let program = build_program();
        assert!(
            u16::try_from(program.len()).is_ok(),
            "the program is {} instructions; the kernel's limit is u16::MAX",
            program.len()
        );
    }
}
