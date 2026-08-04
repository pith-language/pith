---
schema: design-doc/v1
id: design-rules-and-graph
title: rules and graph
summary: typed requests, deterministic rule selection, and tracked dynamic dependencies
kind: design
status: proposed
created: 2026-04-06
updated: 2026-04-06
tags:
  - rules
  - incrementality
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - decision-0007-tracked-dynamic-dependencies
    - design-values-and-types
  supersedes: []
---

# rules and graph

a request asks for a typed result. a rule explains how to derive one from input values and other requests.

```text
rule compile(source: Source, compiler: Compiler) -> Artifact
```

the engine records dependencies as the rule evaluates. dependencies can depend on earlier results, but they cannot be discovered through ambient reads.

rule selection returns one explained implementation or an ambiguity error. registration order has no semantic meaning.

## one engine

evaluation, analysis, and execution use one dependency engine. conceptual stages can still exist in library APIs. they are not global barriers that prevent safe overlap.

the engine needs parallel requests, cancellation, persistent caching, equality-based change pruning, and queries explaining invalidation.

incremental execution must remain equivalent to an empty-cache evaluation under the same declared inputs.

## queries

queries are part of the semantic interface. tools should be able to ask which rule can provide a type, why a rule was selected, what a value depends on, which capabilities it requires, and why a previous result was invalidated.

query results are structured values. command-line text, editor views, and generated documentation are derived from them.

