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

## semantic prototype

define typed values, requests, rules, dependency recording, structured diagnostics, and an in-memory query interface.

## action prototype

add content-addressed blobs and trees, sandboxed local actions, caching, and explanations for cache hits and invalidation.

## first build library

build a small project with at least two toolchains, generated input, tests, and fine-grained rebuilds. all build concepts use public library APIs.

## package and environment libraries

add package identity, basic constraints, lock data, binary reuse, and a reproducible development environment.

## Linux system library

compose files, users, a service, and boot configuration into an immutable Linux artifact.

## deployment library

observe one machine, derive a plan, apply it, confirm the result, and return to an earlier realization. secrets use references resolved at the target.

## broader execution

only after the vertical slice works: remote execution, additional operating systems, multi-machine placement, continuous reconciliation, and richer transition protocols.

