---
schema: design-doc/v1
id: planning-open-questions
title: open questions
summary: unresolved questions that can still change the architecture
kind: planning
status: active
created: 2026-03-23
updated: 2026-08-21
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

- milestone M-5b (Linux system activation) and M-6 (deployment library) are gated by decision 0012 (revision-pinned plans) and 0013 (managed-object identity). M-5a (Linux system composition) is gated by neither, and the earlier form of this line said it was: a composed immutable artifact has content identity, touches no machine, and needs no revision-pinned plan or managed object. the gate applies where an external object is adopted and mutated, which is the activation half
- milestone M-6 is additionally gated on a foundation contradiction rather than a decision: it needs an ordered, reversible plan, and scope.md says the project is not an ordered task runner. the record that settles that is named in the milestone, and [milestones](milestones.md) now says it depends on nothing else in the sequence and can run in parallel from any point
- the time and resource bound five callers were each deferring to the next milestone is milestone M-8, and is no longer a gate held by whichever milestone needed it last
- the reproducibility story in milestone M-3 (first build library) is gated by decision 0014
- surface language syntax is gated by the 0026 calculus landing in the core, not by a milestone. the older gate has been discharged: [0028](../decisions/0028-sandboxed-local-executor.md) deferred all surface syntax to the M-3 build library on the same grounds as [0026](../decisions/0026-generic-typed-calculus.md), and M-3 is complete — it discharged the deferral by building xylem as a rust library API, so no surface syntax exists yet. what stands in the way now is the calculus itself, and less of it than before: `pith-core`'s `Type` carries the six scalars, `List`, `Record`, a `Nominal` and a `Sum` that each carry their declaration, and the recursion cut. [0047](../decisions/0047-the-declaration-table.md) built the declaration site 0026 kept deferring, so a nominal type's representation is checked rather than trusted and a rule's revision derives from the declarations its interface names. what is still unbuilt is the parametric generics and the uncertainty constructors, which 0047 keeps in the set and gates each on the subsystem that would read it. arbitrary-precision `Int` stood in this list too and [0055](../decisions/0055-arbitrary-precision-int.md) landed it, so that clause is discharged. what a surface syntax would still have nothing to express is the larger gap: pattern matching over a declared sum, the merge operator, and type parameters on a declaration are all absent, and none is a calculus constructor. [0038](../decisions/0038-represented-rule-bodies.md) settles what the kernel consumes instead of syntax — rule bodies as data in one core ir — and sits behind the same calculus gate, but not behind surface syntax: the first frontend is the rust registration API over hand-built ir. [the language frontend](language-frontend.md) proposes the decomposition and argues the gate is correctly sized for the notation and oversized for the two things under it — a published declaration surface, which needs no unbuilt constructor, and the ir expression set, which 0038 names as the first design task it gates. [the reordering](reordering.md) took that argument and gave all three milestones: the declaration surface is M-10, the ir expression set M-11, the notation M-13. so surface work is no longer gated on the calculus in the undifferentiated way this bullet stated it, and what remains genuinely gated is the notation's own missing vocabulary — pattern matching over a declared sum, the merge operator, and type parameters on a declaration

milestone M-1 used decisions 0015 and 0019 as prototype hypotheses. 0015 is now `accepted`: four domains and the frontend design have exercised interface selection, and each `E-1102` collision it predicted has been met by a nominal type rather than by a ranking rule. 0019 remains proposed, and narrowly so — `Pure` and `Action` are operational, and `Observation`, `Mutation` and `Opaque` stay marker types until M-9, M-5b and M-15 exercise them.

## language and types

- should the language use structural or nominal types at module boundaries? (decision 0017 proposed structural default with opt-in nominal; superseded by [0026](../decisions/0026-generic-typed-calculus.md), now accepted, which carries that mechanism into a full closed calculus with records, declared sums, generics, effect types, and uncertainty types. of that calculus `Nominal`, `List`, `Record` and `Sum` are built in `pith-core` and M-5a measured them convergent; the parametric and uncertainty halves are not — see gating)
- how much refinement typing can stay fast enough for editors? (settled by [0026](../decisions/0026-generic-typed-calculus.md): no predicate types in the language; validation is pure rules; the `Unchecked<T>`/`T` distinction is structural)
- should evaluation be total by construction, or can termination be checked with an explicit unsafe boundary? (decision 0018 proposes total by construction with cycle detection and a backstop limit)
- what is the smallest effect syntax that keeps capability use visible? ([0038](../decisions/0038-represented-rule-bodies.md) fixes the kernel-side half: the `PureStep` protocol — `Need`, `NeedAll`, `NeedBlob`, `NeedAction` — is the effect vocabulary of the core ir, as explicit constructs with binders for resumption values. what stays open is the surface spelling above it)
- how do typed values cross repository and version boundaries without freezing the type system too early?

## rule engine

- should rules be selected by output type, explicit name, capability interface, or a combination? ([0015](../decisions/0015-interface-rule-selection.md), accepted: interface match, exercised by four domains and the frontend design)
- how are multiple valid providers ranked without introducing invisible policy? ([0015](../decisions/0015-interface-rule-selection.md), accepted: refusing rather than ranking. every collision the corpus has produced was answered by a nominal type, not by a rule that picks)
- what equality is used for change pruning when values contain opaque or external references? (canonical equality prunes across a pure edge and, since [0033](../decisions/0033-consumer-of-action-reuse.md), across an action edge; opaque and external references are untouched because neither category is operational yet)
- how much dynamic graph structure can be allowed while keeping queries useful before execution?
- which parts of the graph persist between daemon versions? ([0024](../decisions/0024-persistent-engine-state.md) persists attempts, edges, provenance, and the reusable index; [0027](../decisions/0027-retention-and-gc.md) frames what is retained and for how long, with [0051](../decisions/0051-transitive-revalidation.md)'s closure as the lower bound it has to clear)

## constraints

- is there one generic constraint representation with multiple solvers, or several domain-specific models with shared evidence? ([0040](../decisions/0040-declared-constraints-and-resolution.md) proposes the second: constraint models are domain-declared values in the 0026 calculus, and what is shared is a protocol — a solver request names the constraint set, the candidate universe with provenance, and the preference order; an answer names the choice and a derivation. this binds M-6's placement and toolchain domains the same way: they declare their own models rather than translating into package vocabulary)
- how are preferences separated from hard requirements? ([0040](../decisions/0040-declared-constraints-and-resolution.md): hard constraints intersect to define validity; a preference is a third input value, a lexicographic list of orderings the domain already declares — 0039's version scheme makes "newest" fact rather than policy — and an underdetermined preference refuses on 0015's terms)
- what makes a resolution explanation useful for versions, toolchains, and machine placement? ([0040](../decisions/0040-declared-constraints-and-resolution.md): two layers — the engine's existing invalidation explanation names the input that moved, and the solver carries its own derivation as a value in the answer, held to a proof standard that separates "no solution exists" from "the search budget ran out." the exact derivation shape stays open there)
- how are locks represented when the valid result depends on several target platforms? ([0039](../decisions/0039-package-identity.md) fixes the entry's identity half: a package version bound to the content identity of its source, with origin as evidence and per-platform realizations derived rather than locked. whether a lock should ever pin realizations is left here, and the constraint and solver questions above are untouched)

## actions and effects

- are `Action`, `Observation`, and `Mutation` separate primitives or handlers of one effect calculus? (decision 0019 proposes five type-level categories — `Pure`, `Action`, `Observation`, `Mutation`, `Opaque` — with nondeterminism tracked as a dependency)
- should the synchronous pure step machine be unable to even name effectful steps at the type level? today `PureStep` carries `NeedBlob`/`NeedAction` variants that `evaluate_pure` rejects at runtime (`E-1206`); a separate pure-only step type would make that a compile property, which is what decision 0022's "structurally pure core" already claims
- which action properties can be enforced and which can only be claimed by an adapter? (decision 0014 addresses the reproducibility subcase; decision 0028 records the local-Linux enforcement claim — declared paths via landlock, declared syscalls via seccomp, `AccessVerification` reported from what was installed; decision 0030 resolves the executable-as-blob subcase 0028's "unresolved" section named by carrying the toolchain as a declared closure of host paths and making its confinement a kernel fact; the syscall side is now installed and measured, so `Prevented` reports two kernel-enforced layers and a child outside the allowlist is killed; [0044](../decisions/0044-the-first-source-adapter.md) answers the source-side half — a source adapter's provenance claims are measured by the adapter as a caller-side effect and the engine never measures, the fetch being declarable down to an output digest makes its action-shaped future nix's fixed-output derivation with the binding's digest as the declared output, and until an executor exists that can confine network access under such a declaration, the network subcase stays "no executor admits it at all"; the general enforcement question stays open, and the network subcase narrows to egress beyond the local `AF_UNIX` socket the filter admits by argument)
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
- what is the retention and migration model for historical observations and plans? ([decision 0027](../decisions/0027-retention-and-gc.md) frames the problem: roots, policy axes, cross-store ordering. the default numeric parameters wait on workload evidence, and [0051](../decisions/0051-transitive-revalidation.md) puts a floor under them: revalidation walks a reusable attempt's recorded subtree, so the cache tier is that subtree's closure rather than the reusable index alone, and results are needed throughout it)

## first-party domains

- what is the minimal artifact interface needed by both build and deployment libraries? ([0045](../decisions/0045-a-locked-source-becomes-a-built-artifact.md) gives the answer its first extent: a nominal content identity produced by a declared interface — the package build's interface is `(Toolchain, Tree, Build) -> xylem.Executable`, the same nominal content type a package-less build links. [0046](../decisions/0046-an-index-line-carries-the-requirement.md) gives the second: past an executable, the artifact a library package needs is objects plus offered headers as one value over the 0026 constructors, produced by a second declared interface and consumed in-graph by a dependent's build. what stays open is whatever a deployment domain finds it needs that a build does not produce)
- should package resolution happen before rule evaluation, during it, or through a fixed point? ([0040](../decisions/0040-declared-constraints-and-resolution.md) proposes during: resolution is an ordinary request against a declared interface, selected by 0015, whose solver body sits in 0038's host-rule tier. a pre-pass could not key or invalidate its result — a changed candidate universe would leave the engine serving a stale resolution with no input diff to explain it — and the graph's own propagation is the fixed point. the solver's algorithm choice stays open there, and whether several versions of one package may coexist in one realization is left to the first solver to measure)
- when does a dependency edge enter the package line? ([0046](../decisions/0046-an-index-line-carries-the-requirement.md) took it: the index line carries `requires` clauses, so the edge is read from a registry answer rather than fabricated by a fixture; the solver reaches the dependency through the dependent's requirement in one resolve request, the dependency builds as a library (objects plus offered headers) through a second package-build interface, the dependent's build names its dependencies as (tree, build) pairs and the graph orders the edge, and the dependent's compiles see the dependency's headers as declared request data through the third input xylem's compile entry gained. what opens with the answer: dependency chains — the pair is flat and a dependency's own dependencies have no channel yet — and include renaming, both recorded there as unresolved)
- what is the semantic definition of a service without importing systemd or Kubernetes assumptions?
- how much rollout planning can be generic across machines, schedulers, and external APIs?
- how can persistent-data migrations remain declarative without becoming too weak for real systems?

## bootstrapping and adoption

- which implementation language gives the right balance of performance, embedding, and development speed? (decision 0016 proposes rust for the kernel, with prolog and lean outside it)
- how is the first toolchain bootstrapped without making the bootstrap chain the permanent public model?
- can an existing Make, Cargo, npm, Nix, or Terraform project be imported as an explicit opaque boundary? ([0032](../decisions/0032-action-granularity.md) settles the granularity: a foreign build system is one `Opaque` and one tool invocation is one `Action`, declared per target rather than inferred. whether the import is usable is untested, because `Opaque` exists only as a marker type)
- what is the smallest vertical slice that tests the kernel instead of building a polished domain facade around missing semantics?
