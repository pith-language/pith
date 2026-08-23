---
schema: design-doc/v1
id: decision-0019-effect-categories-and-nondeterminism
title: five type-level effect categories, with nondeterminism as a tracked dependency
summary: Pure, Action, Observation, Mutation, and Opaque are distinct types in the kernel IR; nondeterminism enters the graph as a capability dependency rather than an untracked property
kind: decision
status: proposed
created: 2026-04-23
updated: 2026-04-23
tags:
  - effects
  - capabilities
  - language
relations:
  informed_by:
    - research-build-systems
    - research-deployment-and-state
    - research-reproducibility
  depends_on:
    - decision-0003-explicit-effects
    - decision-0007-tracked-dynamic-dependencies
    - decision-0013-managed-object-identity
    - decision-0014-reproducibility-properties
    - decision-0015-interface-rule-selection
    - decision-0018-termination-and-recursion
  supersedes: []
---

# five type-level effect categories, with nondeterminism as a tracked dependency

## context

decision 0003 proposes that the model distinguish pure computation, bounded actions, observations, and mutations, and leaves the exact effect calculus open. the effects-and-capabilities design doc says the final implementation may express these through one effect calculus but that "their different behavior must remain available to the scheduler, cache, policy engine, and user interface." the open questions list names the choice directly: are `Action`, `Observation`, and `Mutation` separate primitives or handlers of one effect calculus.

the question is load-bearing for milestone M-1. a rule's request model cannot be defined until the engine knows what categories of work a rule may perform and how the graph records them. decision 0015 settles rule selection and is the stated gate on the semantic prototype; the effect algebra is the other term that prototype depends on and cannot define for itself.

two further questions change the shape of the model and are settled here rather than left implicit.

first, the four named categories leave no place for unmodeled effectful work. real adoption does not begin with every rule fully categorized. a build that wraps an opaque toolchain, an imported legacy target, or a hastily written migration cannot honestly declare `Action`, `Observation`, or `Mutation` until someone has modeled what it does. without a category for that, the model forces premature commitment or silently collapses unmodeled work into one of the four.

second, the reproducibility lineage (research/reproducibility) enumerates the ways a build becomes nondeterministic: timestamps, readdir order, locale, address-space layout, embedded randomness, parallelism races, cpu-feature detection, and others. decision 0014 says the engine verifies reproducibility by comparison and does not produce it. the question is whether nondeterminism is something the graph tracks as a dependency, like a file read, or something the executor enforces as a contract, like a sandbox restriction. the choice changes whether the type of a rule alone can say anything about its reproducibility.

## proposed decision

### five categories, type-level

the kernel IR distinguishes five effect categories. each is a distinct type. the scheduler, cache, capability checker, and policy engine dispatch on the type. no category is a field on another.

- `Pure<A>` computes from immutable values. terminates by construction per decision 0018. caches indefinitely under its computation identity.
- `Action<A>` runs bounded external work with declared inputs, outputs, platform, and capabilities. cacheable by content identity when the executor honors the declared action contract (requirement A-6).
- `Observation<A>` reads external state and records source, revision, freshness, and uncertainty. carries a revision pin per decision 0012. not cacheable across revisions; staleness is a graph fact.
- `Mutation<A>` changes external state. carries authority, target managed-object identity (decision 0013), retry behavior, known failure modes, and completion evidence. not cacheable as a result; its effect is recorded in provenance and in subsequent observations.
- `Opaque<A>` is effectful work that has not been modeled into one of the four categories above. it is the adoption on-ramp and the escape hatch. an `Opaque` result carries a visibly weaker guarantee: the graph records that the rule performed effectful work but does not know its category, its declared inputs, or its authority. it is treated, for scheduling and caching, like a Nix derivation or a Bazel genrule: a fixed-output boundary whose interior the engine cannot inspect.

`Opaque` is foundational, not a future amendment. it exists so the category system can be opt-in by progression rather than required up front.

### closure rule

the category set is closed. a sixth category requires a kernel decision record. it does not amend this decision silently and does not arrive as a library extension. this mirrors how decision 0013 added a fifth identity type: by argument and record, not by field.

a future category that is a pure rename of an existing one is an amendment to this decision and leaves it standing. a future category that introduces new scheduling, caching, or authority semantics supersedes this decision, which stays in the repository.

### capabilities are the declared dependency surface

a rule's declared capability set is its declared external dependency surface, not only its authority. an `Observe<Platform>` capability declares that the result may depend on observed platform state, and the graph records an edge to that observed value at its revision. this is decision 0007 (tracked dynamic dependencies) applied to effects: every external thing a rule could depend on enters through a capability, and every capability use becomes a graph edge.

capability requirements propagate through composition and are queryable before execution, as requirements K-6 and T-3 already require.

### nondeterminism is a tracked dependency

nondeterminism is modeled as a dependency, not as an untracked property of the executor. a rule whose result depends on the wall clock, on randomness, on readdir order, on address-space layout, or on any other source the reproducibility community enumerated creates a dependency edge to that source, the same way a rule that reads a file creates an edge to the file's content.

at the kernel layer, the dependency is to a single `Nondeterminism` capability primitive. the kernel records that the result depends on something outside the declared deterministic inputs. it does not record the specific value of what was read.

the granular taxonomy — `Time`, `Rng`, `ReaddirOrder`, `Locale`, `AddressSpaceLayout`, `Hostname`, `Username`, `CpuFeatures`, `ParallelismOrder`, and the rest of the reproducibility lineage's enumeration — is a library refinement of the kernel primitive, not a set of kernel types. a first-party determinism library (or a shared library the build library composes) specializes `Nondeterminism` into these granular sub-capabilities. the sandbox and executor report actual access at the granular level and record it in provenance, where library-level reproducibility analysis can use it.

the dependency records the source, not the value. a build that reads the clock creates an edge to `Nondeterminism` (refined by the library to `Time`), not an edge to `1722739200`. whether the build is reproducible depends on whether the executor fixed the value, which is a contract fact recorded in provenance, consistent with decision 0014. the engine does not produce reproducibility by pinning nondeterministic values into the graph; it records that nondeterminism was a dependency and lets the executor's determinism discipline, verified by comparison per 0014, establish the rest.

### consumer policy layers on top of category

the category is a structural input to the scheduler, cache, and policy engine. it is not a complete policy. each consumer may layer its own policy on top: the cache may treat an `Observation` as valid for a configured window while the scheduler treats it as stale immediately. the category tells a consumer what kind of thing it is scheduling or caching; the consumer decides what to do with that, within the contract the category imposes (an `Observation` is never cached across revisions regardless of consumer policy).

## why type-level primitives, not one tagged value

decision 0005 separates four identity types rather than overloading one identifier, and decision 0013 adds a fifth (managed-object), because using one identifier for semantic, computation, content, and external identity "makes refactors, replacement, adoption, caching, and provenance interfere with each other." decision 0015 refuses to rank ambiguous rule candidates, because "priority numbers and scores rot."

the same reasoning applies here. the four operational categories have genuinely different caching, scheduling, authority, and staleness contracts. an `Observation` and a read-only `Action` may exercise similar capabilities but have different staleness and cache semantics. a `Mutation` is not cacheable as a result at all. if the category were a field on one `Effect` type, the distinction the scheduler depends on would be a value someone has to set correctly, with no structural enforcement. that is the same failure as a priority score: a silent local property that can drift, invisible in review.

making the categories distinct types enforces the distinction structurally. the cost is that a sixth category requires an IR change rather than a field value, which the closure rule above accepts deliberately. the alternative — one primitive with a category field — is the option this decision exists to reject, for the same reason 0015 rejects ranking and 0005 rejects identity overloading.

the friction case is real and accepted: an operation that genuinely spans two categories, such as a build action that also observes the host toolchain, is not allowed to be silently one-or-the-other. it is modeled as one category whose declared inputs include a value of another (an `Action` whose inputs include an `Observation`), which makes the composition explicit and visible in provenance. this is more verbose and more correct, the same tradeoff 0015 chose for ambiguity.

## why an escape category rather than requiring full categorization

Nix avoided this entire problem by collapsing all impure work into one opaque boundary, the derivation. the cost is that Nix cannot observe live state, track mutation, or carry capability granularity, which is why deployment and infrastructure stayed outside its model and the project this notebook designs for exists.

requiring every rule to declare one of four honest categories on day one would re-impose, at a different layer, the purity bar Nix avoids. real projects adopt tools around a subset of their build or deployment (requirement U-2). a rule that wraps an opaque toolchain, an imported Makefile, or a hastily written cloud call cannot honestly declare its category until someone has modeled it. forcing the choice produces false declarations, which are worse than an honest `Opaque` marker.

`Opaque` is the visibly distinct, marked construct the principles doc allows for cases that cannot yet be expressed in the normal machinery (principles:80). its weaker guarantee is visible in types, plans, queries, and user interfaces, consistent with requirement T-6. there is no hidden option that makes a categorized rule behave as `Opaque` or vice versa.

the incentive to migrate off `Opaque` is structural rather than moral. `Opaque` results do not participate in capability propagation, fine-grained invalidation, revision pinning, reproducibility analysis, or authority queries, because the engine does not know what they depend on or what authority they exercised. as a project models its rules, each migration unlocks those properties. whether that incentive is strong enough in practice for real projects to mostly migrate off `Opaque` is an empirical question only a prototype answers, and is a stated reason this decision is proposed rather than accepted.

## interaction with decision 0014

decision 0014 separates three properties: content-addressed identity (constructed), clean-build equivalence (engine-provided under the declared contract), and bit-for-bit reproducibility (verified by comparison, not produced).

this decision is consistent with 0014 and does not amend it. the engine does not pin nondeterministic values into the graph, so it does not produce reproducibility by construction. it records that a result depended on `Nondeterminism`, and the executor's determinism discipline (SOURCE_DATE_EPOCH clamping, ordered readdir, fixed locale, and the rest of the reproducibility rules) is what makes two builds comparable. verification remains by building twice and comparing content identities, per 0014.

what this decision adds is a necessary precondition for cheap verification: the type of a rule can now say, at the library-refined granularity, which sources of nondeterminism the result depends on. a rule whose type carries no `Nondeterminism` capability is reproducible by construction at the capability axis, because the graph has recorded that it depends on nothing outside its declared deterministic inputs. a rule whose type carries `Time` (library-refined) is reproducible only if the executor fixed the clock. this does not replace verification; it tells the verifier where to look and lets the build library refuse to assert reproducibility for rules whose types make it impossible.

whether the granular library taxonomy is rich enough that static type analysis meaningfully predicts reproducibility, or whether verification always dominates, is open and is part of what the prototype tests.

## alternatives considered

### one effect primitive with a category field

one `Effect<A>` type carrying a category tag. the scheduler reads the field.

smallest surface and the easiest to extend. it puts the load-bearing distinction the scheduler depends on into a value that can be set incorrectly with no structural enforcement. this is the silent-local-property failure mode the design rejects elsewhere (0015 on ranking, 0005 on identity overloading). a category field repeats that failure at a different layer. rejected on the same principle.

### derive category from capability

the category is whatever capability the effect requires: an `Execution` capability implies `Action`, an `Observe` capability implies `Observation`, a `Mutate` capability implies `Mutation`.

true one-mechanism: capabilities already exist for authority, so deriving category from them eliminates a redundant axis. it conflates two questions the deployment decisions carefully separated: what authority does this exercise (capability) and what is the caching and scheduling contract of this work (category). they come apart in real cases. a build `Action` that runs hermetically exercises an `Execution` capability but its category is independent of the fact that it executes. an `Observation` and a read-only `Action` may both require an `Observe` capability but have different staleness and cache contracts. the deployment research (research/deployment-and-state) records that the whole problem with prior systems is confusing authority and ownership with the nature of the operation: Terraform's failures come from treating ownership of a resource as if it determined that a plan still holds. decision 0012 exists to separate the observation's revision from the mutation's authority. deriving category from capability re-entangles exactly what 0012 and 0013 were written to separate. rejected.

### effect handlers, categories as registered handlers

the type system knows pure versus effectful; `Action`, `Observation`, and `Mutation` are registered handler classes.

most extensible: a new category is a new handler, no IR change. it inverts the requirement the effects design doc states. the four categories "must remain available to the scheduler, cache, policy engine, and user interface." handler dispatch hides the category from the scheduler: the scheduler sees an effect and the policy lives inside the handler, not in the value the graph carries. for a graph whose job is to schedule, cache, and explain work differently by category, putting the category behind a handler indirection defeats the purpose. handlers are the right tool when the caller decides how to interpret an effect; here the engine needs to know the category to schedule it. rejected for the dispatch model; the underlying one-calculus idea is absorbed into the type-level primitives, which can share a common internal representation while exposing distinct types.

### require full categorization, no escape category

the four honest categories only; every rule must declare one.

strongest safety story and the highest adoption barrier. real adoption does not begin fully modeled, and forcing the choice produces false declarations, which are worse than an honest `Opaque` marker because they are invisible. this re-imposes Nix's purity bar at a different layer while still expecting the scope Nix cannot cover. rejected; `Opaque` is added as a foundational category instead.

### defer, prototype with one primitive and revisit

ship the prototype with one `Effect` type and a category field, marked provisional, and let the prototype reveal whether the categories need to be type-level.

violates the notebook's own discipline. decision 0015 is resolved before the prototype because prototyping on an undefined base cannot test the question. the effect algebra is the other term milestone M-1 depends on. deferring it while building the prototype on top of an undefined effect model means the prototype cannot test it honestly. rejected for the same reason 0015 was not deferred.

## consequences

the kernel IR has five effect categories as distinct types. the scheduler, cache, capability checker, and policy engine read the category structurally rather than by convention.

`Opaque` is the adoption default and the escape hatch. new rules and imported legacy targets begin as `Opaque` and migrate to categorized rules as the project models them. the graph always shows which rules are modeled and which are not.

nondeterminism enters the graph as a dependency on a kernel `Nondeterminism` capability, refined granularly by a first-party library. reproducibility stays a verified property per decision 0014; this decision adds a static precondition (the type says which nondeterminism sources apply) without replacing verification.

an operation that spans two categories is modeled as a composition (one category whose inputs include a value of another), not as a silent either-or. this is more verbose and more honest.

the category set is closed. a sixth category requires a decision record and either amends or supersedes this one.

[0060](0060-observation-identity-and-freshness.md) makes the third category operational: an observation rule derives a subject, an observer returns a value and revision, and recorded consumers revalidate freshness by attesting that revision. `Pure`, `Action` and `Observation` therefore exercise the structural split; `Mutation` and `Opaque` remain marker types, so this record stays proposed.

## unresolved

the granular nondeterminism taxonomy — which sources the first-party library names, how it specializes the kernel primitive, and whether the taxonomy is stable enough that static type analysis meaningfully predicts reproducibility — needs library design alongside the first build library. the reproducibility lineage's enumeration is the starting set; whether it is complete enough is empirical.

whether the `Opaque` category is strong enough as an adoption on-ramp that real projects mostly migrate off it, or whether it becomes a permanent crutch that leaves most rules unmodeled, is the central empirical question this decision raises. it is the main reason the decision is proposed rather than accepted.

the exact type-level syntax for declaring a rule's category and capability set, and how that surface composes with the structural-and-nominal typing of decision 0017, needs design alongside the values-and-types work.

how an `Opaque` rule is migrated to a categorized rule without invalidating its historical provenance (whether old `Opaque` results remain queryable as `Opaque` after the rule is re-typed) is a provenance-versioning question for the prototype.
