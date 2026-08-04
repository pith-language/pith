---
schema: design-doc/v1
id: requirements-kernel
title: kernel requirements
summary: requirements for domain independence, evaluation, dependencies, effects, extension, provenance, and queries
kind: requirements
status: proposed
created: 2026-03-16
updated: 2026-03-16
tags:
  - requirements
  - kernel
relations:
  informed_by:
    - research-build-systems
    - research-configuration
  depends_on:
    - design-kernel
  supersedes: []
---

# kernel requirements

## K-1: domain independence

the language and engine must not contain privileged concepts for packages, services, machines, clouds, or deployments.

## K-2: typed immutable values

rules exchange typed immutable values. invalid combinations are rejected during evaluation where the available information permits it.

## K-3: pure and terminating evaluation

ordinary evaluation terminates and cannot directly read the filesystem, network, process environment, clock, randomness, credentials, or current deployment state.

## K-4: tracked dependencies

every input that can affect a cached result appears in the dependency graph. dynamic dependencies are allowed through tracked requests.

## K-5: explicit effects

external interaction occurs through typed effects. the graph distinguishes pure computation, bounded actions, observations, mutations, and opaque effectful work.

## K-6: capability control

effects require explicit scoped capabilities. authority propagates through composition and can be inspected before execution.

## K-7: deterministic rule resolution

rule and provider selection returns one explained result or an ambiguity error. load and registration order cannot select behavior.

## K-8: public extension surface

an external library can define new types, rules, effects, capabilities, planners, and adapters through the same interfaces used by first-party libraries.

## K-9: incremental correctness

incremental and cached evaluation produces a result equivalent to evaluation from an empty cache under the same declared inputs.

## K-10: persistent provenance

derived values retain their origin, dependencies, transformations, capability use, validation state, and relevant diagnostics.

## K-11: structured diagnostics

failures have stable semantic codes and structured context. rendering them for a terminal, editor, or API does not change their identity.

## K-12: queryability

the effective graph, value provenance, rule selection, invalidation reason, and capability requirements are available through a stable query API.

