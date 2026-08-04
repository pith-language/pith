---
schema: design-doc/v1
id: design-effects-and-capabilities
title: effects and capabilities
summary: how external work and authority enter an otherwise pure dependency graph
kind: design
status: proposed
created: 2026-04-08
updated: 2026-04-08
tags:
  - effects
  - capabilities
relations:
  informed_by:
    - research-build-systems
    - research-deployment-and-state
  depends_on:
    - decision-0003-explicit-effects
    - design-rules-and-graph
  supersedes: []
---

# effects and capabilities

the design currently distinguishes four semantic categories:

```text
Pure<A>
Action<A>
Observation<A>
Mutation<A>
```

`Pure` computes from immutable values.

`Action` runs bounded external work with declared inputs and outputs. compilers and archive tools normally live here.

`Observation` reads external state and records source, freshness, revision, and uncertainty.

`Mutation` changes external state. it carries authority, ownership, retry behavior, known failure modes, and whatever evidence can confirm completion.

the final implementation may express these through one effect calculus. their different behavior must remain available to the scheduler, cache, policy engine, and user interface.

## capabilities

effects request typed capabilities. examples include an execution platform, network access to a named endpoint, a secret reference for a particular consumer, observation of a platform, or authority to mutate an owned object.

capabilities are scoped. unrestricted network access and access to one repository are different values. secret access is not represented by passing secret bytes through ordinary configuration.

the effective capability requirements of a result propagate through composition and can be queried before execution.

## claims and enforcement

some guarantees can be enforced by a sandbox. others depend on an adapter or external platform.

the model must distinguish measured facts, enforced restrictions, and adapter claims. marking an action `hermetic` is not evidence that it was hermetic.

