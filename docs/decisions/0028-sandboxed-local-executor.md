---
schema: design-doc/v1
id: decision-0028-sandboxed-local-executor
title: a first-party sandboxed local executor using landlock and seccomp
summary: ship the first real Executor for Linux x86_64, staging declared inputs into a scratch root and confining the child with landlock and a seccomp allowlist, so AccessVerification reports what was actually enforced rather than a convention
kind: decision
status: proposed
created: 2026-05-18
updated: 2026-06-19
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

the confinement described here is not yet fully installed. the crate exists and the staging, exec, and capture path runs real children, but `sys_seccomp` sets only `no_new_privs` and `sys_landlock` installs nothing, so `access_verification` reports `Unverified`. that is the honest reading of this record's own rule: `Prevented` requires both layers, and a build that installed neither must not claim either. the two modules are the seam the remaining work lands behind, and the `reports_unverified_until_both_layers_are_installed` test is written to flip when it does. this record stays `proposed` until the child is measurably confined, since that is the claim it exists to settle.

the surface syntax for declaring that an action runs under the local executor, and for passing executor configuration (scratch base, timeout, `Unverified` opt-in), waits on the M-3 build library, on the same grounds 0026 defers all surface syntax to M-3. the `LocalExecutor` constructor is public API today; a declarative surface around it is not.

`NetworkPolicy::AllowHosts` and `NetworkPolicy::AllowAll` are rejected by the first version. honoring them needs a network-namespace design (for egress filtering to a declared host set) or a decision to broaden the seccomp filter to the socket family and trust the child to police its own peers, neither of which belongs in this record. the design interacts with remote-cache trust (0024 unresolved) and with secret taint (open question), and is deferred to a follow-up that can take the trust-and-transport question whole.

the exact seccomp allowlist will need widening the first time a real toolchain (M-3) runs under the executor. the list in this record is the starting position; the discipline is that every addition is a named, commented entry justified by a concrete toolchain need, never an opaque broadening. the list lives in `sys_seccomp.rs` as a constant, not in configuration.

timeout and resource limits (cpu, wall, address space, file descriptors) are not in this record's scope. the executor accepts an optional timeout in its configuration; full rlimit / cgroup enforcement is a follow-up that composes on the same confinement seam this record establishes.

whether the executor belongs as its own crate (`pith-executor-local`) or as a module behind a cargo feature inside `pith-engine` is left open until the first build library clarifies how many executors ship together. the crate-per-executor default matches the adapter pattern 0024 set for content and state stores; a feature gate is the fallback if packaging forces it.
