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
    - decision-0019-effect-categories-and-nondeterminism
    - design-rules-and-graph
  supersedes: []
---

# effects and capabilities

the design distinguishes five semantic categories:

```text
Pure<A>
Action<A>
Observation<A>
Mutation<A>
Opaque<A>
```

`Pure` computes from immutable values.

`Action` runs bounded external work with declared inputs and outputs. compilers and archive tools normally live here.

`Observation` reads external state and records source, freshness, revision, and uncertainty.

`Mutation` changes external state. it carries authority, ownership, retry behavior, known failure modes, and whatever evidence can confirm completion.

`Opaque` is effectful work that has not been modeled into one of the four categories above. it is the adoption on-ramp and the visibly distinct escape hatch: the graph records that the rule performed effectful work but does not know its category, declared inputs, or authority, and treats it as a fixed-output boundary whose interior the engine cannot inspect. decision 0019 establishes `Opaque` as foundational, not a future amendment, so the category system can be opt-in by progression rather than required up front.

the final implementation may express these through one effect calculus. their different behavior must remain available to the scheduler, cache, policy engine, and user interface.

## capabilities

effects request typed capabilities. examples include an execution platform, network access to a named endpoint, a secret reference for a particular consumer, observation of a platform, or authority to mutate an owned object.

capabilities are scoped. unrestricted network access and access to one repository are different values. secret access is not represented by passing secret bytes through ordinary configuration.

the effective capability requirements of a result propagate through composition and can be queried before execution.

## claims and enforcement

some guarantees can be enforced by a sandbox. others depend on an adapter or external platform.

the model must distinguish measured facts, enforced restrictions, and adapter claims. marking an action `hermetic` is not evidence that it was hermetic.

