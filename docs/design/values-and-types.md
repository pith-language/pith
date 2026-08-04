---
schema: design-doc/v1
id: design-values-and-types
title: values and types
summary: the immutable value model and the current language constraints
kind: design
status: proposed
created: 2026-04-07
updated: 2026-05-22
tags:
  - types
  - language
relations:
  informed_by:
    - research-configuration
  depends_on:
    - decision-0010-typed-pure-language
    - design-kernel
  supersedes: []
---

# values and types

all information passed between rules is an immutable typed value. libraries can define new domain types without engine support.

the required type features currently include records, variants, generics, interfaces, opaque types, and validation that can refine an uncertain value into a stronger type.

declared configuration should make invalid combinations impossible where enough information exists. external and legacy input can remain `Unknown` or `Unchecked<T>` until validation succeeds.

## evaluation

ordinary evaluation is pure, deterministic, and terminating. filesystem reads, environment variables, clocks, randomness, processes, network access, credentials, and live state enter through explicit inputs or effects.

the graph can still be demand driven. language-level laziness is not required merely because dependencies are discovered as rules request them.

## composition

ordinary record replacement is not enough for configuration. libraries need typed operations for combining constraints, extending collections, selecting implementations, and deliberately replacing owned values.

these operations should return values with provenance. there is no global magic merge that decides conflicts from import order.

## serialization

values crossing a persistent cache or process boundary need a canonical representation and a versioned schema. local opaque values may exist when their rule contract makes the limitation clear.

the exact language and type calculus remain open. the accepted direction is stronger typing, pure evaluation, and honest dynamic boundaries.

