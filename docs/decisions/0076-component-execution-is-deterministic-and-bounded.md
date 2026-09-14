---
schema: design-doc/v1
id: decision-0076-component-execution-is-deterministic-and-bounded
title: component execution follows the deterministic profile in a fresh store per computation, fuel is its bound, and exhausting a bound is a host fault that is never cached
summary: NaN canonicalization, deterministic relaxed SIMD, no threads and no imports beyond the generated world; a fresh store and instance for every computation; fuel as the execution bound with per-store memory, stack and host-call limits and an epoch deadline tied to the run bound; running out is a host fault and not a result; the Wasmtime version is recorded in provenance and kept out of revisions, with a verification query that re-runs on a second backend
kind: decision
status: proposed
created: 2026-09-29
updated: 2026-09-29
tags:
  - wasm
  - determinism
  - caching
relations:
  informed_by:
    - research-wasm-components
  depends_on:
    - decision-0023-rule-and-cache-identity
    - decision-0059-a-caller-declared-run-bound
    - decision-0071-host-bodies-bind-through-host-adapters
    - decision-0074-the-component-step-protocol-is-wasi-async
    - decision-0075-components-run-in-a-confined-worker
  supersedes: []
---

# component execution follows the deterministic profile in a fresh store per computation, fuel is its bound, and exhausting a bound is a host fault that is never cached

## context

pith caches a pure rule's result under its computation key and replays it. a component's result is safe
to cache only if it depends on nothing but the rule's inputs and answers.

[the research note](../research/wasm-components.md) records the relevant facts. the WebAssembly 3.0
specification defines a deterministic profile covering NaN bit patterns and relaxed SIMD. threads, stack
exhaustion and host imports remain sources of nondeterminism. Typst reuses plugin instances and cannot
detect a plugin that keeps state between calls. Wasmtime documents fuel as deterministic, but its fuel
accounting changed in versions 46 and 49. and Wasmtime does not count a wrong result that stays inside
the sandbox as a vulnerability.

## decision

### the deterministic profile

the worker's engine follows the deterministic profile: NaN canonicalization on, relaxed SIMD
deterministic, threads and shared memory off. a component imports only what its generated world defines
(0073), and features whose results are nondeterministic, `error-context` and threading built-ins, are
refused at load.

### a fresh store per computation

each computation runs in a fresh store and a fresh instance. no state carries over from one computation to
the next, including when a worker (0075) serves several computations of the same component.

### bounds

fuel is the execution bound. memory, table size, stack depth, host-call copying and the resource table are
bounded per store. an epoch deadline runs alongside fuel as a wall-clock limit and is tied to 0059's run
bound.

### running out is a fault

exhausting any bound is a host fault. it is reported and never cached: the same body and input can
finish under one Wasmtime release and run out under another. a declared failure returned by the body is a
value and is cached like any other.

### the runtime version is provenance

the Wasmtime version and configuration are recorded in each result's provenance and left out of the
rule's revision. including them would invalidate every component result on every monthly release.

leaving them out means a miscompile that stays inside the sandbox can put a wrong result in the cache. a
verification query re-runs a computation's host bodies on a second backend, in the same kind of worker,
and compares the results. it runs only when asked for and does not affect cache keys.

## alternatives considered

epoch interruption alone is faster, up to two or three times by Wasmtime's measurements, and is driven by
a timer, so whether a body finishes would depend on machine load.

caching a fuel exhaustion as a failure would make a result depend on the Wasmtime release that produced
it.

reusing instances between computations of one component would save instantiation time and allow state to
leak between computations, as it can in Typst.

## evidence

- the cost of instantiating a component per computation is measured on the witness, with a target stated
  before measuring. the only published figure, 5 microseconds for one core module in 2022, does not cover
  components
- a component that runs out of fuel, memory or stack produces a host fault and leaves no cached result
- a component that writes global state in one computation cannot observe it in the next
- the verification query reports a mismatch when one backend is made to return a different value

## unresolved

there is no default fuel budget. Shopify's instruction limit is its users' most frequent complaint, and
pith's default should come from the witness's measured use.

the second backend for verification is not chosen. Pulley is the obvious candidate and is tier 2, which is
acceptable for a check that runs inside the worker and only compares.
