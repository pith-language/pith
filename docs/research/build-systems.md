---
schema: design-doc/v1
id: research-build-systems
title: build-system lineage
summary: why modern graph build systems chose their dependency, incremental, and extension models
kind: research
status: researching
evidence: reviewed
created: 2026-03-03
updated: 2026-03-03
tags:
  - research
  - builds
  - incrementality
relations:
  informed_by: []
  depends_on:
    - research-method
  supersedes: []
---

# build-system lineage

the useful history here is not a straight `Bazel -> Buck2` chain.

Blaze and Buck were separate responses to large monorepositories where Make-like task orchestration stopped scaling. Bazel came from Blaze. Buck2 replaced Buck and explicitly borrowed from Bazel, Pants, Shake, Tup, Adapton, and build-system research.

```text
Make-like task systems
        |
        +-- Blaze -- Bazel ------+
        |                        |
        +-- Buck 1 --------------+-- Buck2
                                 |
              Pants, Shake, Tup, Adapton
```

the lines show influence. they do not imply shared code.

## the pressure

large repositories contain several languages, shared generated inputs, and enough targets that rerunning commands by convention becomes slow and unreliable. builds also need to work across developer machines and remote workers.

Bazel's account contrasts task-based systems with artifact-based systems. an artifact graph gives the engine enough information to schedule work, cache results, and avoid work whose inputs did not change.

Buck and Blaze were created inside Meta and Google under similar monorepo pressure. Buck2's own history says the rewrite kept Buck's target graph, remote execution, file watching, multi-language support, and Starlark while replacing large parts of the engine.

## Bazel and Skyframe

Skyframe models evaluation as immutable keys, immutable values, and functions that request dependencies through the engine. this provides dependency tracking and parallel evaluation as long as every input flows through the graph.

its change pruning is important. when a source change recomputes an object file to the same value, downstream work can remain cached.

Skyframe deliberately uses all-or-nothing recomputation for a node. its documentation discusses fine-grained mutation of previous values and rejects it as difficult to verify against a clean build for limited expected benefit. clean-build equivalence was the stronger invariant.

direct filesystem reads are a correctness hole because the engine cannot invalidate what it did not observe.

## the Buck2 rewrite

Buck2 changed several parts of Buck1 while keeping its user model recognizable.

the core was rewritten in Rust. Meta cites speed and avoiding garbage-collector pauses, while also noting that Java had stronger profiling tools in some areas.

all language rules moved to Starlark. the core became language agnostic, and features previously available to privileged native rules became available to external rule authors.

remote execution became the normal model. local execution is treated as another executor, and inputs are prepared around content digests from the start. Buck2 uses the Bazel Remote Execution API rather than inventing a private protocol.

Buck2 replaced separate loading, analysis, and execution graphs with DICE, one incremental dependency graph. work from different conceptual stages can overlap, and invalidation follows actual graph edges.

dynamic dependencies are supported through controlled mechanisms. Buck2 keeps the graph queryable and hermetic by refusing arbitrary untracked discovery.

its virtual filesystem and deferred materialization reduce filesystem work. this is a useful reminder that build performance is not only compiler CPU time.

## DICE and Adapton

DICE is a demand-driven incremental computation engine. computations are keyed, dependencies are recorded as they are requested, values are shared, and unchanged results can cut off downstream work.

the DICE retrospective describes a move away from fine-grained locks in the async evaluator toward a single-threaded core state. the locks had made the engine difficult to reason about. parallel work remains outside that state-management bottleneck.

Adapton attacked two limits in earlier incremental computation: recomputing outputs that were no longer demanded and poor reuse when computations moved between contexts. it separated inner incremental computations from outer observers and tracked a demanded computation graph.

Buck2 names Adapton as an influence. this does not make DICE an Adapton implementation, but the demand-driven graph and reuse problem are part of the same research line.

## Pants v2 as another answer

Pants v2 also replaced an earlier engine with Rust while keeping extension logic in a higher-level language. typed Python rules run through the same engine used by built-in behavior.

Pants chose more dependency inference than Bazel. its stated reason was the cost of manually maintained build metadata. inference uses source analysis, and hermetic execution prevents a missed dependency from silently succeeding through ambient access.

this is a useful alternative to both fully manual declarations and unrestricted tracing. derived dependencies can keep provenance and fail closed.

## the research model

*Build Systems à la Carte* separates the scheduling algorithm from the rebuilding strategy. systems that look like one indivisible architecture can be described as combinations of choices: static or dynamic dependencies, dirty bits or dependency traces, restarting or suspending schedulers, local or cloud execution.

that separation matters for this project. it lets us choose demand-driven rules without automatically inheriting every cache or scheduling decision from Buck2.

## decisions suggested by this lineage

keep an explicit graph and clean-build equivalence.

allow dependencies to be selected during evaluation, but require every selection to pass through a tracked request. inference should add inspectable graph edges, not invisible magic.

make rule APIs language-neutral and give first-party rules no private powers.

treat remote execution and deferred materialization as part of the action and artifact contracts early. they are expensive to attach after rules have learned to depend on local paths and processes.

keep scheduling, invalidation, storage, and rule semantics separable enough to test them independently.

## questions still open

- should rule evaluation suspend on dependency requests like Shake, Pants, and DICE?
- can the graph be queried usefully before all dynamic dependencies are discovered?
- should dependency inference live in language-specific libraries or a shared compiler-service layer?
- what equality and naming model gives useful change pruning across repository refactors?
- when may an action reuse a previous output directory, and how can stale output be detected?
- is the Remote Execution API expressive enough for the intended capability and secret model?

## sources

- [Bazel build-system basics](https://bazel.build/basics)
- [Bazel Skyframe](https://bazel.build/versions/8.2.0/reference/skyframe)
- [Bazel hermeticity](https://bazel.build/basics/hermeticity)
- [Buck2: why Buck2](https://buck2.build/docs/about/why/)
- [Buck2 architecture](https://buck2.build/docs/concepts/architecture/)
- [Buck2 DICE](https://buck2.build/docs/insights_and_knowledge/modern_dice/)
- [Buck2 remote execution](https://buck2.build/docs/users/remote_execution/)
- [Buck2 BXL](https://buck2.build/docs/bxl/)
- [Meta's Buck2 announcement and retrospective](https://engineering.fb.com/2023/04/06/open-source/buck2-open-source-large-scale-build-system/)
- [Pants v2 design lessons](https://www.pantsbuild.org/blog/2020/10/27/introducing-pants-v2)
- [Adapton](https://matthewhammer.org/adapton/)
- [Build Systems à la Carte](https://www.microsoft.com/en-us/research/publication/build-systems-a-la-carte/)
- [Bazel Remote Execution API](https://github.com/bazelbuild/remote-apis)
