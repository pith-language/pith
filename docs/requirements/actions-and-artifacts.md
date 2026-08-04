---
schema: design-doc/v1
id: requirements-actions-and-artifacts
title: actions and artifacts requirements
summary: requirements for hermetic work, immutable content, caching, and executor independence
kind: requirements
status: proposed
created: 2026-03-17
updated: 2026-03-17
tags:
  - requirements
  - actions
  - artifacts
relations:
  informed_by:
    - research-build-systems
    - research-artifacts-and-trust
  depends_on:
    - design-rules-and-graph
    - design-identity-and-storage
  supersedes: []
---

# actions and artifacts requirements

## A-1: declared action contract

an action declares its executable, arguments, inputs, outputs, environment, platform requirements, capabilities, and network policy.

## A-2: undeclared access fails

executors prevent or detect access outside the declared action contract.

## A-3: content-addressed data

immutable blobs, trees, and serializable values use content identities that include every declared input affecting their contents.

## A-4: deferred materialization

the engine can reason about and pass an immutable object without materializing it locally until a consumer requires the bytes.

## A-5: executor equivalence

local and remote executors implement the same action semantics. changing executor does not require rewriting a rule.

## A-6: clean-build equivalence

incremental, cached, local, and remote execution produce results equivalent to a clean execution under the same declared inputs and platform contract.

## A-7: derived evidence

artifact dependencies and action provenance come from the graph that produced the artifact. they are not separately maintained declarations.

## A-8: multidimensional performance

benchmarks cover startup, warm evaluation, cold builds, incremental builds, memory, local I/O, network transfer, editor latency, and graph-query latency.

## A-9: verified-reproducible status

bit-for-bit reproducibility is a property of the build instructions and environment, not of the engine. the engine verifies it by building twice under the same declared inputs and comparing content identities, records the result in provenance, and refuses to assert it when unverified or when the only evidence is an attestation whose trust state is not established. the distinction between "built once" and "verified reproducible" is visible in provenance and in any supply-chain attestation derived from it.

