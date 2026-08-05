---
schema: design-doc/v1
id: project-readme
title: pith
summary: entry point for the design notebook
kind: index
status: active
created: 2026-08-04
updated: 2026-08-05
tags:
  - project
  - design
relations:
  informed_by: []
  depends_on: []
  supersedes: []
---

# pith

this repository is the design notebook for **Pith**, a system built from the ideas that made Nix useful, without treating Nix's current architecture as a constraint.

the rough idea is a typed, incremental, capability-controlled computation kernel. builds, packages, development environments, services, system management, deployment, and other automation models are libraries built on that kernel. the first-party libraries should be useful enough to make a complete tool, but they do not get private access to the engine.

## start here

- [documentation index](docs/index.md) is the complete map.
- [name and brand](docs/foundation/name.md) defines the project's name, word forms, file extension, store paths, ecosystem vocabulary, forge strategy, and domain plan.
- [scope](docs/foundation/scope.md) defines what the project owns and where it stops.
- [principles](docs/foundation/principles.md) contains the design rules that constrain the architecture.
- [kernel architecture](docs/design/kernel.md) draws the boundary between the generic engine and domain libraries.
- [requirements](docs/requirements/index.md) turns the discussion into requirements that can eventually be tested.
- [research](docs/research/index.md) records what existing systems tried, why they made those choices, and what later systems changed.
- [decisions](docs/decisions/index.md) contains the decisions made so far.
- [open questions](docs/planning/open-questions.md) is where unresolved design work lives.

## current position

accepted so far:

- domain concepts are library types, not language keywords or hidden engine concepts
- configuration denotes values, constraints, and acceptable states; it does not prescribe an operation sequence
- first-party code uses the same extension surface as third-party code
- build, package, system, and deployment functionality share one graph and provenance model without being forced into one domain hierarchy

the proposed design also separates pure computation, bounded actions, observations, and mutations, with an `Opaque` category for unmodeled effectful work; makes authority explicit through capabilities; permits tracked dynamic dependencies; and distinguishes semantic, computation, content, external, and managed-object identity.

further proposed decisions settle some of the open questions: rule selection matches typed interfaces and refuses ambiguity (0015); the kernel is implemented in rust with arena-and-index graph modeling and no unsafe for structure (0016); types are structural by default and nominal by declaration (0017); pure evaluation is total by construction, with the graph carrying recursion and cycle detection providing the real bound (0018); and the kernel has five type-level effect categories with nondeterminism tracked as a dependency (0019).

the M-1 semantic prototype is complete. the rust prototype tests exact-interface selection over the current interface subset, the five effect-category types, arena-owned graph identity, exact in-memory reuse of completed pure applications, versioned pure-computation keys over separate stable rule identity and cache-invalidating rule revision, validated and stably digested action contracts, propagated action capability requirements, actual capability-use edges, the synchronous pure step machine, and an inspectable action-rule/policy/executor boundary. every scheduled run supplies an action policy; authorization happens before execution and is retained in provenance. before execution, the engine materializes only the executable and inputs declared by the validated action contract; after execution, the engine imports captured output bytes and trees into its own store before action completion sees content identities. action results deliberately remain non-reusable until cache identity includes the resolved platform and complete execution semantics, with current policy reapplied on reuse.

the exact constraint model and persistent incremental engine are still open design work. persistent graph and cache storage, equality-based change pruning, invalidation explanations, operational support for the remaining effect categories, sandbox enforcement, and concurrent scheduling remain unproved. proposed decisions stay open until later milestones test their broader claims; completing a scoped prototype milestone does not accept those decisions in full.

## document status

`accepted` means the direction has been chosen for now. it can still be replaced by a later decision record.

`proposed` means there is a concrete position worth arguing about.

`researching` means the document mostly contains evidence and questions.

`active` means a living reference document (an index, glossary, source ledger, or open-question list) that is always current rather than accepted, proposed, or under research.

`draft` means the structure or wording is incomplete.
