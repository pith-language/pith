---
schema: design-doc/v1
id: decision-0032-action-granularity
title: one action is one tool invocation, and a foreign build system is one opaque boundary
summary: fix the granularity at which external work enters the graph — `Action` is a single tool invocation with a declared contract, `Opaque` is an entire foreign build system as a fixed-output boundary, and a target declares which it is
kind: decision
status: proposed
created: 2026-05-29
updated: 2026-05-29
tags:
  - actions
  - effects
  - build
  - adoption
relations:
  informed_by:
    - research-build-systems
    - research-nix
  depends_on:
    - decision-0003-explicit-effects
    - decision-0007-tracked-dynamic-dependencies
    - decision-0019-effect-categories-and-nondeterminism
    - decision-0020-nix-as-adapter-not-substrate
    - decision-0031-action-cache-identity
  supersedes: []
---

# one action is one tool invocation, and a foreign build system is one opaque boundary

> collects a decision the notebook has already made in five separate places and never stated once. it introduces no new mechanism: [0019](0019-effect-categories-and-nondeterminism.md) fixed the categories, [0020](0020-nix-as-adapter-not-substrate.md) fixed how foreign work enters, and the glossary fixed what an action is. what was missing is the sentence that says how coarse an `Action` is, which is the question the first build library cannot start without.

## context

the notebook never asks how much work one `Action` represents. it answers the question anyway, in five documents that do not cite each other.

the glossary defines the term: "a bounded external computation with declared inputs and outputs. a compiler invocation is an action." the rules-and-graph design doc's only worked example is `rule compile(source: Source, compiler: Compiler) -> Artifact`, which takes one source. requirement U-5 asks the build library for "fine-grained invalidation." 0019 describes `Opaque` as work "treated, for scheduling and caching, like a Nix derivation or a Bazel genrule: a fixed-output boundary whose interior the engine cannot inspect." 0020 makes a nixpkgs derivation enter the graph "as `Opaque`, with a content identity, and usable like any other value."

read together these say something specific: one tool invocation is an `Action`, one foreign build system is an `Opaque`, and the two are different categories rather than two settings of one knob. read separately, none of them says it, which is why the question keeps reappearing. it reappeared again when the first toolchain ran, because `crates/pith-executor-local/tests/real_toolchain.rs` compiles exactly one source file and there is nothing in the repository that says whether that is the intended unit or an artifact of the test being small.

the question is not academic. it decides what the first build library is. at one granularity `xylem` defines rules that plan a compiler invocation per source and a linker invocation per binary, and the engine's dependency edges, reuse index, and invalidation explanations are what make a rebuild small. at the other it defines one rule that plans `cargo build`, and the engine schedules a single opaque box whose interior does its own caching. both are buildable on the current kernel. they are different products.

## proposed decision

the two granularities are two categories, not two policies, and each names what it is.

an `Action` is one invocation of one tool. `gcc -c a.c -o a.o` is an action. `ld a.o b.o -o prog` is an action. the declared contract names that invocation's own inputs, outputs, toolchain closure, environment, platform, capabilities, and network policy, and the engine confines, caches, and explains at that granularity. this is the granularity the glossary already states and the one the engine was built for.

an `Opaque` is one entire foreign build system. `cargo build`, `gradle assemble`, `make`, and a nixpkgs derivation are each one `Opaque`. the engine records that effectful work happened and takes a content identity for what came out. it does not know the interior, does not confine it beyond what the host offers, and makes no claim about what it read.

a target declares which one it is. there is no inference, no automatic decomposition of a foreign build into actions, and no default that silently downgrades an `Action` into an `Opaque` when its contract turns out to be incomplete. a contract that cannot be honestly declared is an `Opaque` and says so, which is 0019's rule applied to the case that motivates it most.

### why one tool invocation is the `Action` unit

the kernel's cost is already paid at that granularity. per-attempt dependency edges (0024), the reusable action index and its admission test (0031), `explain_invalidation` over the recorded graph (0025), capability propagation and use edges (0019), and landlock confinement over a declared closure (0030) are all machinery that distinguishes one unit of external work from another. at derivation granularity none of it earns its keep: one box per package, one cache entry per box, and an interior the engine cannot explain. that is Nix, and 0020 already decided that pith reuses Nix as an adapter rather than reproducing it.

it is also the granularity the enforcement claims are true at. requirement A-2 says executors prevent or detect access outside the declared contract. `gcc -c` has a declarable contract and 0030 demonstrated confinement of it. `cargo build` wants the network, a package registry, and a home directory; declaring a contract for it means declaring approximately the whole host, at which point A-2 is satisfied by vacuum. the honest reading is that the coarse case does not have a declared contract, which is exactly what `Opaque` means.

### why `Opaque` is a peer, not a lesser setting

U-2 requires that "existing projects can adopt the tool around a subset of their build or deployment" with "unmodeled boundaries remain explicit." 0019 is explicit that `Opaque` is "foundational, not a future amendment" and "exists so the category system can be opt-in by progression rather than required up front." 0020 makes it the mechanism by which a mature package collection becomes reachable on day one.

a project that wraps its existing `cargo build` in an `Opaque` and gets caching, provenance, content identity, and composition with other pith values has gained something real, and has done so without lying about a contract it cannot honor. that is the intended on-ramp, not a failure to adopt properly.

what it gives up is enumerated by 0019 and not repeated here: an `Opaque` result does not participate in capability propagation, fine-grained invalidation, revision pinning, reproducibility analysis, or authority queries. the incentive to model a target as actions is that each of those turns on. this record adds only that the incentive is meant to operate per target, so a repository can hold both and migrate one target at a time.

### where the boundary falls between tools

the distinction is not "external program" versus "internal program." it is whether the thing has a declarable contract or is itself a build system.

`gcc`, `as`, `ld`, `ar`, `rustc`, `javac`, and `protoc` invoke once, read what they are given, and write what they are asked for. they are actions. the toolchain they need enters as a declared closure (0030) rather than as ambient authority.

`cargo`, `gradle`, `poetry`, `uv`, `npm`, `make`, and `cmake --build` resolve dependencies, own a lockfile, maintain their own cache, and decide their own build order. they are rival kernels. wrapping one as an `Action` would be claiming a contract for work that determines its own inputs while running, which 0007 forbids as ambient discovery. they are `Opaque`.

the second list has a further consequence worth stating, because it constrains M-4 rather than M-3. those tools carry their own resolvers and lock data, and the package library that milestone M-4 opens is specified by U-6 to own "multiple versions, variants, constraints, feature selection, lock data, source and binary distribution, and resolution explanations." a repository that resolves through `cargo` inside an `Opaque` has not used the package library; it has delegated M-4's job. both are legitimate, and which one a user is doing must be visible rather than blurred, which the category distinction gives for free.

### the cost this record accepts

at action granularity something has to know that `a.c` includes `a.h`, and the engine will not discover it by watching. 0007 already settled the mechanism: dependencies are selected by rules through tracked requests, and "ambient filesystem, environment, process, or network discovery is not a valid dependency mechanism." what 0007 leaves open, and the build-system research names directly, is whether that knowledge is declared by the author (Bazel) or inferred by a language-aware rule that then makes the request (Pants).

this record does not settle that; it records that the choice belongs to `xylem` rather than to the kernel, and that either answer produces tracked requests rather than traced reads. 0007's note already covers the failure mode: inference can be incomplete, so hermetic execution must fail when an undeclared dependency is missed, which is the property 0030's landlock ruleset now provides.

## alternatives considered

### derivation granularity only

one action is one package build, whose interior runs a builder script. the Nix shape.

this is the smallest build library and the fastest route to something usable, and it is what most people expect from a Nix successor. it is rejected as the model because it makes the kernel's distinguishing machinery inert. per-attempt dependency edges, the action reuse index, invalidation explanations, and capability-use edges all exist to tell one unit of work from another, and at package granularity there is one unit per package. a project that adopted this exclusively would have a typed, better-tooled Nix, which is a reasonable thing to want and is not what 0001 through 0031 describe. it is not rejected as a capability: it is `Opaque`, available per target, which is what this record makes explicit.

### action granularity only, with no opaque escape

require every target to declare a real contract. the strongest guarantee.

rejected on the same ground 0019 rejected requiring full categorization: real adoption does not begin fully modeled, and forcing the declaration produces false contracts, which are worse than an honest `Opaque` because they are invisible. it would also make 0020's nixpkgs adapter unimplementable, since a nixpkgs derivation's interior is precisely what pith cannot declare.

### infer granularity from the contract

let a rule declare whatever contract it can, and have the engine decide whether the result is trustworthy enough to treat as an `Action` or must be demoted to `Opaque`.

rejected because it makes a type-level fact into a runtime judgment. 0019's whole argument for distinct types over a category field is that a load-bearing distinction the scheduler depends on must not be a value someone sets, and a demotion rule is worse than a field: it is a value the engine sets, from a heuristic, after the fact. an author who cannot declare a contract should write `Opaque` and have that visible in the source.

### wrap everything opaque first, then refine

adopt by declaring one `Opaque` per project and decompose into actions over time.

this is not an alternative to this record, it is the adoption path this record enables, and it is worth naming so it is not mistaken for a competing option. what makes it work is that the two categories coexist per target within one repository, which is the thing being decided here.

## consequences

`xylem`, the build library milestone M-3 opens, defines action-granular rules: one compile per source, one link per binary, one archive per library. the toolchain closure discovery currently living in `crates/pith-executor-local/tests/support/mod.rs` moves into it, which is where 0030 said it belongs.

`Opaque` needs an operational path, and does not have one. `crates/pith-core/src/effect.rs` declares `Opaque` as a marker type sealed into `EffectCategory` with `CACHEABLE_AS_RESULT = false`, and that is the entire implementation: there is no rule registration, no step variant, no scheduler path, and no durable record. the same is true of `Observation` and `Mutation`, which are gated on later milestones and can wait. `Opaque` cannot, because U-2's adoption story, 0020's nixpkgs adapter, and the coarse half of this record all rest on it. it is also the cheapest of the three: a fixed-output boundary is an action that declares no inputs, claims no confinement, and commits to an output identity.

the incremental gap 0031 names becomes the gating engine work rather than a footnote. at action granularity a build is compile-then-link, so the consumer of an action is the ordinary case rather than an edge case, and `crates/pith-engine/src/graph/reuse.rs` marking every pure computation with an action dependency `NotReusable(EffectfulDependency)` means the link step re-plans on every run. 0031's consequences section explains why the current invariant is correct and names carrying the action's identity into its consumer's key as the fix. this record makes that fix a prerequisite for U-5 rather than an optimization.

a repository can hold both categories, and queries can report which. "how much of this build is modeled" becomes an answerable question over the graph, which is the structural migration incentive 0019 describes made visible.

## unresolved

whether `xylem` declares source-level dependencies or infers them from source analysis is open, and is the question the build-system research already names ("should dependency inference live in language-specific libraries or a shared compiler-service layer?"). both produce tracked requests; they differ in who writes them. the first `xylem` prototype should do the declared version, because inference that is wrong is indistinguishable from inference that is right until an undeclared read is denied.

whether an `Opaque` boundary can be refined into actions incrementally within one target, rather than replaced wholesale, is open. the appealing case is a foreign build whose final link step is modeled while its compiles stay opaque; whether that composes usefully or just produces a boundary with no cache value needs a prototype.

the seam between a delegated resolver inside an `Opaque` and the package library M-4 opens is named above and not settled. a project that resolves through `cargo` and a project that resolves through `phloem` produce different provenance, and how the two coexist in one repository, or convert into each other, belongs to M-4.

whether `Opaque` should carry a declared output identity the engine verifies (Nix's fixed-output derivation shape) or accept whatever it produced and content-address it, is an implementation question this record does not settle. the fixed-output shape is stronger and is what 0019's "fixed-output boundary" phrasing implies; accepting the output is cheaper and is what the current import path already does for actions.
