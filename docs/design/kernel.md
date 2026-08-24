---
schema: design-doc/v1
id: design-kernel
title: the generic kernel
summary: the smallest shared substrate needed by every domain library
kind: design
status: proposed
created: 2026-04-03
updated: 2026-08-24
tags:
  - architecture
  - kernel
  - extensibility
relations:
  informed_by:
    - research-build-systems
    - research-nix
  depends_on:
    - decision-0001-generic-kernel
    - decision-0003-explicit-effects
    - decision-0004-first-party-without-privilege
    - decision-0013-managed-object-identity
  supersedes: []
---

# the generic kernel

the kernel is a typed incremental computation engine with controlled effects.

it knows how to represent values, satisfy requests through rules, track dependencies, invoke effects through capabilities, identify immutable content, cache results, and preserve provenance.

it does not know what a package, service, machine, deployment, cloud, or container is.

## inclusion rule

a feature belongs in the kernel when every domain needs one shared implementation to preserve correctness, security, composition, or inspectability.

putting less in the kernel would force libraries to build incompatible dependency graphs, caches, effect models, and identities. putting more in it would turn domain assumptions into permanent engine semantics.

## kernel responsibilities

- typed immutable values and their canonical boundary representation
- canonical declared types and typed rule interfaces
- typed requests and deterministic rule resolution
- dynamic dependency tracking
- incremental evaluation, concurrency, cancellation, and caching
- explicit effects and scoped capabilities
- semantic, computation, content, external, and managed-object identity primitives
- immutable blob, tree, and structured-value storage
- provenance, structured diagnostics, and graph queries

## library responsibilities

constraint algorithms, version selection, transition planning, filesystem models, package conventions, service semantics, and deployment policies are libraries.

the kernel can provide protocols these libraries use. it should not choose one package solver or define what a rollout means.

## extension boundary

libraries define types, rules, effects, capabilities, planners, and adapters through public interfaces. first-party libraries use the same interfaces.

there is no global mutable plugin registry. the loader resolves explicit imports before the kernel receives declarations and rules. typed provider selection determines which registered implementation answers a request; ambiguity is an error with an explanation.

## design test

a new domain such as an FPGA toolchain, scientific workflow, firmware manager, or documentation system should be implementable without a core patch.

if it needs new universal effect semantics or a correctness invariant shared by every domain, a kernel change may be justified. if it only needs a new domain noun, the library interface is the right place.
