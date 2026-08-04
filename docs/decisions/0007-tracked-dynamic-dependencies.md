---
schema: design-doc/v1
id: decision-0007-tracked-dynamic-dependencies
title: allow tracked dynamic dependencies
summary: permit rules to discover dependencies through graph requests while rejecting ambient discovery
kind: decision
status: proposed
created: 2026-03-27
updated: 2026-03-27
tags:
  - dependencies
  - incrementality
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - decision-0001-generic-kernel
  supersedes: []
---

# allow tracked dynamic dependencies

## context

fully static graphs are easy to query and schedule. real builds often discover imports, generated sources, toolchain details, or platform-specific dependencies after some computation.

unrestricted filesystem tracing can discover what a command used. it makes pre-execution queries and remote planning harder, and it may record accidental access as a dependency.

Buck2, Pants, Shake, and related incremental systems allow computation to request dependencies while it runs through the engine.

## proposed decision

rules may select dependencies based on earlier typed results. every dependency request passes through the graph and carries provenance.

ambient filesystem, environment, process, or network discovery is not a valid dependency mechanism.

## alternatives considered

### static graph only

all dependencies could be declared before analysis.

this supports complete queries and predictable scheduling. it creates metadata burden and pushes language-level discovery into generators outside the model.

### unrestricted tracing

the engine could run a command, trace everything it reads, and use that as the dependency set.

this reduces declarations and can capture real access. accidental dependencies become legitimate, and the full graph is known only after execution.

### static inference

language-aware tooling could infer dependencies from imports before the build.

this is useful and should be supported. inference can be incomplete, so hermetic execution must fail when an undeclared dependency is missed.

### tracked requests

rules can suspend and request more values through the graph. this is the selected direction, with static declarations and inference available as convenient producers of requests.

## unresolved

the query model for partially discovered graphs and the persistence of dynamic edges need prototypes.

