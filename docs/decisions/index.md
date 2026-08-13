---
schema: design-doc/v1
id: decisions-index
title: decisions
summary: chronological record of accepted and proposed architectural choices
kind: decision
status: active
created: 2026-03-11
updated: 2026-08-18
tags:
  - decisions
relations:
  informed_by: []
  depends_on:
    - foundation-problem
  supersedes: []
---

# decisions

decision records preserve the alternatives and the reason for choosing among them. accepted records describe the current direction. proposed records contain a preferred direction that still needs research or a prototype.

when an accepted decision changes, a new record supersedes it. the old record stays in the repository.

## accepted

- [0001: use a generic semantic kernel](0001-generic-kernel.md)
- [0002: declarations denote values and constraints](0002-declarative-semantics.md)
- [0004: first-party without privilege](0004-first-party-without-privilege.md)
- [0008: research design lineages](0008-lineage-research.md)
- [0009: keep first-party domains as peers](0009-peer-first-party-domains.md)
- [0011: separate documentation by role](0011-document-structure.md)
- [0021: a hand-built arena graph with explicit change propagation, not a salsa query DB](0021-arena-graph-engine.md)

## proposed

- [0003: model effects and capabilities explicitly](0003-explicit-effects.md)
- [0005: separate identity types](0005-separate-identities.md)
- [0006: target Linux first without putting Linux in the kernel](0006-linux-first.md)
- [0007: allow tracked dynamic dependencies](0007-tracked-dynamic-dependencies.md)
- [0010: use a typed, pure, terminating declaration language](0010-typed-pure-language.md)
- [0012: revision-pinned plans](0012-revision-pinned-plans.md)
- [0013: managed-object identity](0013-managed-object-identity.md)
- [0014: separate the reproducibility properties](0014-reproducibility-properties.md)
- [0015: select rules by interface match and refuse ambiguity](0015-interface-rule-selection.md)
- [0016: implement the kernel in rust, graph by arena and index](0016-implementation-language.md)
- [0017: structural types by default, nominal by declaration](0017-structural-with-nominal.md) (superseded by [0026](0026-generic-typed-calculus.md))
- [0018: total pure evaluation by construction, with cycle detection and a backstop limit](0018-termination-and-recursion.md)
- [0019: five type-level effect categories, with nondeterminism as a tracked dependency](0019-effect-categories-and-nondeterminism.md)
- [0020: reuse Nix infrastructure as adapters, not as the substrate](0020-nix-as-adapter-not-substrate.md)
- [0022: a synchronous pure evaluator core with an async scheduler at the effect boundary](0022-sync-core-async-scheduler.md)
- [0023: separate stable rule identity from cache-invalidating rule revision](0023-rule-and-cache-identity.md)
- [0024: persist content in a filesystem cas and engine state in sqlite](0024-persistent-engine-state.md)
- [0025: store engine state as normalized relations, not as canonical record blobs](0025-relational-engine-state.md)
- [0026: a generic structural type calculus, with nominal identity, generic uncertainty, and no predicate types](0026-generic-typed-calculus.md)
- [0027: frame retention and garbage collection as roots, two stores, and composable policy axes](0027-retention-and-gc.md)
- [0028: a first-party sandboxed local executor using landlock and seccomp](0028-sandboxed-local-executor.md)
- [0029: independence is declared by the rule body, not inferred by the scheduler](0029-declared-independence.md)
- [0030: a toolchain enters an action as a declared closure of host paths](0030-toolchain-closure-as-declared-input.md)
- [0031: an action is identified by its request and admitted by its execution facts](0031-action-cache-identity.md)
- [0032: one action is one tool invocation, and a foreign build system is one opaque boundary](0032-action-granularity.md)

note: 0013 amends 0005 to add a fifth identity type. 0005 stands; the amendment is recorded in 0013.

note: 0022 refines 0019 by fixing where each effect category executes (synchronous step machine for `Pure`, async scheduler for the other four). 0019 stands; the refinement is recorded in 0022.

note: 0025 refines 0024 by fixing how an adapter represents the records 0024 defines. 0024's choice of storage substrates stands; the representation within them is recorded in 0025.

note: 0026 supersedes 0017 (its structural-default and nominal-by-declaration mechanism becomes one section of the larger calculus) and amends 0010 (settling the calculus questions among 0010's unresolved list). 0017 stays in the repository with a pointer; 0010 stands.

note: 0027 complements 0024 by framing the retention and GC problem 0024 left open. it does not implement GC; it defines the design space (roots, policy axes, cross-store ordering) a later workload-evidence record lands in.

note: 0028 amends 0016 by recording the sandboxing approach 0016 left in "unresolved." 0016 stands; its sanctioned `unsafe`-at-ffi-boundary discipline is what 0028's two `sys_*` modules implement.

note: 0029 refines 0022 by answering where the scheduler's concurrency comes from. 0022 stands; its synchronous core is unchanged, and 0029 records that the width a concurrent scheduler needs is declared by rule bodies rather than inferred.

note: 0031 completes the action half of 0023, which built a computation key for pure applications and left action results uncached. 0023 stands; its separation of stable identity from cache-invalidating revision is what the action key reuses, and 0031 adds only what an effectful computation needs on top of it.

note: 0021 moved from proposed to accepted once the five things its "prototype evidence" section named as unsettled all existed: durable rule-revision identity (0023), action cache identity (0031), persistent graph and cache storage (0024, 0025), invalidation after changed durable inputs, and cache explanations. its own unresolved items remain, which is what `accepted` allows — the direction is chosen, not every question closed.

note: 0032 introduces no mechanism. it states the granularity at which external work enters the graph, which the glossary, the rules-and-graph design doc, requirement U-5, 0019, and 0020 each imply separately and none states. 0019 and 0020 stand unchanged; 0032 is the record a build library can be started from.

note: 0030 amends 0028 by recording how a toolchain enters an action contract as a declared closure of host paths rather than a single executable blob. 0028 stands; the executable-as-blob model its "unresolved" section named as wrong is resolved here, and reading a nix store path's closure is recorded as the first prototype of the local content-store adapter over a Nix store that 0020 named.
