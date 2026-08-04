---
schema: design-doc/v1
id: foundation-glossary
title: glossary
summary: working meanings for terms used across the design
kind: foundation
status: draft
created: 2026-03-25
updated: 2026-03-25
tags:
  - glossary
relations:
  informed_by: []
  depends_on:
    - foundation-problem
  supersedes: []
---

# glossary

these definitions are part of the design. ambiguous words should be split instead of stretched until they cover unrelated concepts.

## action

a bounded external computation with declared inputs and outputs. a compiler invocation is an action.

## artifact

a domain value backed by immutable content. the kernel stores content; a library gives that content artifact meaning.

## capability

typed authority or behavior required by a rule or effect. capabilities can describe execution, network access, a platform, secret access, observation, or mutation.

## declaration

source that evaluates to typed values, rules, constraints, or requests. ordinary declaration evaluation does not perform external effects.

## effect

an explicitly represented interaction whose result cannot be derived from ordinary immutable inputs alone.

## computation identity

the identity of a specific rule application and the inputs relevant to its result. two builds of the same semantic value under different inputs have different computation identities.

## content identity

the identity of immutable bytes or a canonical structured value, given by digest. two values with the same content share content identity by construction.

## external identity

the identity assigned to an object by a system outside this tool.

## kernel

the shared engine for values, rules, dependency tracking, effects, identity, immutable storage, provenance, and diagnostics.

## managed-object identity

the identity of the durable external object a deployment owns and mutates across observations, mutations, and platform re-creation. distinct from external identity, which is an identifier the platform assigns and which can change while the managed object persists. constructed and maintained by the deployment library and its adapters; the kernel provides the primitive and provenance machinery.

## mutation

an effect that changes external state.

## observation

a time- or revision-bound statement about external state, including its source and uncertainty.

## realization

one concrete value or arrangement satisfying a declaration and its constraints.

## rule

a typed recipe for deriving a value from inputs and requests tracked by the graph.

## semantic identity

the stable identity of what a value represents, independent of its current contents, file location, or external provider address.

## world

a domain-level collection of desired values and constraints. `World` is not currently a kernel primitive.

