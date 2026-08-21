---
schema: design-doc/v1
id: decision-0028-sandboxed-local-executor
title: a first-party sandboxed local executor using landlock and seccomp
summary: ship the first real Executor for Linux x86_64, staging declared inputs into a scratch root and confining the child with landlock and a seccomp allowlist, so AccessVerification reports what was actually enforced rather than a convention
kind: decision
status: accepted
created: 2026-05-18
updated: 2026-08-21
tags:
  - actions
  - effects
  - sandboxing
  - linux
  - executor
relations:
  informed_by:
    - research-build-systems
    - research-artifacts-and-trust
  depends_on:
    - decision-0003-explicit-effects
    - decision-0006-linux-first
    - decision-0014-reproducibility-properties
    - decision-0016-implementation-language
    - decision-0022-sync-core-async-scheduler
  supersedes: []
---

# a first-party sandboxed local executor using landlock and seccomp

> amends [0016: implement the kernel in rust, graph by arena and index](0016-implementation-language.md), whose "unresolved" section names "the sandboxing approach" as needing a prototype. 0016 stands; the sandboxing approach it left open is recorded here. complements [0014: separate the reproducibility properties](0014-reproducibility-properties.md), whose claim "marking an action hermetic is not evidence that it was hermetic" is exactly the gap a real executor closes for the local-Linux case. carries the local enforcement side of [0003: model effects and capabilities explicitly](0003-explicit-effects.md) from a runtime convention into a measured fact.

## context

milestone M-2 (action prototype) lists "a sandboxed local executor" as the remaining item alongside invalidation explanations, concurrency, and cancellation. the engine side of the action lifecycle is built: planning produces a validated, digested [`ActionSpec`](../../crates/pith-core/src/action.rs); the engine materializes only declared inputs; the executor returns captured output bytes and the engine content-addresses them on import; authorization, capability checking, platform matching, and declared-output validation all run against whatever the executor reports. the durable publish path and the cross-process sqlite adapter are in place.

what does not exist is an executor. the `Executor` trait ([`crates/pith-engine/src/action.rs:27`](../../crates/pith-engine/src/action.rs)) and the `AccessVerification` enum (`Prevented` / `Observed` / `Unverified`, action.rs:101) are exercised only by nine test fakes across `action_and_blobs.rs` and `engine_state_wiring.rs`. every claim the engine makes about least authority, declared inputs, and capability control is therefore asserted against a mock and never against a process that could actually misbehave.

four earlier decisions already constrain the executor without it existing.

decision 0006 commits the project to Linux first, without putting Linux in the kernel. that licenses the first-party executor to use Linux facilities (landlock, seccomp, `clone`/`execve`) at the adapter layer, with no engine dependency on them.

decision 0016 sanctions `unsafe` for exactly this case. "unsafe is reserved for genuine foreign-function boundaries where the host cannot express the operation: sandbox setup, syscall interception, and similar primitives." the workspace lints `unsafe_code = "deny"` ([`Cargo.toml:66`](../../Cargo.toml)); 0016 is the record that says sandbox setup is the sanctioned exception, at a named, reviewable boundary.

decision 0014 separates reproducibility properties and is explicit that "an action marked hermetic without evidence of determinism discipline is a claim, not a measured fact." `AccessVerification` was designed to carry exactly that distinction. a real executor is what makes `Prevented` mean "the kernel confined this child" rather than "the executor promised it did."

decision 0003 makes authority visible. capabilities, declared inputs, declared outputs, and network policy are all in the contract. without an executor that enforces them they are documentation; with one they are the boundary the engine reasons about.

so the question is not whether to build an executor but what enforcement claim the first-party local executor makes, how honestly it reports that claim through `AccessVerification`, and where the `unsafe` lives.

## proposed decision

a first-party `pith-executor-local` crate implements `Executor` for Linux x86_64. it stages declared inputs and the executable into a private scratch root, fork/execs the executable with a minimal environment, confines the child with landlock and a seccomp allowlist, captures declared outputs after exit, and reports `AccessVerification` from what it actually installed.

### staging and capture

the executor is the authority that maps the engine's typed `ActionInvocation` to concrete filesystem layout. it creates a fresh per-action scratch directory under a configurable base, lays each declared input out at its declared relative path (writing blobs as files and trees recursively, preserving file executability and symlink targets from the materialized content the engine already resolved), and writes the executable blob with its executable bit set. after the child exits, the executor walks the declared outputs, reads each back from the scratch root, and builds the `CapturedOutput` / `CapturedTree` values the engine imports.

the executor receives fully materialized bytes, not content identities it has to resolve: the engine's `materialize_action` ([`crates/pith-engine/src/graph/action_pipeline.rs:275`](../../crates/pith-engine/src/graph/action_pipeline.rs)) reads from the content store and packs `MaterializedBlob { id, bytes }` before the executor is called. the executor therefore never touches the content store and never sees a content identity it could fail to resolve; outputs carry raw bytes and the engine content-addresses them on import (`import_output`, action_pipeline.rs:386). staging and capture are pure filesystem work and need no `unsafe`.

### least authority: landlock and seccomp

the child runs under two independent confinement layers, installed in that order between `fork` and `execve`.

landlock restricts filesystem access to the scratch root and to each declared output path, via `LandlockRuleType::PathBeneath`. the child can read its declared inputs and write its declared outputs and nothing else outside the scratch root. landlock requires no privilege; it is a voluntary restriction a process applies to itself, which is exactly the property a fork/exec parent wants.

seccomp installs a deny-by-default syscall allowlist covering the POSIX subset a build action needs: file descriptors (`read`, `write`, `openat`, `close`, `fstat`, `lseek`, `dup`, `dup2`, `fcntl`), memory (`mmap` anonymous, `mprotect`, `munmap`, `brk`), process exit (`exit`, `exit_group`, `rt_sigreturn`, `rt_sigaction`, `rt_sigprocmask`), and timing (`clock_gettime`, `nanosleep`). the allowlist is a named, documented constant in the crate, not an inline list. `socket`, `connect`, `accept`, `bind`, and the other network syscalls are not in the allowlist, so a spec with `NetworkPolicy::Deny` is enforced by absence: the child that tries to open a socket is killed.

the two layers compose. landlock says what paths the child may touch; seccomp says what syscalls it may issue. neither alone is complete: landlock does not restrict syscalls, and seccomp does not restrict paths. together they cover the declared-input / declared-output / declared-network contract the engine already validates.

### reporting AccessVerification honestly

the executor reports the verification level it actually installed, not the level it intended.

- `AccessVerification::Prevented` when both landlock and seccomp were installed. this is the level the engine trusts to mean "the child could not reach outside the contract."
- `AccessVerification::Observed` when the kernel lacks the landlock feature (detected at executor construction by probing `landlock_create_ruleset` and reading its availability) and the executor fell back to seccomp-only. the child is confined at the syscall boundary but not the path boundary, which is real but partial confinement; `Observed` is the honest report.
- `AccessVerification::Unverified` only when the caller explicitly opts in via an executor configuration flag. the default never produces `Unverified` for a first-party run, because producing it silently would collapse the distinction 0014 and 0003 put in the type system.

this is the load-bearing claim of the record. `AccessVerification` is not a label the executor chooses; it is a measurement of what was installed, and the executor reports it from the fingerprint taken at construction plus the result of each `landlock_restrict_self` / `seccomp` call.

### what the executor does not claim

no reproducibility claim beyond what 0014 allows. `Prevented` reports what was *enforced*, not that the action was deterministic. an action confined to a clean scratch root with no network can still read a timestamp from a source it legitimately read, embed a build-id derived from memory layout, or otherwise produce output that varies across runs. provenance records `Prevented` as the confinement fact; whether the result is reproducible is the separate, measurable property 0014 keeps distinct.

the first version does not honor `NetworkPolicy::AllowHosts` or `NetworkPolicy::AllowAll`. rather than silently relaxing the seccomp filter or half-implementing network namespaces, the executor returns `Err` with a clear diagnostic when a spec requests network access, so the declared contract is never quietly violated. lifting this restriction is a follow-up that needs a network-namespace design (it touches trust and transport, which 0024 leaves open for remote caches); it is named in "unresolved" below.

the platform string the executor reports is literal: `operating_system: "linux"`, `architecture: "x86_64"`, detected from `std::env::consts` at construction and validated against the spec's `PlatformRequirement` by the engine's existing `validate_execution_platform` ([`crates/pith-engine/src/graph/diagnostics.rs:253`](../../crates/pith-engine/src/graph/diagnostics.rs)). there is no normalization layer; the strings must match the declared platform character for character, which is the property that makes platform a cache axis.

### unsafe discipline

the crate root carries `#![deny(unsafe_code)]`. `unsafe` appears only in two named, commented modules:

- `sys_landlock.rs`, with `#![allow(unsafe_code)]` at the module head, each `unsafe` block justified by a `// SAFETY:` comment naming `landlock_create_ruleset(2)`, `landlock_add_rule(2)`, or `landlock_restrict_self(2)` as the foreign operation the block enables.
- `sys_seccomp.rs`, same shape, each `unsafe` block justified by a `// SAFETY:` comment naming `prctl(PR_SET_NO_NEW_PRIVS)` and `seccomp(SECCOMP_SET_MODE_FILTER, …)`.

this is the discipline 0016 describes: a reviewer can ask of any `unsafe` block what foreign operation it enables, and there is an answer. the workspace lints `undocumented_unsafe_blocks = "deny"` and `multiple_unsafe_ops_per_block = "deny"` ([`Cargo.toml:87-88`](../../Cargo.toml)) are in force inside both modules, so a missing `// SAFETY:` comment or a block that bundles two foreign operations is a compile failure, not a style note.

the crate uses `rustix` for the typed syscall surface wherever rustix exposes one (fork, execve, fcntl, the standard posix layer), so the `unsafe` count inside the two `sys_*` modules is exactly the count of operations rustix does not yet wrap: the landlock syscalls and the seccomp `prctl`/`seccomp` pair. raw `libc` is avoided unless rustix genuinely lacks the call.

## alternatives considered

### user namespaces and bind mounts

mount, pid, net, and user namespaces with read-only bind mounts of declared inputs and a private overlay for outputs. this is the strongest isolation in the kernel and the model Nix and Bazel's Linux sandbox use.

rejected for M-2 on setup cost. a no-new-privileges user namespace needs either `CLONE_NEWUSER` (which works unprivileged on most modern kernels but is restricted on some — Debian, older RHEL, locked-down CI runners — and triggers noisy audit logs), or a setuid helper binary, which is a permanent privilege surface and a packaging dependency. the landlock + seccomp combination needs no privilege and no helper, covers the same declared-path and declared-syscall contract the engine validates, and runs on the same kernel versions. namespaces are reserved as a follow-up for the cases landlock cannot cover (network egress for `AllowHosts`, pid isolation); they are not the default.

### bubblewrap or firejail as a subprocess

delegate confinement to an existing sandbox helper. bubblewrap is the canonical unprivileged sandbox; firejail is the configurable one.

rejected on the dependency and the claim. an external binary is a packaging dependency the engine cannot version, and the enforcement claim becomes "the helper did something correct," which is strictly weaker than "the kernel confined the child," because the helper's behavior is not visible to the engine or to provenance. 0003 makes authority visible; a binary that hides its ruleset in a config file makes authority invisible. the first-party `unsafe` boundary is smaller, auditable in one diff, and reports its own enforcement level through `AccessVerification`.

### process isolation with AccessVerification::Unverified

fork/exec into a scratch root with no syscall filter and no landlock, reporting `Unverified`. the cheapest option.

rejected as the default. it leaves `AccessVerification` meaningless for the first-party case (the whole point of the enum is to distinguish enforced from claimed), it defeats the authority-visible goal of 0003, and it makes the declared-network and declared-path contract a convention the executor happens to follow rather than a fact the kernel enforces. available as an explicit opt-in for trusted toolchains where sandbox setup fails or is unwanted, never as the default.

### seccomp only, no landlock

install the syscall allowlist and rely on it plus the scratch-root layout for path confinement.

considered and kept as the fallback (`Observed`), not the primary. seccomp confines syscalls but not paths: a child that can issue `openat` can open any path its uid can read. the scratch root prevents accidental writes outside the output directory but does not prevent reads outside it without landlock. landlock is the layer that makes declared-input confinement a kernel fact rather than a layout convention. the two together is the complete claim; seccomp alone is the documented fallback when the kernel predates landlock.

## consequences

the M-2 milestone gains its first non-test executor. every claim the engine makes about declared inputs, declared outputs, capability control, and network policy becomes testable against a process that could violate them, rather than against a mock that does not.

`AccessVerification::Prevented` acquires a concrete meaning for pith: "the local executor installed landlock and seccomp and the child ran confined." `Observed` means "the kernel lacked landlock; seccomp was installed; path confinement is partial." `Unverified` is the explicit opt-in for trusted toolchains. provenance carries whichever the executor measured, which is the input 0014's reproducibility story needs to decide whether a result is worth verifying by rebuild.

a new crate and two new dependencies enter the workspace: `rustix` (typed Linux syscalls, MIT/Apache, the standard way to do this in modern rust) and `tempfile` (scratch roots, MIT/Apache). both are widely used and add no runtime. the crate is `cfg(target_os = "linux")` at the root; building it on another platform is a no-op with a clear diagnostic, consistent with 0006.

the `unsafe` count in the workspace grows from zero to whatever the landlock and seccomp syscalls require beyond rustix's wrappers. the workspace `unsafe_code = "deny"` lint stays in force for every other crate; the executor crate denies it at the root and allows it only in the two named modules, each block commented and lint-checked. the safety story 0016 advertises is preserved: the `unsafe` is at a named ffi boundary with a foreign operation to point at.

the declared-network gap is honest. a spec that requests `NetworkPolicy::AllowHosts` or `AllowAll` fails fast at the executor with a diagnostic, rather than running under a relaxed seccomp filter that silently violates the declared contract. this keeps the contract honest until a network-namespace design lands.

## unresolved

both layers are now installed, and on Linux x86_64 `access_verification` reports `Prevented`. `sys_landlock` installs the path ruleset, `sys_seccomp` sets `no_new_privs` and loads the deny-by-default BPF allowlist, and the level still comes from what was installed, as this record requires: on an architecture the filter does not target, `seccomp_filter_installed` answers false and the honest report is `Observed`. the rule this record set is unchanged, and the test that asserted the partial state now asserts the full one alongside `a_forbidden_syscall_kills_the_child`, in which a script issues `kill(2)` — absent from the allowlist, and a shell builtin, so the script makes the call itself — and dies on `SIGSYS`. the same script exits zero unfiltered, which is what makes the kill evidence rather than a claim. this record moves to `accepted`: the confinement it exists to settle is measured.

the surface syntax for declaring that an action runs under the local executor, and for passing executor configuration (scratch base, `Unverified` opt-in), waits on the M-3 build library, on the same grounds 0026 defers all surface syntax to M-3. the `LocalExecutor` constructor is public API today; a declarative surface around it is not. this paragraph originally named timeout among the configuration a surface would pass, which was an error on both counts: the executor accepted no timeout, and the bound is the run's to declare, not the executor's to hold. both are corrected by [0059](0059-a-caller-declared-run-bound.md), which retracts the claim below.

`NetworkPolicy::AllowHosts` and `NetworkPolicy::AllowAll` are rejected by the first version. honoring them needs a network-namespace design (for egress filtering to a declared host set) or a decision to broaden the seccomp filter to the socket family and trust the child to police its own peers, neither of which belongs in this record. the design interacts with remote-cache trust (0024 unresolved) and with secret taint (open question), and is deferred to a follow-up that can take the trust-and-transport question whole.

the exact seccomp allowlist will need widening the first time a real toolchain (M-3) runs under the executor. the list in this record is the starting position; the discipline is that every addition is a named, commented entry justified by a concrete toolchain need, never an opaque broadening. the list lives in `sys_seccomp.rs` as a constant, not in configuration.

that widening has now been measured rather than estimated. `crates/pith-executor-local/tests/real_toolchain.rs` drives a C compiler through `Engine::run`, and the same compile traced under `strace` on a nix gcc 15.2.0 and a distribution gcc 13 issues 45 distinct syscalls between them, of which the list above names 15. the 30 it does not name are not a long tail. they include process creation — `clone3` / `vfork`, `execve`, `wait4`, `pipe2` — because the thing called `cc` is a driver that execs `cc1` to compile and `as` to assemble, so a filter without process creation kills a compile at its first real step. they include `newfstatat`, which is the single most frequent call of the run and the form glibc actually uses where the list says `fstat`. they include `readlink`, `access` / `faccessat2`, `getcwd`, `prlimit64`, `pread64`, `arch_prctl`, `set_tid_address`, `set_robust_list`, and `rseq`, which are ordinary glibc startup. in the other direction `clock_gettime` and `nanosleep` were never issued, and `dup` was not either. the starting position was drawn from a man page; this is the first list drawn from a compiler.

the seccomp filter is also the wrong instrument for the network claim. under this record `NetworkPolicy::Deny` is enforced by the absence of `socket`, and a real compile calls `socket` twice: glibc's name-service switch opens an `AF_UNIX` stream to `/var/run/nscd/socket` for a passwd lookup, gets `ENOENT`, and the compile continues. that is a local name lookup, not egress, so "issues no `socket` syscall" is a strictly stronger claim than "reaches no network" and a real toolchain violates the stronger one while honoring the weaker. distinguishing them needs the address family — permitting `AF_UNIX` while denying `AF_INET`/`AF_INET6`, which seccomp can express as an argument filter — or the network namespace this record already defers to. deciding which belongs with the network-namespace design named above.

that address-family question is now settled in the direction that paragraph named. `socket` is admitted only for `AF_UNIX`, as a seccomp argument filter over the low word of the first argument, which is the same truncation to `int` the kernel performs on the domain. `connect` is admitted outright, since no socket outside `AF_UNIX` survives to be connected. `NetworkPolicy::Deny` is therefore enforced as "no socket outside the local namespace" rather than "no socket", the weaker and true claim the traced compile already required. egress beyond the local socket stays with the network-namespace design.

the installed allowlist names 77 syscalls: 75 admitted outright and 2 admitted only for a particular first argument. the 45-syscall compile trace above is the floor. the rest came from running the fixtures and reading each kill: dash and the coreutils its scripts invoke add `getresuid` and `getresgid`, `uname`, `getpgrp`, `getrusage`, the plain `mkdir` / `unlink` / `chmod` forms alongside the `at` forms already named, and `fchdir` for a recursive walk. the link step added `sysinfo`, which is how gcc sizes its heuristics against host memory, and `chmod` on the binutils wrapper's output. each entry sits in the source under a comment naming what asked for it.

two entries record a method worth naming, because an allowlist entry is only tested by deleting it. `getresgid` went in as the obvious peer of `getresuid` with no measured call behind it. taking it out failed twelve tests: with `getresuid` admitted the shell proceeds to the group triple, and that second call had been masked by the kill on the first. `close_range` went in carrying a specific justification — that rust's `std::process` closes inherited descriptors in the child after the `pre_exec` hooks run, so a filter omitting it would kill every exec. deleting it failed nothing, and no trace on this host issues the call at all; the fork/exec path `pre_exec` forces does not close descriptors that way here. it came out. one entry survived its deletion and one did not, and the difference was not visible from the justification either carried.

the list was first written as `(number, name)` pairs transcribed by hand, and two numbers were wrong. `prlimit64` was recorded as 261, which on x86_64 is `futimesat`, so the filter admitted `futimesat` and killed the `prlimit64` every dynamically linked binary issues before `main`. `getrandom` was recorded as 355, which x86_64 leaves unassigned, so that entry admitted nothing at all. the failure mode is quiet: a wrong number compiles, denies the syscall its name claims, and grants whatever else holds the slot. worse, the kill arrives as `SIGSYS` in a child whose stderr the executor has already taken, and `strace` under `SECCOMP_RET_KILL_PROCESS` reports the death without naming the call. switching the default action to `SECCOMP_RET_TRAP` makes the kernel fill in `si_syscall`, which is how every entry above was identified. the numbers, the `seccomp_data` field offsets, the BPF opcodes, and the filter ABI constants now come from `libc`, with `offset_of!` deriving the offsets from the struct that defines them. that is `libc` entering the workspace on the terms this record set: rustix wraps calls, and a per-architecture number table is not a call.

rustix 1.1.4 does wrap the `prctl` this record's unsafe-discipline section pairs with `seccomp`. `rustix::thread::set_no_new_privs` is safe and issues the raw syscall, so that pair is now a single operation, and `sys_seccomp` holds two `unsafe` blocks: loading the filter with `seccomp(2)`, and registering the `pre_exec` hook that loads it.

the filter refuses in two ways, and the difference is deliberate. a `socket` outside `AF_UNIX` kills the process, because a build action reaching for the network is violating the declared contract and an errno would let it fall back and carry on quietly. an unnamed `prctl` operation returns `EPERM` instead. `prctl` is a multiplexer; the fixtures need one operation from it (`PR_SET_NAME`, which coreutils use to name themselves) and then ask for `PR_SET_MM`, which requires `CAP_SYS_RESOURCE` and already fails for an ordinary caller. killing a best-effort call whose failure the caller handles would make the sandbox stricter than the kernel it stands in for. so deny-by-default is a kill, and an errno appears only where the kernel itself would have refused.

the argument-filter encoding carried a bug whose unit test passed by accident, which is worth recording as a shape to watch for. a filtered entry loads the first argument into the BPF accumulator, and the first version let a failed argument comparison fall through to the next syscall comparison with that argument word still loaded, so an argument value equal to a later syscall number would have been admitted as that syscall. `socket` sat second to last in the list, and neither `AF_INET` nor `AF_INET6` equals the one number following it, so three hand-picked domains all gave the right answer. each filtered entry now reloads the syscall number at its own start, and the test sweeps every allowlist number as an argument value instead of picking three.

a second compiler widened it again, which is this record's own prediction holding. clang 21.1.8 needs `sigaltstack` for the alternate stack its crash handler runs on, `rename` because it writes output to a temporary and moves it into place, and `alarm` for a watchdog around its own work. gcc issued none of the three. the discipline held in the direction that matters: each entry came from a kill, and the widening was three syscalls rather than a category.

the list is measured on one host: dash as `/bin/sh`, the distribution coreutils, a nix gcc 15.2.0, a nix clang 21.1.8, and binutils 2.46, kernel 6.17. `chdir` is not in it, so an action whose script does `cd` dies, and the discipline here says that entry waits for a concrete need, and plausibility is not one. a second host, a second libc, or a compiler that spawns differently will find more, and the miss will read as `SIGSYS` rather than as a diagnostic. that is the standing cost of deny-by-default, and the reason the executor's stderr excerpt is the first thing to read when a previously working action dies without output.

the executable-as-a-blob model does not describe a toolchain. `ActionSpec::executable` is one `ContentId`, staged as one file at `root/exe`, and a compiler is not one file: the traced compile opened 103 distinct paths under nix and 18 under the distribution compiler, and every one of them — `cc1`, `as`, the shared libraries behind both, the fixed includes, `/etc/passwd`, `/dev/tty` — is outside the declared contract. it works today only because `sys_landlock` installs nothing, so the child can still read the host filesystem; the first landlock ruleset confined to the scratch root turns every one of those reads into the failure that ends the compile. the honest way to run a toolchain under this record's own claim is to stage its closure as declared inputs, which makes the toolchain a `Tree` the action depends on rather than a blob it points at. that is a change to the action contract, not to the executor, and it belongs to M-3's first build library. two search-path environment variables (`COMPILER_PATH`, `PATH`) also had to be declared for the driver to find its own parts, with values asked of the driver via `-print-prog-name` because they differ per host; a build library that hardcoded them would work on exactly one machine.

timeout and resource limits (cpu, wall, address space, file descriptors) are not in this record's scope. an earlier version of this paragraph claimed "the executor accepts an optional timeout in its configuration," which was never true — no timeout, deadline, or `Duration` existed anywhere under `crates/*/src` — and [0059](0059-a-caller-declared-run-bound.md) retracts it: the run declares the bound, the deadline descends into the invocation, and the local executor kills the child at it and refuses with the bound's code. full rlimit / cgroup enforcement remains a follow-up that composes on that same invocation seam.

whether the executor belongs as its own crate (`pith-executor-local`) or as a module behind a cargo feature inside `pith-engine` is left open until the first build library clarifies how many executors ship together. the crate-per-executor default matches the adapter pattern 0024 set for content and state stores; a feature gate is the fallback if packaging forces it.
