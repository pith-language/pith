---
schema: design-doc/v1
id: decision-0001-generic-kernel
title: use a generic semantic kernel
summary: keep domain nouns out of the engine while sharing the mechanisms needed for composition and correctness
kind: decision
status: accepted
created: 2026-03-11
updated: 2026-03-11
tags:
  - kernel
  - architecture
relations:
  informed_by:
    - research-nix
    - research-build-systems
  depends_on:
    - foundation-problem
  supersedes: []
---

# use a generic semantic kernel

## context

the first scope described the project as a system compiler that also built artifacts and deployed systems. this made operating-system configuration the center of the model.

the next version described an integrated build, package, system-management, and deployment product. that fixed the product scope, but still risked putting today's domain concepts into the engine.

## decision

the engine is a typed incremental computation kernel. it owns values, rules, dependencies, effects, capabilities, identity, immutable storage, provenance, diagnostics, and queries.

packages, services, systems, and deployments are library-defined types and rules.

## alternatives considered

### system compiler

the whole system could compile a world specification into machines and deployments. builds and packages would be subordinate stages.

this gives a coherent story for operating systems. it makes standalone builds awkward and encourages system concepts to leak into unrelated domains.

### integrated domain-specific engine

the engine could have first-class nodes for builds, packages, services, machines, and deployments.

this would make the initial product easier to implement. extensions would be limited to the categories anticipated by the engine, and adding a new domain would require core work.

### language runtime only

the core could stop at a typed programming language and let libraries implement every other mechanism.

this is flexible in syntax and weak in composition. separate libraries would recreate dependency tracking, caching, authority, identity, and provenance in incompatible ways.

## consequences

the kernel boundary has to be justified through shared invariants. it cannot become a hiding place for useful first-party behavior.

domain libraries need a strong enough public API to build complete tools. this is a harder extension design than a conventional plugin registry.

the first-party domains become tests of the kernel. if one needs a private hook, either the kernel is missing a generic mechanism or the library is relying on an accidental privilege.

