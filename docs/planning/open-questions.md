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

milestone M-1 used decisions 0015 and 0019 as prototype hypotheses and is complete at its semantic-prototype scope. those decisions remain proposed: their broader interface and effect-category claims stay open until the later milestones exercise them.

## language and types

- should the language use structural or nominal types at module boundaries? (decision 0017 proposed structural default with opt-in nominal; superseded by [0026](../decisions/0026-generic-typed-calculus.md), which carries that mechanism into a full closed calculus with records, declared sums, generics, effect types, and uncertainty types)
- how much refinement typing can stay fast enough for editors? (settled by [0026](../decisions/0026-generic-typed-calculus.md): no predicate types in the language; validation is pure rules; the `Unchecked<T>`/`T` distinction is structural)
- should evaluation be total by construction, or can termination be checked with an explicit unsafe boundary? (decision 0018 proposes total by construction with cycle detection and a backstop limit)
- what is the smallest effect syntax that keeps capability use visible?
- how do typed values cross repository and version boundaries without freezing the type system too early?

## rule engine

- should rules be selected by output type, explicit name, capability interface, or a combination? (decision 0015 proposes interface match)
- how are multiple valid providers ranked without introducing invisible policy? (decision 0015 proposes refusing rather than ranking)
- what equality is used for change pruning when values contain opaque or external references?
- how much dynamic graph structure can be allowed while keeping queries useful before execution?
- which parts of the graph persist between daemon versions? ([0024](../decisions/0024-persistent-engine-state.md) persists attempts, edges, provenance, and the reusable index; [0027](../decisions/0027-retention-and-gc.md) frames what is retained and for how long)

## constraints

- is there one generic constraint representation with multiple solvers, or several domain-specific models with shared evidence?
- how are preferences separated from hard requirements?
- what makes a resolution explanation useful for versions, toolchains, and machine placement?
- how are locks represented when the valid result depends on several target platforms?

## actions and effects

- are `Action`, `Observation`, and `Mutation` separate primitives or handlers of one effect calculus? (decision 0019 proposes five type-level categories — `Pure`, `Action`, `Observation`, `Mutation`, `Opaque` — with nondeterminism tracked as a dependency)
- should the synchronous pure step machine be unable to even name effectful steps at the type level? today `PureStep` carries `NeedBlob`/`NeedAction` variants that `evaluate_pure` rejects at runtime (`E-1206`); a separate pure-only step type would make that a compile property, which is what decision 0022's "structurally pure core" already claims
- which action properties can be enforced and which can only be claimed by an adapter? (decision 0014 addresses the reproducibility subcase; decision 0028 records the local-Linux enforcement claim — declared paths via landlock, declared syscalls via seccomp, `AccessVerification` reported from what was installed; the general enforcement question and the network-egress subcase remain open)
- how is secret taint tracked through values and diagnostics?
- how should retry safety and compensation be represented for mutations? (decision 0012 names the validity scope; retry-safety representation is open)
- can observations participate in incremental computation without making pure results time-dependent? (decision 0012 pins observation revisions; the incremental-purity interaction is open)

## kernel type system and content model

- is the `K` phantom type parameter on `Request<K>` / `Rule<K>` exploited enough? it prevents type confusion but the engine then re-derives pure-vs-action via runtime matches; leaning in further (distinct step types, distinct rule-id brands per effect) would move more checks to compile time
- should `RuleId` carry a per-effect brand so a pure `RuleId` cannot index the action body map? the brands exist (`define_arena!`) but `RuleId` is shared across both arenas today
- should `EvalFrame`'s `resume_with: Option<Value>` be a typed state (`Initial` vs `Resuming(Value)`) so "forgot to set the resume value" is unrepresentable?

## identity and state

- what gives a semantic object its identity when files move or modules are refactored? (decision 0013 introduces managed-object identity for the deployment case; source-level semantic identity is open)
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
- can an existing Make, Cargo, npm, Nix, or Terraform project be imported as an explicit opaque boundary?
- what is the smallest vertical slice that tests the kernel instead of building a polished domain facade around missing semantics?
