---
schema: design-doc/v1
id: decision-0030-toolchain-closure-as-declared-input
title: a toolchain enters an action as a declared closure of host paths
summary: replace the single-blob executable with a host path the executor execves plus a declared closure of host paths the action may read, so landlock can confine the toolchain the action actually used rather than a convention
kind: decision
status: proposed
created: 2026-05-25
updated: 2026-05-27
tags:
  - actions
  - effects
  - sandboxing
  - toolchain
  - nix
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - decision-0003-explicit-effects
    - decision-0014-reproducibility-properties
    - decision-0020-nix-as-adapter-not-substrate
    - decision-0028-sandboxed-local-executor
  supersedes: []
---

# a toolchain enters an action as a declared closure of host paths

> amends [0028: a first-party sandboxed local executor using landlock and seccomp](0028-sandboxed-local-executor.md), whose "unresolved" section measured that `ActionSpec::executable` is one `ContentId` staged as one file and that a compiler is not one file. 0028 stands; the executable-as-blob model it named as wrong is resolved here. carries the [0020: reuse Nix infrastructure as adapters, not as the substrate](0020-nix-as-adapter-not-substrate.md) boundary into the action contract: reading a nix store path's closure is the first prototype of the local content-store adapter over a Nix store.

## context

0028's unresolved section already did the measurement. `ActionSpec::executable` names one blob and the executor stages it at `root/exe`. `cc` is a driver that execs `cc1` to compile and `as` to assemble, and it finds them through paths baked into its own binary at its build time, plus a search of `COMPILER_PATH` and `PATH` for the parts it does not bake in. the traced compile opened 103 distinct paths under a nix gcc and 18 under a distribution gcc, none of them declared, and it succeeded only because `sys_landlock` installed nothing. the honest way to run a toolchain under 0028's own claim is to stage its closure as declared inputs, which makes the toolchain a `Tree` the action depends on rather than a blob it points at. that is a change to the action contract, and this record is it.

the measurement was repeated for this record on a host whose `nix develop` shell provides gcc 15.2.0. a traced compile of one c source to one object opened 106 distinct paths across 13 `/nix/store/...` directories: the gcc wrapper, gcc itself, binutils, the binutils wrapper and lib, glibc and its dev and bin outputs, glibc-locales, bash, zlib, isl, libmpc, gmp, and mpfr. the gcc driver binary carries 21 baked-in `/nix/store` path references; `strings` finds them in the executable image. the driver locates `cc1`, the libc, the dynamic linker, and the fixed includes through those baked-in absolute paths. `COMPILER_PATH` and `PATH` redirect the search for sub-programs, not the fixed paths the driver already knows.

that observation is what fixes the shape of the decision. a toolchain cannot be relocated under a scratch-relative path without rebuilding the driver, because the driver opens its parts at absolute paths it compiled in. the two search-path environment variables that had to be declared alongside it (`COMPILER_PATH`, `PATH`, values asked of the driver via `-print-prog-name` because they differ per host) cover the parts the driver searches for, not the parts it does not search for because it already knows where they are.

## proposed decision

`ActionSpec::executable` stops being a `ContentId` and becomes a host path: a `Box<str>` the executor `execve`s. the executor no longer stages a blob for the executable and no longer reads one from the content store. the action contract no longer claims "the executable is these bytes"; it claims "the executable is at this path."

a second field, `toolchain: Box<[Box<str>]>`, carries the declared closure: the host filesystem paths the action may read to find the rest of its toolchain. for a nix toolchain these are the top-level `/nix/store/...` directories returned by `nix path-info -r` over the executable's store path. the executor adds one landlock `PathBeneath` rule per declared closure path, granting read and execute, alongside the rules that grant read, write, and create on the scratch root and on the declared output paths. the child can read its declared toolchain and write its declared outputs and nothing else outside the scratch root.

this makes the toolchain a dependency the action declares, with its full closure visible in the contract, and it makes the closure claim a kernel-enforced fact. landlock is the only instrument that tests it. an undeclared path the child tries to read is denied at the syscall boundary, and the compile fails there rather than silently succeeding against an unconfined host filesystem.

### how the closure is obtained

reading a nix store path's closure with `nix path-info -r` is the first prototype of 0020's local content-store adapter over a Nix store. each line is a store path; reducing each to its top-level `/nix/store/<hash>-<name>` directory gives the set of trees the action reads. the build library the M-3 milestone opens will own this discovery and pass the closure into the `ActionSpec`; the contract the kernel sees is just the list of paths.

### the 0020 boundary, stated so it is not relitigated

reading `nix path-info -r` is reading nix's own bookkeeping for which store paths a path pulls in. it is not pith trusting nix's content-addressing. pith hashes the actual bytes on import: when an action's captured output references a file the toolchain provided, or when the build library records a toolchain as a tree, pith computes the content identity from the bytes it reads, making them content-addressed by measurement regardless of how nix addressed them.

this record deliberately does not touch 0020's hard open questions. input-addressed versus content-addressed derivations, narinfo trust, and substitution are all about whether to trust a path nix offers before pith has read its bytes. here pith reads the bytes from a path the action's own author declared, the same way it reads any declared input, and assigns identity from what it read. the closure adapter is a read-and-measure interface over a Nix store; the trust questions belong to the remote-cache adapter 0020 names separately and 0024 leaves open.

### what `executable` means afterwards

a host path, not a content identity. the action contract distinguishes "the executable is at this path" from "the closure around it is these paths." the content identity of a toolchain is the closure's content, which the build library captures when it records a toolchain as a tree, not when an action declares one. an action that names a toolchain by path has declared where to find it and what it may read; it has not asserted the toolchain's bytes, because the bytes are the closure's bytes and the closure is large.

## alternatives considered

### copy the closure into the scratch root as a tree

walk each nix store path in the closure, content-address every file into pith's store as a `Tree`, and stage that tree into the scratch root reproducing the `/nix/store` layout. the most isolated option: the child sees only the scratch root and the closure is a pith content identity.

rejected because it does not work under landlock-only confinement. gcc opens absolute `/nix/store` paths baked into its binary, and those paths resolve against the host filesystem regardless of what is staged in the scratch root. without a way to make `/nix/store` itself resolve to the staged copy, the child reaches the host store on its first baked-in path and never sees the staged tree at all. making `/nix/store` resolve to a private copy needs a mount namespace and a bind mount, which needs a user namespace, which 0028 defers to a follow-up. the copy is correct work for the world that follow-up opens, and the `Tree` it produces is the content identity of the toolchain this record says the build library captures. it is not the mechanism by which the closure enters the action under landlock.

### keep `executable: ContentId`, add closure paths separately

leave the executable as one blob staged at `root/exe` and add `toolchain` as a second field. the smaller contract change.

rejected because it keeps the fiction 0028 named as wrong. the staged gcc blob is the driver, and the driver cannot find `cc1` without the closure. an executable that cannot run without a separate, parallel field of host paths is not really "one blob"; pretending it is leaves the staging code writing a file the driver then ignores in favour of its baked-in paths, and leaves `executable` describing a thing that is not the thing the action runs. making the executable a path and the closure its companion says what is actually true.

### bind-mount the closure into a private `/nix/store`

give the child a user namespace and a mount namespace, bind-mount each declared closure path under a private `/nix/store`, and hide the host store entirely. the strongest isolation.

rejected for this increment on the same ground 0028 rejected it as the default. it needs `CLONE_NEWUSER`, which works unprivileged on most modern kernels but is restricted on some and triggers noisy audit logs, or a setuid helper, which is a permanent privilege surface. landlock needs no privilege and no helper and covers the declared-path contract the engine already validates. the user-namespace design is the right follow-up for hiding the host store from a confined action, and this record names it as unresolved so it can be taken whole.

## consequences

the executor's staging code stops writing an executable blob and stops reading one from the content store. the scratch root holds only declared source inputs and declared outputs; the toolchain lives in the host filesystem at paths the action declared. an action's provenance now records the closure paths it ran against, which is the input 0014's reproducibility story needs to decide whether a result is worth verifying by rebuild: two runs that declared different closures ran against different toolchains even if their source inputs matched.

`AccessVerification::Observed` becomes the honest report for this increment. the local executor installs landlock (path confinement is a kernel fact) and does not yet install the seccomp allowlist (syscall confinement is still absent), so `Prevented`, which 0028 reserves for both layers, is not yet earned. `Observed` is exactly the level 0028 defines for "real but partial confinement," and reporting it is what keeps the `Prevented` claim honest for when seccomp lands.

the closure adapter is nix-specific today. `nix path-info -r` answers the closure question for a nix store path. a distribution gcc whose driver lives at `/usr/bin/cc` needs a different discovery mechanism, because its parts are not a nix closure: an `ldd` walk over the driver and its sub-programs, plus the driver's own `-print-search-dirs` and `-print-prog-name` answers, assemble an approximation. the approximation is a follow-up that discovers paths; it does not change the `ActionSpec` contract, which only carries them once discovered.

the action digest changes. encoding `executable` as a path string and `toolchain` as a sorted list of path strings alters `ActionSpecDigest` for every spec. schema and encoding versions stay at 1, because nothing is released and a break is cheaper to take now than to carry a migration path through.

## unresolved

the seccomp allowlist is the next increment. 0028's measurement of the 45-syscall list a real compile issues is drawn from the same trace that produced the 106-path measurement here. when seccomp lands, `AccessVerification` moves from `Observed` to `Prevented` for the local executor.

action caching is the increment after that. `finish_action` records every action `NotReusable(ActionCachingDisabled)` until its identity covers the resolved platform and complete execution semantics, and the closure the action declared is part of that identity. the `real_toolchain` test that asserts the second run re-executes flips when caching lands.

the closure adapter covers nix store paths and not yet distribution compilers. the discovery mechanism for a non-nix toolchain is a follow-up that does not change the contract. what discovery must not use is `ldd`: it picks a loader from `PATH` rather than the one named in the binary's own `PT_INTERP`, so inside a nix devshell it resolves a distribution binary against nix's glibc and reports libraries the binary will never open. asking the binary's own loader through `LD_TRACE_LOADED_OBJECTS` reports what the kernel will actually open, and is what the tests use where a store path is unavailable.

a confined action needs a writable temporary directory, and the executor provides it. the assembler in a compile creates temporaries and reaches for `/tmp`, which is outside the scratch root and therefore denied. the executor now creates `tmp` alongside `work` in the scratch root and exports `TMPDIR` naming it, so a tool that asks the ordinary way gets somewhere inside the sandbox to write.

this is the one variable the executor adds to an otherwise declared-only environment, and the reason it is not the ambient authority `env_clear` exists to prevent is that it names a directory the executor itself just created inside the action's own scratch root. it is the same kind of thing as the working directory: execution environment the executor constructs, not host state leaking in. a spec that declares its own `TMPDIR` still wins, because the declared environment is applied after.

the temporary directory is a sibling of the working directory rather than a subdirectory of it. declared input and output paths are relative and land under `work`, so a tool's temporaries cannot collide with a declared path, and capture, which reads only declared outputs under `work`, can never mistake one for an output.

`NetworkPolicy::AllowHosts` and `AllowAll` remain rejected by the local executor, on the same ground and with the same deferral 0028 records. the network-namespace design that would honor them is untouched by this record.

the host store is exposed to a confined action, because the closure paths are host paths. a child that reads an undeclared store path is denied by landlock, but a child that reads a declared one reads the real host file, which a malicious or buggy toolchain could have swapped since the action was planned. the user-namespace follow-up that bind-mounts a private copy of the closure closes this; until then the closure is a claim about which host paths the action may read, enforced, rather than a claim that those paths are isolated from the host.
