---
schema: design-doc/v1
id: research-configuration
title: configuration and composition
summary: language and composition choices across Nix, Dhall, Nickel, CUE, and Starlark
kind: research
status: researching
evidence: preliminary
created: 2026-03-10
updated: 2026-03-10
tags:
  - research
  - configuration
  - types
relations:
  informed_by: []
  depends_on:
    - research-method
    - research-nix
  supersedes: []
---

# configuration and composition

configuration languages keep rediscovering the same tension. a plain data format is easy to inspect but weak at abstraction. a general-purpose language composes well but brings effects, nontermination, unstable evaluation, and a larger security boundary.

the question is not which existing syntax looks best. it is which semantic restrictions give enough abstraction without making evaluation unpredictable.

## the current candidates

Dhall makes totality, purity, typed functions, imports, and normalization central. its safety argument is that evaluation terminates and does not perform arbitrary effects. this makes configuration comparable and cacheable, though the type system and normalization model impose their own learning and implementation cost.

Nickel combines records, contracts, gradual typing, and merge-based configuration. contracts are useful at typed and untyped boundaries. its merge semantics are especially relevant because configuration is often assembled from partial definitions rather than ordinary function calls.

CUE treats configuration as constraints that narrow values. unification avoids a simple last-writer-wins model and can represent defaults separately from hard requirements. it is a useful precedent for describing a set of valid realizations.

Starlark restricts Python into deterministic configuration and extension code. Bazel and Buck2 use it because it is familiar, embeddable, and controlled. Buck2 added language-server, debugger, lint, and typechecking tools around it. Starlark's dynamic type model still leaves questions for a system that wants invalid compositions rejected early.

the Nix language demonstrates how laziness and functions can describe large dependency graphs. NixOS modules add another composition system for defaults, overrides, priorities, and recursive configuration. the power is real, but having package functions and module merges as different semantic worlds complicates tooling and understanding.

## current direction

the first-party language should be pure, terminating, and strongly typed. records, variants, generics, interfaces, and refinement should carry domain information through composition.

configuration needs more than ordinary record replacement. it needs explicit operations for combining constraints, extending collections, selecting implementations, and replacing owned behavior. those operations should be library-visible and typechecked instead of hidden in a global merge engine.

some boundaries will remain dynamic. imported legacy metadata, external schemas, and low-level adapters may only produce `Unknown` or `Unchecked<T>`. validation refines them into stronger types. the language should never claim that a runtime check already happened.

## alternatives that still need testing

- a nominal typed functional language with no built-in configuration merge
- structural records with row polymorphism
- CUE-like constraints as the main value model
- ordinary typed values plus a separate constraint library
- compile-time contracts with gradual values at import boundaries
- a small typed core language with Starlark or another language as an untrusted frontend

these choices affect editor speed, error locality, module compatibility, solver complexity, and whether libraries can define new composition behavior.

## questions

- is laziness needed beyond graph requests, or can the rule engine provide demand without lazy language values?
- should defaults be values, preferences, or low-priority constraints?
- can user-defined merge behavior remain associative and explainable?
- how are recursive module definitions represented without creating opaque fixed points?
- can schemas and runtime validators be derived from the same type definition?
- what information must survive compilation into the engine's typed intermediate representation?

## sources

- [Dhall safety guarantees](https://docs.dhall-lang.org/discussions/Safety-guarantees.html)
- [Nickel user manual](https://nickel-lang.org/user-manual/introduction/)
- [CUE configuration use case](https://cuelang.org/docs/concept/configuration-use-case/)
- [Starlark specification](https://github.com/bazelbuild/starlark/blob/master/spec.md)
- [Buck2 Starlark](https://buck2.build/docs/developers/starlark/)
- [Nix language](https://nix.dev/manual/nix/latest/language/)
- [NixOS module system](https://nixos.org/manual/nixos/stable/#sec-writing-modules)
