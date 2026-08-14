---
schema: design-doc/v1
id: planning-open-questions
title: open questions
summary: unresolved questions that can still change the architecture
kind: planning
status: active
created: 2026-03-23
updated: 2026-08-18
tags:
  - questions
  - research
relations:
  informed_by:
    - research-index
  depends_on:
    - design-kernel
  supersedes: []
---

# open questions

these are questions, not a disguised roadmap. each one can change the architecture enough that it needs research and a decision record.

where a question has a proposed decision against it, the decision is named in parentheses. the question stays open until the decision is accepted and prototyped.

## gating

several questions block milestones rather than sitting alongside the implementation work. the current gates:

- milestone M-5 (Linux system library) and M-6 (deployment library) are gated by decision 0012 (revision-pinned plans) and 0013 (managed-object identity)
- the reproducibility story in milestone M-3 (first build library) is gated by decision 0014
- surface language syntax is gated by the 0026 calculus landing in the core, not by a milestone. the older gate has been discharged: [0028](../decisions/0028-sandboxed-local-executor.md) deferred all surface syntax to the M-3 build library on the same grounds as [0026](../decisions/0026-generic-typed-calculus.md), and M-3 is complete — it discharged the deferral by building xylem as a rust library API, so no surface syntax exists yet. what stands in the way now is the calculus itself: `pith-core`'s `Type` carries the six scalars, `Nominal { name }`, `List`, `Record`, and `Sum` — the last two landed 0039-measured, ahead of the declaration site 0026 keeps deferring — while the parametric generics and the effect and uncertainty constructors 0026 specifies are not built, so a surface syntax would still have nothing to type. [0038](../decisions/0038-represented-rule-bodies.md) settles what the kernel consumes instead of syntax — rule bodies as data in one core ir — and sits behind the same calculus gate, but not behind surface syntax: the first frontend is the rust registration API over hand-built ir

milestone M-1 used decisions 0015 and 0019 as prototype hypotheses and is complete at its semantic-prototype scope. those decisions remain proposed: their broader interface and effect-category claims stay open until the later milestones exercise them.

## language and types

- should the language use structural or nominal types at module boundaries? (decision 0017 proposed structural default with opt-in nominal; superseded by [0026](../decisions/0026-generic-typed-calculus.md), which carries that mechanism into a full closed calculus with records, declared sums, generics, effect types, and uncertainty types. of that calculus only `Nominal`, `List`, `Record`, and `Sum` are built in `pith-core`, and surface syntax for it is gated on it landing — see gating)
- how much refinement typing can stay fast enough for editors? (settled by [0026](../decisions/0026-generic-typed-calculus.md): no predicate types in the language; validation is pure rules; the `Unchecked<T>`/`T` distinction is structural)
- should evaluation be total by construction, or can termination be checked with an explicit unsafe boundary? (decision 0018 proposes total by construction with cycle detection and a backstop limit)
- what is the smallest effect syntax that keeps capability use visible? ([0038](../decisions/0038-represented-rule-bodies.md) fixes the kernel-side half: the `PureStep` protocol — `Need`, `NeedAll`, `NeedBlob`, `NeedAction` — is the effect vocabulary of the core ir, as explicit constructs with binders for resumption values. what stays open is the surface spelling above it)
- how do typed values cross repository and version boundaries without freezing the type system too early?

## rule engine

- should rules be selected by output type, explicit name, capability interface, or a combination? (decision 0015 proposes interface match)
- how are multiple valid providers ranked without introducing invisible policy? (decision 0015 proposes refusing rather than ranking)
- what equality is used for change pruning when values contain opaque or external references? (canonical equality prunes across a pure edge and, since [0033](../decisions/0033-consumer-of-action-reuse.md), across an action edge; opaque and external references are untouched because neither category is operational yet)
- how much dynamic graph structure can be allowed while keeping queries useful before execution?
- which parts of the graph persist between daemon versions? ([0024](../decisions/0024-persistent-engine-state.md) persists attempts, edges, provenance, and the reusable index; [0027](../decisions/0027-retention-and-gc.md) frames what is retained and for how long)

## constraints

- is there one generic constraint representation with multiple solvers, or several domain-specific models with shared evidence?
- how are preferences separated from hard requirements?
- what makes a resolution explanation useful for versions, toolchains, and machine placement?
- how are locks represented when the valid result depends on several target platforms? ([0039](../decisions/0039-package-identity.md) fixes the entry's identity half: a package version bound to the content identity of its source, with origin as evidence and per-platform realizations derived rather than locked. whether a lock should ever pin realizations is left here, and the constraint and solver questions above are untouched)

## actions and effects

- are `Action`, `Observation`, and `Mutation` separate primitives or handlers of one effect calculus? (decision 0019 proposes five type-level categories — `Pure`, `Action`, `Observation`, `Mutation`, `Opaque` — with nondeterminism tracked as a dependency)
- should the synchronous pure step machine be unable to even name effectful steps at the type level? today `PureStep` carries `NeedBlob`/`NeedAction` variants that `evaluate_pure` rejects at runtime (`E-1206`); a separate pure-only step type would make that a compile property, which is what decision 0022's "structurally pure core" already claims
- which action properties can be enforced and which can only be claimed by an adapter? (decision 0014 addresses the reproducibility subcase; decision 0028 records the local-Linux enforcement claim — declared paths via landlock, declared syscalls via seccomp, `AccessVerification` reported from what was installed; decision 0030 resolves the executable-as-blob subcase 0028's "unresolved" section named by carrying the toolchain as a declared closure of host paths and making its confinement a kernel fact; the syscall side is now installed and measured, so `Prevented` reports two kernel-enforced layers and a child outside the allowlist is killed; the general enforcement question stays open, and the network subcase narrows to egress beyond the local `AF_UNIX` socket the filter admits by argument)
- when may a recorded effectful result stand in for running the effect again? ([0031](../decisions/0031-action-cache-identity.md) answers the action case: identity is the request, and the environment an attempt was recorded in is tested when it is considered for reuse. [0033](../decisions/0033-consumer-of-action-reuse.md), accepted and prototyped, answers the consumer case that matters for incrementality, by re-selecting and re-planning the recorded request at revalidation rather than carrying the action's identity into the consumer's key. what stays open is whether `plan()` is honest — 0033 relies on it depending on nothing but its inputs and the rule's own state, and nothing enforces that. [0034](../decisions/0034-discovered-header-dependencies.md) is the first record written under that constraint: header discovery runs as its own action and reaches the compile as a request input, so no planner reads a depfile off the filesystem. the constraint held by design there, and is still a convention the compiler cannot check; [0038](../decisions/0038-represented-rule-bodies.md) is the mechanism that would make it structural, since a represented planner evaluates as kernel data with no ambient access. [0036](../decisions/0036-produced-program-as-content.md) and [0037](../decisions/0037-exit-status-as-a-declared-outcome.md) put a produced program and a reported exit status inside the same discipline: the program is content named in the contract, and the verdict a rule derives from the status is the recorded result, so a failing test is a reusable value rather than a failed computation)
- how is secret taint tracked through values and diagnostics?
- how should retry safety and compensation be represented for mutations? (decision 0012 names the validity scope; retry-safety representation is open)
- can observations participate in incremental computation without making pure results time-dependent? (decision 0012 pins observation revisions; the incremental-purity interaction is open)

## kernel type system and content model

- is the `K` phantom type parameter on `Request<K>` / `Rule<K>` exploited enough? it prevents type confusion but the engine then re-derives pure-vs-action via runtime matches; leaning in further (distinct step types, distinct rule-id brands per effect) would move more checks to compile time
- should `RuleId` carry a per-effect brand so a pure `RuleId` cannot index the action body map? the brands exist (`define_arena!`) but `RuleId` is shared across both arenas today
- should `EvalFrame`'s `resume_with: Option<Value>` be a typed state (`Initial` vs `Resuming(Value)`) so "forgot to set the resume value" is unrepresentable?

## identity and state

- what gives a semantic object its identity when files move or modules are refactored? (decision 0013 introduces managed-object identity for the deployment case; [0039](../decisions/0039-package-identity.md) proposes the source-level answer for packages: an author-declared name in a domain, not a location or a digest. the general question for other declarations stays open)
- when is an external object adopted, replaced, or treated as unrelated? (decision 0013)
- where does the binding between semantic and external identity live? (decision 0013)
- how are ownership transfers made safe? (decision 0013 names the primitive; transfer safety is open)
- what is the retention and migration model for historical observations and plans? ([decision 0027](../decisions/0027-retention-and-gc.md) frames the problem: roots, policy axes, cross-store ordering. the default numeric parameters wait on workload evidence)

## first-party domains

- what is the minimal artifact interface needed by both build and deployment libraries?
- should package resolution happen before rule evaluation, during it, or through a fixed point?
- what is the semantic definition of a service without importing systemd or Kubernetes assumptions?
- how much rollout planning can be generic across machines, schedulers, and external APIs?
- how can persistent-data migrations remain declarative without becoming too weak for real systems?

## bootstrapping and adoption

- which implementation language gives the right balance of performance, embedding, and development speed? (decision 0016 proposes rust for the kernel, with prolog and lean outside it)
- how is the first toolchain bootstrapped without making the bootstrap chain the permanent public model?
- can an existing Make, Cargo, npm, Nix, or Terraform project be imported as an explicit opaque boundary? ([0032](../decisions/0032-action-granularity.md) settles the granularity: a foreign build system is one `Opaque` and one tool invocation is one `Action`, declared per target rather than inferred. whether the import is usable is untested, because `Opaque` exists only as a marker type)
- what is the smallest vertical slice that tests the kernel instead of building a polished domain facade around missing semantics?
