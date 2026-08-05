---
schema: design-doc/v1
id: planning-milestones
title: milestones
summary: a provisional implementation order for proving the design with one complete vertical slice
kind: planning
status: draft
created: 2026-03-23
updated: 2026-08-18
tags:
  - planning
  - milestones
relations:
  informed_by:
    - foundation-scope
  depends_on:
    - planning-open-questions
  supersedes: []
---

# milestones

this order is provisional. the first implementation should test the kernel through real domain libraries instead of polishing syntax around an unproven engine.

## M-1: semantic prototype

define typed values, requests, rules, dependency recording, structured diagnostics, and an in-memory query interface.

current evidence: the rust prototype has a typed pure step machine, exact-interface rule selection, cycle diagnostics, dependency recording, instance-owned graph identities, exact in-memory reuse for completed pure applications, and an in-memory query surface. action capability requirements propagate canonically through completed pure dependencies and are queryable from computation provenance. persistent computation identity, invalidation, capability-use edges, and propagation for the remaining effect categories still need to prove the incremental contract before this milestone is complete.

## M-2: action prototype

add content-addressed blobs and trees, sandboxed local actions, caching, and explanations for cache hits and invalidation.

current evidence: blobs and trees are content addressed; pure steps can request deferred blob materialization; and action planning now produces a validated, inspectable contract with a stable digest. every scheduler run supplies an action policy, named allow and deny decisions are retained in provenance, and denial prevents executor invocation. executors return captured outputs and structured reports, action rules derive typed results, and the engine retains successful and rejected reports in provenance. action caching is disabled until its identity includes rule semantics, the resolved platform, and policy. a sandboxed local executor, persistent caching, invalidation explanations, concurrency, and cancellation remain.

## M-3: first build library

build a small project with at least two toolchains, generated input, tests, and fine-grained rebuilds. all build concepts use public library APIs.

## M-4: package and environment libraries

add package identity, basic constraints, lock data, binary reuse, and a reproducible development environment.

## M-5: Linux system library

compose files, users, a service, and boot configuration into an immutable Linux artifact.

## M-6: deployment library

observe one machine, derive a plan, apply it, confirm the result, and return to an earlier realization. secrets use references resolved at the target.

## M-7: broader execution

only after the vertical slice works: remote execution, additional operating systems, multi-machine placement, continuous reconciliation, and richer transition protocols.
