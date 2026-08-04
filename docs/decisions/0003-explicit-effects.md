---
schema: design-doc/v1
id: decision-0003-explicit-effects
title: model effects and capabilities explicitly
summary: distinguish pure work, bounded actions, observations, and mutations while making authority visible
kind: decision
status: proposed
created: 2026-03-13
updated: 2026-03-13
tags:
  - effects
  - capabilities
relations:
  informed_by:
    - research-build-systems
    - research-deployment-and-state
  depends_on:
    - decision-0002-declarative-semantics
  supersedes: []
---

# model effects and capabilities explicitly

> amended by [0019: five type-level effect categories, with nondeterminism as a tracked dependency](0019-effect-categories-and-nondeterminism.md), which adds `Opaque` as a fifth, foundational category and fixes the type-level shape of all five. the four categories below stand; the amendment is recorded in 0019 and reflected in the effects-and-capabilities design doc.

## context

build commands, filesystem observations, cloud API reads, secret resolution, and deployment mutations all interact with the outside world. treating them as normal functions breaks caching and hides authority. treating them as one undifferentiated `IO` operation loses information needed by schedulers and plans.

## proposed decision

the semantic model distinguishes pure computation, bounded actions, observations, and mutations.

effects request typed scoped capabilities. capability requirements propagate through rule composition and become part of provenance.

## alternatives considered

### keep all effects outside the graph

the core could remain completely pure while separate tools perform builds and deployment.

this preserves a small evaluator. dependency tracking and provenance stop at the exact boundary where external work begins.

### one unrestricted plugin interface

extensions could call processes, filesystems, networks, and APIs through ordinary host-language code.

this is easy to extend. it prevents the engine from making reliable claims about dependencies, caching, secrets, or authority.

### one generic effect type

all external work could use one effect primitive with metadata.

this may be the implementation. the type system and scheduler still need to preserve the semantic differences between actions, observations, and mutations.

### separate effect categories

different effect types make caching and authority explicit. the cost is a larger public model and possible friction when an operation fits more than one category.

## unresolved

the exact effect calculus is open. retry safety, freshness, secret taint, compensation, and adapter claims need prototypes before this becomes accepted.

