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

evidence: the rust prototype has a typed pure step machine, exact-interface rule selection over the current interface subset, cycle diagnostics, dependency recording, instance-owned graph identities, exact in-memory reuse for completed pure applications, structured diagnostics, and an in-memory query surface. it constructs versioned, domain-separated pure-computation keys over separate stable rule identity and cache-invalidating rule revision (0023). action capability requirements propagate canonically through completed pure dependencies, and actual capability use is retained as dependency edges and exposed through the query interface.

persistent graph and cache storage, equality-based change pruning, and invalidation explanations belong to the incremental and caching work in M-2. operational support for `Observation`, `Mutation`, and `Opaque` belongs to the milestones that exercise those effects. neither is required to reopen this semantic prototype milestone.

## M-2: action prototype

add content-addressed blobs and trees, sandboxed local actions, caching, and explanations for cache hits and invalidation.

current evidence: blobs and trees are content addressed, with tree identity preserving file executability and symlink targets; the engine accesses them through an injectable pith-owned content-store interface. the in-memory and filesystem adapters implement that interface. the filesystem adapter atomically publishes blob bytes and canonical tree manifests, verifies identity on reads, and preserves content across store instances. pure steps can request deferred blob materialization, and action planning produces a validated, inspectable contract with a stable digest. pure computation keys use the stable rule identity and cache-invalidating revision from decision 0023. every scheduler run supplies an action policy, named allow and deny decisions are retained in provenance, and denial prevents executor invocation. the engine materializes only declared executable and input content for an executor, imports captured output bytes and trees into the engine store, and gives action rules engine-owned content identities. executors still return structured reports, and the engine retains successful and rejected reports in provenance. action caching is disabled until its identity includes the resolved platform and complete execution semantics, with current policy reapplied on reuse. every computation that leaves `Pending` is published to a pith-owned engine-state interface, and a pure request that this engine instance has not evaluated is answered from the durable reusable index after its recorded dependency set revalidates: the completed attempt loads into a fresh arena node carrying the durable identity of the attempt it came from, so later computations depend on that attempt rather than a duplicate. equality-based change pruning exists for this path — a dependency recomputed to a canonically equal result leaves its consumers reusable — but not yet as a general propagation mechanism. a sqlite adapter stores that state as normalized relations, and a result computed by one process is hydrated by another after the writer has exited. the two adapters are held to each other by a generated conformance suite rather than by a shared encoding. reopening the database marks attempts left `Pending` by an interrupted owner as failed, so a reader finds a consistent graph rather than one waiting on a process that will not return. invalidation explanations are built as a chain over the recorded dependency graph: `EngineStateStore::explain_invalidation` walks the latest completed attempt's recorded reuse reason, following the single dependency it names, and both adapters share the walk through a common `AttemptLookup`-driven builder. the live arena exposes the same chain through `EngineQuery::explain_invalidation`, and the cross-adapter conformance suite compares the explanation for every pure key alongside the existing reuse and history reads. a first-party executor exists: `pith-executor-local` stages declared inputs and the executable into a per-action scratch root, runs the child with only the declared environment, captures declared outputs back from that root, and refuses a spec whose network policy or platform it cannot honor rather than running it under a weaker contract. its confinement is partial — the child gets `no_new_privs` but not yet the landlock ruleset or the seccomp allowlist decision 0028 describes — so it reports `AccessVerification::Unverified`, which is what it actually installed. independent work evaluates concurrently: a rule body declares that a batch of requests do not depend on one another with a single fan-out step, `run_many` drives one chain per root request, and the scheduler overlaps their actions while each chain stays on the synchronous step machine (0029). the overlap is measured rather than assumed — a test holds three actions at a barrier that only releases once all three have arrived. a run is cancellable: `run_cancellable` polls a caller-supplied signal at scheduling boundaries, and work stopped that way is recorded `Cancelled` — a terminal state distinct from `Failed` in the arena, in the durable record, and in the cross-adapter conformance suite — so a reader can tell a computation that cannot work from one that never got to run. the local executor kills its child when a cancelled action's future drops. that overlap is bounded: a chain that stops for an action queues the request and the driver starts it only while fewer than `Engine::action_concurrency` actions are running, so a fan-out of a thousand requests holds a thousand chains but plans, materializes, and spawns a bounded number of them. the landlock ruleset is installed (decision 0030 confines the scratch root, declared outputs, and declared toolchain closure); the seccomp allowlist remains, as do timeouts and cancelling part of a run rather than all of it.

## M-3: first build library

build a small project with at least two toolchains, generated input, tests, and fine-grained rebuilds. all build concepts use public library APIs.

current evidence: one toolchain runs, declared as a closure of host paths and confined by landlock. `crates/pith-executor-local/tests/real_toolchain.rs` compiles a C source file to an object file through `Engine::run` and the local executor — the engine plans the contract, materializes the source, the executor execves the declared compiler path with a landlock ruleset over the scratch root and the declared toolchain closure, captures the object, and content-addresses it, with no change to the engine for a tool it did not author. the toolchain enters the contract as a declared closure of host paths (decision 0030), obtained from `nix path-info -r` over the compiler's store path, which is the first prototype of the local content-store adapter over a Nix store that 0020 named. two runs of the same compile in different scratch roots produce byte-identical objects, so this compile is deterministic in the sense 0014 keeps separate from confinement.

the slice that first ran the compiler found three things, each of which changed what the build library has to be. **a toolchain is not an executable** — resolved by 0030: `ActionSpec::executable` is now a host path and a declared `toolchain` closure carries the paths the action may read, so landlock confines the toolchain the action actually used rather than a convention. **the seccomp allowlist in 0028 is a sixth of what a compiler needs**, and the missing part is structural rather than incidental — process creation, because the driver is a supervisor; the landlock ruleset is installed but the seccomp layer is not, so `AccessVerification` reports `Observed` (path confinement enforced, syscall confinement still absent) rather than `Prevented`. both layers' measurements are recorded in 0028's unresolved section. **fine-grained rebuild cannot be shown yet**: the second run of an unchanged compile runs the compiler again, because `finish_action` records every action `NotReusable(ActionCachingDisabled)`. the test asserts that re-execution, so it fails when action caching lands, which is the next thing this milestone needs.

## M-4: package and environment libraries

add package identity, basic constraints, lock data, binary reuse, and a reproducible development environment.

## M-5: Linux system library

compose files, users, a service, and boot configuration into an immutable Linux artifact.

## M-6: deployment library

observe one machine, derive a plan, apply it, confirm the result, and return to an earlier realization. secrets use references resolved at the target.

## M-7: broader execution

only after the vertical slice works: remote execution, additional operating systems, multi-machine placement, continuous reconciliation, and richer transition protocols.
