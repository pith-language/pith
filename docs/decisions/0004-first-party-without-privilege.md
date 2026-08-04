---
schema: design-doc/v1
id: decision-0004-first-party-without-privilege
title: first-party without privilege
summary: official domain libraries use the same extension interfaces available to external code
kind: decision
status: accepted
created: 2026-03-24
updated: 2026-03-24
tags:
  - extensibility
  - libraries
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - decision-0001-generic-kernel
  supersedes: []
---

# first-party without privilege

## context

build systems have often implemented common language rules inside the engine and exposed a narrower API to users. Buck2's rewrite moved language rules to Starlark and made advanced features available through one rule interface.

the project still needs a maintained and coherent default experience. making everything external without official libraries would move basic design work onto every user.

## decision

build, package, environment, system, deployment, secret, and policy libraries ship as first-party components.

they use public types, rules, effects, capabilities, and adapters. external libraries can access the same mechanisms under the same authority checks.

## alternatives considered

### privileged built-ins

common domains could be implemented directly in the engine for speed and integration.

this makes the first version easier. it freezes domain policy into the kernel and leaves extensions with weaker composition and tooling.

### external plugins with a smaller API

official behavior could use private internals while third parties receive a stable plugin interface.

this is common and practical. it means the official implementation is no longer proof that the extension model is complete.

### no official domains

the project could publish only the kernel.

this maximizes neutrality and produces a research engine instead of a useful replacement for existing tools.

## consequences

first-party code becomes an extension conformance suite. compatibility guarantees apply to the public extension surface.

performance-sensitive behavior may require new generic primitives. those additions need justification independent of the official library asking for them.

