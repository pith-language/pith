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

status: complete.

evidence: the rust prototype has a typed pure step machine, exact-interface rule selection over the current interface subset, cycle diagnostics, dependency recording, instance-owned graph identities, exact in-memory reuse for completed pure applications, structured diagnostics, and an in-memory query surface. it constructs versioned, domain-separated pure-computation keys over an explicitly provisional rule identity. action capability requirements propagate canonically through completed pure dependencies, and actual capability use is retained as dependency edges and exposed through the query interface.

durable rule-revision identity, persistent graph and cache storage, equality-based change pruning, and invalidation explanations belong to the incremental and caching work in M-2. operational support for `Observation`, `Mutation`, and `Opaque` belongs to the milestones that exercise those effects. neither is required to reopen this semantic prototype milestone.

## M-2: action prototype

add content-addressed blobs and trees, sandboxed local actions, caching, and explanations for cache hits and invalidation.

current evidence: blobs and trees are content addressed, with tree identity preserving file executability and symlink targets; pure steps can request deferred blob materialization; and action planning now produces a validated, inspectable contract with a stable digest. every scheduler run supplies an action policy, named allow and deny decisions are retained in provenance, and denial prevents executor invocation. the engine materializes only declared executable and input content for an executor, imports captured output bytes and trees into the engine store, and gives action rules engine-owned content identities. executors still return structured reports, and the engine retains successful and rejected reports in provenance. action caching is disabled until its identity includes durable rule-revision semantics, the resolved platform, and relevant policy. a sandboxed local executor, persistent graph and cache storage, equality-based change pruning, invalidation explanations, concurrency, and cancellation remain.

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
