---
schema: design-doc/v1
id: decision-0010-typed-pure-language
title: use a typed, pure, terminating declaration language
summary: choose strong static information and controlled boundaries while leaving the exact type and constraint calculus open
kind: decision
status: proposed
created: 2026-04-01
updated: 2026-04-01
tags:
  - language
  - types
relations:
  informed_by:
    - research-configuration
    - research-nix
  depends_on:
    - decision-0002-declarative-semantics
  supersedes: []
---

# use a typed, pure, terminating declaration language

## context

Nix demonstrates expressive lazy functional configuration, but dynamic typing and late evaluation can move errors far from their source. Starlark offers controlled familiar extension code with dynamic types. Dhall makes totality and typing central. Nickel adds contracts and merge semantics. CUE treats values as constraints.

the project wants typed composition, editor support, and predictable evaluation while still accepting legacy and partially known input.

## proposed decision

the first-party language is strongly typed, pure, deterministic, and terminating for ordinary declaration evaluation.

dynamic or unvalidated boundaries produce explicit weaker types. validation can refine them.

the language compiles into the kernel's typed semantic representation. source syntax is not the engine API.

## alternatives considered

### lazy dynamically typed functional language

this gives concise recursive composition and follows Nix's proven model.

errors and semantic information arrive late. tooling has to approximate evaluation behavior.

### restricted dynamic scripting language

a Starlark-like language is familiar, embeddable, and practical for extensions.

runtime checks and external tooling have to recover type information that could otherwise guide composition and rule selection.

### constraint language as the whole model

a CUE-like language can describe acceptable values and make conflict detection central.

general transformations, abstraction boundaries, effects, and large program structure may become harder to express and optimize.

### total typed functional language

a Dhall-like direction gives strong normalization and safety guarantees.

the type system, constraint composition, and ergonomics for large domain libraries still need proof.

### typed value language with constraint libraries

ordinary typed values form the language core. libraries add constraints and solver interfaces where domains need them.

this is the current preferred direction. the exact treatment of refinements, recursion, defaults, and user-defined composition is open.

## unresolved

nominal versus structural typing, termination checking, row polymorphism, refinement performance, schema evolution, and module compatibility require separate decisions.

