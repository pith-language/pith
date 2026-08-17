---
schema: design-doc/v1
id: project-readme
title: pith
summary: entry point for the design notebook
kind: index
status: active
created: 2026-08-04
updated: 2026-08-17
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

## building and testing

the repository builds inside a nix devshell, which pins the rust toolchain and the tools the checks use.

```
nix develop
just
```

`just` with no recipe lists the rest. `just test` runs the suite, `just check` is format plus clippy, and `just ci` is everything the github workflow gates plus the checks it does not yet run.

the linux-gated fixtures are the evidence base for most of what the milestones claim, and they compile to nothing off linux — on a darwin host those suites report zero tests rather than failing, so a green local run there is not a measured one.

## current position

milestones M-1 through M-4 are complete at their own scopes, and [milestones](docs/planning/milestones.md) carries the evidence for each. M-5a, linux system composition, is next. the workspace is twelve crates: the kernel as `pith-core`, `pith-engine`, `pith-arena`, `pith-ids`, `pith-store`, `pith-state-sqlite`, `pith-diag`, `pith-output`, `pith-executor-local` and the `pith` cli, with two domain libraries beside them — `xylem` for builds and `phloem` for packages and environments.

accepted so far:

- domain concepts are library types, not language keywords or hidden engine concepts
- configuration denotes values, constraints, and acceptable states; it does not prescribe an operation sequence
- first-party code uses the same extension surface as third-party code
- build, package, system, and deployment functionality share one graph and provenance model without being forced into one domain hierarchy
- a consumer of an action revalidates by re-planning it (0033), the written lock is a text projection whose write is a caller effect (0041), and the local executor confines an action with landlock and seccomp (0028)

the proposed design separates pure computation, bounded actions, observations, and mutations, with an `Opaque` category for unmodeled effectful work; makes authority explicit through capabilities; permits tracked dynamic dependencies; and distinguishes semantic, computation, content, external, and managed-object identity. rule selection matches typed interfaces and refuses ambiguity (0015); the kernel is rust with arena-and-index graph modeling and no unsafe for structure (0016); pure evaluation is total by construction, with the graph carrying recursion (0018); and the type calculus is one closed structural set with nominal identity by declaration (0026, superseding 0017), whose declaration site 0047 builds.

what a build does today, end to end: the engine plans an action contract from a request, materializes only the declared executable and inputs, runs the child under a landlock ruleset and a 77-entry seccomp allowlist so it reports `AccessVerification::Prevented`, imports the captured outputs into its own content store, and keys the attempt by the request rather than the execution (0031). an unchanged compile is served from the reusable index within a run and across runs; a second build of unchanged sources reuses at the root, and a fresh engine over the same sqlite state hydrates it without allocating beneath the root. touching one source recompiles that object and re-links while the other object's compile is served. two toolchains share one graph, header dependencies are discovered by their own action rather than declared, and a test's exit status is a declared outcome so a failing test is a reusable verdict.

what is built and unproved, which the milestones state per item: the `Observation`, `Mutation`, and `Opaque` effect categories are marker types with no step variant, scheduler path, or durable record, so three of the five categories have no operational support. retention and garbage collection are framed (0027) and unbuilt — nothing in the tree deletes a byte. the durable half of invalidation explanation is built and held by the cross-adapter conformance suite; the live query surface has no caller, and its reason taxonomy names configuration rather than change, so it does not yet answer why something recomputed. there is no surface language: every declaration is a rust api call, and the frontend is gated on the 0026 calculus landing rather than on a milestone.

[0047](docs/decisions/0047-the-declaration-table.md) is built. a nominal type carries its declaration rather than a bare name, so `is_type` verifies that a value naming a coordinate also holds a representation the declaration admits — a `Text` claiming to be `xylem.Object` no longer inhabits a link interface, which `crates/xylem/tests/declaration_hole.rs` asserted for as long as it did. declarations live in a per-module table that refuses a duplicate name and a recursive alias, a recursive nominal is finite through a cut, and a rule's revision derives from the declarations its interface names, which retired the hand-bumped revision constant every domain library shipped. under [0048](docs/decisions/0048-pre-release-version-pinning.md) no encoding version moved for it: every version stays at 1 until the first release, and the pre-release database is discarded and rebuilt.

the next implementation items are the ones 0047 and [0049](docs/decisions/0049-pure-edge-revalidation.md) left open — arbitrary-precision `Int` with the arithmetic that needs it, transitive revalidation, and a generated from-scratch-consistency harness — and M-5a, the first domain whose shapes the calculus was not extended for.

proposed decisions stay open until later milestones test their broader claims. completing a scoped prototype milestone does not accept those decisions in full.

## document status

`accepted` means the direction has been chosen for now. it can still be replaced by a later decision record.

`proposed` means there is a concrete position worth arguing about.

`researching` means the document mostly contains evidence and questions.

`active` means a living reference document (an index, glossary, source ledger, or open-question list) that is always current rather than accepted, proposed, or under research.

`draft` means the structure or wording is incomplete.
