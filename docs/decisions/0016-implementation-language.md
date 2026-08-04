---
schema: design-doc/v1
id: decision-0016-implementation-language
title: implement the kernel in rust, graph by arena and index
summary: pick rust for the kernel and use arena-and-index data modeling; reserve unsafe for genuine ffi, never for graph structure
kind: decision
status: proposed
created: 2026-04-20
updated: 2026-04-20
tags:
  - implementation
  - language
  - kernel
relations:
  informed_by:
    - research-build-systems
    - research-tooling
  depends_on:
    - decision-0001-generic-kernel
    - foundation-principles
  supersedes: []
---

# implement the kernel in rust, graph by arena and index

## context

the kernel is a typed incremental computation engine with controlled effects, content-addressed storage, provenance, and concurrent evaluation. it has to be embeddable, fast enough for editor latency and large repositories, and honest about the safety claims it makes in provenance and capability tracking.

no implementation language was chosen when the design started. the open questions list names the choice as gating the first vertical slice, because no prototype can exist without it, and the language constrains the type-system and effect-syntax questions that follow.

rust, go, c, and zig were the candidates under consideration. prolog and lean are useful for adjacent work, specifically constraint solving and proving properties, but they are not candidates for the kernel itself.

## proposed decision

the kernel is implemented in rust.

rust's type system carries capability tags, effect restrictions, and provenance at compile time. pattern matching and enums fit structured diagnostics and query results. the tooling and stability are the strongest of the candidates, and the borrow checker enforces invariants the kernel claims in its public contracts.

the dependency graph is modeled as an arena of nodes indexed by integer handles, not as a structure of references. edges are indices. this is the standard way compilers, ecs libraries, and graph tools represent cyclic or shared graphs in rust, and it avoids lifetime problems without weakening safety.

`unsafe` is reserved for genuine foreign-function boundaries where the host cannot express the operation: sandbox setup, syscall interception, and similar primitives. it is never used to work around graph structure, ownership, or the borrow checker. reaching for `unsafe` to make a cyclic graph convenient would defeat the reason rust was chosen and would undercut the safety story provenance is supposed to carry.

prolog is a candidate for the package and placement solver libraries, where backtracking search over constraints is the natural fit. lean is a candidate for proving properties of the design and the core invariants. both stay outside the kernel. they do not become load-bearing in the engine, or the project inherits a multi-runtime coordination problem.

## alternatives considered

### go

go is simpler to write and has fast compilation and good concurrency primitives.

its type system is too weak for the compile-time gates the kernel wants. capability tagging, effect restriction, and provenance would become runtime checks, which is the category of error the design is trying to move earlier. generics and sum types arrived late and remain limited.

### c

c gives full control and minimal runtime overhead.

it gives up the safety guarantees that are the point of choosing a systems language here. the kernel's claims about authority, provenance, and capability control are not credible if the implementation is full of the classes of bug c permits. the manual discipline required recapitulates what rust's checker provides.

### zig

zig offers control and some safety features with a simpler model than rust.

its type system and ecosystem are less mature, and the compile-time facilities, while real, are weaker for the kind of static gating this kernel depends on. the same graph-modeling considerations apply, without the same tooling to lean on.

### rust with unsafe as an escape hatch for graph structure

the kernel could use rust but reach for `unsafe` where cyclic or shared data is inconvenient.

this is the option this decision rejects explicitly. `unsafe` as a per-case convenience is the same failure mode as priority scores in rule selection: a silent local escape that rots over time, invisible in review, and corrosive to the guarantees the rest of the system advertises. arena-and-index modeling makes it unnecessary, and reserving `unsafe` for genuine ffi keeps the boundary honest and visible.

### a multi-language kernel with prolog or lean inside

the engine could embed a prolog or lean core for the parts they fit well.

this splits the kernel across runtimes, duplicates the value and identity model across languages, and makes the capability and provenance story harder to hold together. prolog and lean are stronger as library or verification tools that operate on the kernel's typed values than as substrates for the kernel itself.

## consequences

the kernel has one implementation language and one runtime. capability tags, provenance, and effect restrictions can be compile-time facts where the type system allows.

graph work uses arenas and indices. this is a different mental model from reference-heavy graph code, and contributors need to learn it. it is well-trodden ground, but it is a real cost compared to a language where cyclic references are trivial.

`unsafe` appears only at named ffi boundaries, each one justified. a reviewer can ask of any `unsafe` block what foreign operation it enables, and there is an answer. `unsafe` to make graph code convenient has no such answer and does not get added.

the solver and verification work that prolog and lean suit are library and tooling concerns. they consume the kernel's typed values and produce results through the same extension surfaces as any other library. they do not get private access to the engine.

## unresolved

the exact crate set, the concurrency model, and the sandboxing approach need prototypes. arena-and-index is the representation; the scheduling and caching layers built on it are open.

how the kernel exposes its typed values to a prolog solver or a lean proof without crossing into a second runtime needs a concrete interface, likely through the canonical serialization the values-and-types design already requires.

the bootstrap sequence, how the first rust toolchain is built and how the kernel builds itself once it can, is a separate question for the bootstrapping milestone.
