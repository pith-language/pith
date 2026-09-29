---
schema: design-doc/v1
id: decision-0075-components-run-in-a-confined-worker
title: components run in a separate worker process confined like an action, one component per worker, on tier 1 Wasmtime configurations only
summary: the wasm runtime never runs in the engine's process; each component gets its own worker confined with Landlock and a measured seccomp allowlist and no network; the worker speaks pith's step protocol over a pipe; only x86_64 linux with Cranelift and spectre mitigations on a patched long-term Wasmtime runs third-party code; precompiled artifacts are produced in a confined worker, loaded only for the same component, and never fetched
kind: decision
status: proposed
created: 2026-09-29
updated: 2026-09-29
tags:
  - wasm
  - security
  - executors
relations:
  informed_by:
    - research-wasm-components
  depends_on:
    - requirements-security-and-trust
    - decision-0028-sandboxed-local-executor
    - decision-0071-host-bodies-bind-through-host-adapters
    - decision-0074-the-component-step-protocol-is-wasi-async
  supersedes: []
---

# components run in a separate worker process confined like an action, one component per worker, on tier 1 Wasmtime configurations only

## context

components can come from authors the user does not know, and pith caches and replays what they return.
the wasm sandbox is the first containment, and [the research note](../research/wasm-components.md)
records its limits.

Wasmtime's security policy covers only tier 1 platforms and features. component-model async, which 0074
uses, is in no tier. aarch64 linux and Pulley are tier 2. on 2026-04-09 Wasmtime published twelve
advisories, including an aarch64 Cranelift miscompile allowing host reads and writes and a sandbox escape
in Winch, and on 2026-09-24 it fixed a bug that let a guest discard spent fuel. precompiled artifacts are
loaded with an `unsafe` call because a crafted one "can trivially be used to execute arbitrary code".

## decision

### the runtime runs in a worker

guests run in a separate worker process, never in the engine's process. the worker is confined with the
mechanisms 0028 uses for actions: a Landlock ruleset with no filesystem access, a seccomp allowlist
measured from Wasmtime's own system calls as 0028 measured compilers, and no network.

the engine sends the worker the component bytes or a precompiled artifact. the two communicate over a
pipe using the step vocabulary and the canonical value encoding. the protocol does not depend on wasm, so
a later adapter can run inside the same worker.

### one component per worker

each worker serves one component, and two domains never share a worker. code that escapes the wasm
sandbox is left in a process holding only that component's inputs, with no files, no network and no
authority.

### tier 1 configurations only

the worker runs third-party code only on x86_64 linux, with Cranelift, spectre mitigations on, and a
supported long-term Wasmtime release on its latest patch. Pulley is tier 2. Winch is tier 1, but it had a
sandbox escape in april 2026 and lacks the verified lowering rules Cranelift has. neither is used for
third-party code.

component-model async is used although it has no tier, and Wasmtime does not treat its bugs as
vulnerabilities. pith relies on the worker boundary to contain them.

### precompiled artifacts

precompiled artifacts are produced in a confined worker and loaded only by a worker serving the same
component. a compiler bug that produced a malicious artifact would run with the authority the component
already had. artifacts are keyed on the component's content identity and Wasmtime's
`precompile_compatibility_hash`, written only by the local pith process, and never fetched from another
machine.

## alternatives considered

running Wasmtime in the engine's process is faster and simpler, but it leaves only untiered runtime code
between third-party components and the engine's store, credentials and files.

an opt-in in-process mode for trusted components would be a setting that weakens a check, which the
principles rule out.

one worker shared by all components would cost less, but a component that escaped the sandbox could read
other domains' inputs.

compiling in the engine's process would expose it to compiler bugs. Wasmtime counts memory unsafety at
compile time as a vulnerability, so its threat model includes such bugs.

## evidence

- the round-trip cost of one step across the worker boundary is measured on the witness, with a target
  stated before measuring
- a worker making a system call outside its allowlist is killed by seccomp, tested with a native test
  binary run in worker mode
- the witness runs with the worker holding no file descriptors other than its pipe and, where used, the
  precompiled artifact

## unresolved

the seccomp allowlist for Wasmtime has to be measured, including the signal handling Wasmtime uses for
traps.

whether workers are reused across computations of the same component, or started per run, depends on the
measured startup cost. reuse must not carry guest state between computations (0076).

platforms other than x86_64 linux are out of scope, following 0006.
