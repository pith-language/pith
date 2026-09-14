---
schema: design-doc/v1
id: decision-0074-the-component-step-protocol-is-wasi-async
title: a component body requests computations by awaiting WASI 0.3 async imports, and requests issued before any is answered form a batch
summary: a pure host body is an async function that awaits typed request imports; the adapter runs the guest until every task waits, yields the requests issued meanwhile as one step or batch in issue order, and delivers all answers in issue order before the guest runs again; the host never re-enters a guest mid-step; a synchronous frame resource is the fallback if async fails fuzzing
kind: decision
status: proposed
created: 2026-09-29
updated: 2026-09-29
tags:
  - wasm
  - graph
  - determinism
relations:
  informed_by:
    - research-wasm-components
  depends_on:
    - decision-0029-declared-independence
    - decision-0062-the-ir-constructor-set
    - decision-0071-host-bodies-bind-through-host-adapters
    - decision-0073-the-component-interface-is-generated-from-declarations
  supersedes: []
---

# a component body requests computations by awaiting WASI 0.3 async imports, and requests issued before any is answered form a batch

## context

a host pure rule in the engine is a `PureRuleFrame` whose `step` takes the previous answer and returns the
next `PureStep` (`crates/pith-engine/src/graph/ir.rs`). a component body has to fit that protocol.

WASI 0.3, released on 2026-06-11, added async functions and futures. Wasmtime runs async calls on a
separate stack and suspends the guest when an async host import returns a pending future. Wasmtime 46
enabled component-model async by default, but it is not listed in any of Wasmtime's support tiers.

## decision

### a body awaits its requests

a pure host body is exported as an `async func`. it requests a computation by calling the typed import for
that interface (0073) and awaiting the result, and reads a blob by calling `read-blob`, which returns a
`stream<u8>`.

### concurrent requests are a batch

requests that a body issues before any of them has been answered cannot depend on each other's results,
so they are treated as a declared batch in the sense of 0029.

the adapter runs the guest until all of its tasks are waiting, collects the requests issued in that time
in issue order, and yields them to the engine: `Need` for one request, `NeedAll` for several pure
requests. requests of mixed kinds are yielded as consecutive steps in issue order, which gives up
parallelism and does not change the result.

### answers are delivered in a fixed order

when the engine resumes, all answers of the batch are delivered in issue order before the guest runs
again. what the guest observes therefore depends only on its inputs and not on the order in which the
engine finished the requests.

### no re-entrance

the adapter never calls into a guest that has a step in progress. the component model stopped trapping on
re-entrance on 2026-08-28, so pith enforces this itself.

### action rules are synchronous

`plan` and `complete` compute a value from their inputs and make no requests, so they are plain
functions.

## alternatives considered

a synchronous `frame` resource with a `step` method maps directly onto `PureRuleFrame` and needs only WASI
0.2, whose component model is tier 1 in Wasmtime. guest authors would have to write each body as a state
machine and build batches by hand. it is the fallback if the async protocol fails the fuzzing below. if
adopted, it replaces the async protocol; the two are not offered side by side.

delivering answers as they complete would let a guest start work earlier. it would also make the guest's
execution depend on engine scheduling, which 0076's determinism rules out.

## evidence

- the witness's pure rules run through this protocol, and a rule that issues two requests before awaiting
  either produces one `NeedAll`
- the cost of a batch of requests is measured against the same requests issued one at a time
- a fuzzing harness drives the protocol with generated batches, cancellations and traps. Wasmtime's tiers
  do not cover component-model async, so this harness is pith's substitute for that coverage

## unresolved

cancellation of a suspended guest when the engine drops a computation needs a test of its own; Wasmtime
fixed a panic when dropping `call_async` futures in february 2026.

whether a batch should ever mix kinds in one engine step depends on the engine gaining a mixed batch.
