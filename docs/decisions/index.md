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
- [0028: a first-party sandboxed local executor using landlock and seccomp](0028-sandboxed-local-executor.md)
- [0033: a consumer of an action revalidates by re-planning it](0033-consumer-of-action-reuse.md)

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
- [0029: independence is declared by the rule body, not inferred by the scheduler](0029-declared-independence.md)
- [0030: a toolchain enters an action as a declared closure of host paths](0030-toolchain-closure-as-declared-input.md)
- [0031: an action is identified by its request and admitted by its execution facts](0031-action-cache-identity.md)
- [0032: one action is one tool invocation, and a foreign build system is one opaque boundary](0032-action-granularity.md)
- [0034: header dependencies are discovered by an action and resolved at plan](0034-discovered-header-dependencies.md)
- [0035: a link is over a list of objects](0035-link-over-a-list-of-objects.md)
- [0036: a program the graph produced enters an action as content](0036-produced-program-as-content.md)
- [0037: a contract declares whether an exit status is a failure or a result](0037-exit-status-as-a-declared-outcome.md)
- [0038: rule bodies are data in one kernel-facing core ir, with host rules as a permanent declared tier](0038-represented-rule-bodies.md)
- [0039: separate package identity from version and realization identity](0039-package-identity.md)

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

note: 0033 completes the consumer half of 0031, which cached an action and kept the pure computation above it out of the index. 0031 stands; its key and its admission test are unchanged, and 0033 adds what a pure attempt has to record and re-derive before it may hold an action edge and still be reused. the first build library is what turned 0031's note into a measurement. it moved to accepted once the walk was built and the M-3 fixture asserted both halves: a second build reusing its root, and a fresh engine over the same durable state hydrating it.

note: 0030 amends 0028 by recording how a toolchain enters an action contract as a declared closure of host paths rather than a single executable blob. 0028 stands; the executable-as-blob model its "unresolved" section named as wrong is resolved here, and reading a nix store path's closure is recorded as the first prototype of the local content-store adapter over a Nix store that 0020 named.

note: 0034 exercises the license 0007 grants for discovered dependencies inside the mechanism 0007 prescribes: discovery runs as a tracked, cached action over a declared header universe, and the discovered set reaches the compile as a request input the planner resolves. 0007 stands; its "static inference" alternative's split — inference declares, hermetic execution fails loudly on a miss — is what the universe and the landlock layer respectively implement.

note: 0035 closes the fixed-arity-two item the link rule's doc comment carried since M-3 began, over the `List` constructor 0034 landed. 0026 stands; the element type keeps nominal identity inside the list, which is what preserves 0015's unambiguous selection now that the link input is no longer positional.

note: 0036 amends 0030 by covering the case 0030's argument does not reach. 0030 stands: a toolchain is not one file, so it enters a contract as a host path and a declared closure. a build product is one file the engine owns, and 0005's separation of content identity from external identity is what makes that a typed sum rather than a path whose meaning depends on its first character.

note: 0037 extends the contract 0003 makes visible to cover how an action ends. it exists because 0032 bars the wrapper that would otherwise answer the question: an action is one invocation of one tool, so what a nonzero exit means has to be declared rather than arranged around.

note: 0038 names the thing the design overview and 0010 call the "typed semantic representation" and that nothing had settled: one elaborated, canonicalizable core ir in which a rule body is data — its yield points are the `PureStep` protocol made explicit, and its suspension is a re-enterable (body, control point, environment) state rather than a host closure. host rules stay as a declared tier with 0023's conservative revisions; represented rules keep 0023's identity at the declaration site (module identity and name) and derive their revisions from a digest of the body. it is the mechanism 0033's unresolved section points at: a represented `plan()` excludes ambient state structurally, and declared state has only spellings the key or the revision covers, so the honesty 0033 trusts becomes enforceable. it sits behind the same gate as surface syntax, the 0026 calculus landing, and not behind surface syntax itself.

note: 0039 opens M-4 by giving 0005's semantic-identity slot its first domain construction: a package is an author-declared name in a domain, stable across version bumps, platform and toolchain changes, and domain-surviving source moves, broken only by rename. a package version adds the coordinates constraints and locks range over; a realization is identified by the computation and content identity the kernel already has, so the package library adds no identity machinery and stays a peer consumer of the build library (0009). a lock entry binds coordinates to source content identity the way flake.lock, go.sum, and Cargo.lock each do, and binary reuse is an admitted substitution over that binding rather than a 0031 cache hit. it is the record that measures the need for records and declared sums in `pith-core`, the way 0015 measured `Nominal` and 0034 measured `List`.
