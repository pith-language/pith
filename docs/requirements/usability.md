---
schema: design-doc/v1
id: requirements-usability
title: usability requirements
summary: requirements for common workflows, adoption, diagnostics, tooling, compatibility, and first-party libraries
kind: requirements
status: proposed
created: 2026-03-20
updated: 2026-03-20
tags:
  - requirements
  - usability
relations:
  informed_by:
    - foundation-principles
    - research-build-systems
  depends_on:
    - design-first-party-domains
  supersedes: []
---

# usability requirements

## U-1: short common path

common builds and environments require little metadata. defaults and inference are deterministic, inspectable, and fail closed when ambiguous.

## U-2: gradual adoption

existing projects can adopt the tool around a subset of their build or deployment. unmodeled boundaries remain explicit.

## U-3: editor support from semantics

formatting, navigation, completion, diagnostics, documentation, and refactoring use the same parsed and typed model as evaluation.

## U-4: semantic compatibility

public meanings have versions and migrations. internal implementation changes do not silently change existing definitions.

## U-5: first-party build library

the official build library supports multi-language targets, generated inputs, toolchains, tests, checks, fine-grained invalidation, local execution, remote execution, and graph queries.

## U-6: first-party package library

the official package library supports multiple versions, variants, constraints, feature selection, lock data, source and binary distribution, and resolution explanations.

## U-7: first-party environment library

the official environment library composes toolchains, packages, commands, variables, and local services without mutating global host configuration.

## U-8: first-party system library

the official system library models filesystems, users, services, mounts, devices, networking, boot, persistent data, and operating-system composition as typed values.

## U-9: first-party deployment library

the official deployment library derives plans from desired values, observations, ownership, and safety constraints.

## U-10: no first-party privilege

each first-party domain library uses public kernel interfaces. tests prove that an external library can replace or extend it without hidden hooks.

