---
schema: design-doc/v1
id: decision-0002-declarative-semantics
title: declarations denote values and constraints
summary: users describe results and acceptable states while plans and operation sequences are derived
kind: decision
status: accepted
created: 2026-03-12
updated: 2026-03-12
tags:
  - declarative
  - semantics
relations:
  informed_by:
    - research-deployment-and-state
    - research-nix
  depends_on:
    - foundation-principles
  supersedes: []
---

# declarations denote values and constraints

## context

Ansible playbooks expose ordered tasks. idempotence usually lives inside each module.

Terraform configuration is declarative at the surface, but provider resource types define much of the meaning through create, read, update, and delete behavior. persistent state binds configuration addresses to remote objects.

neither model is the intended center of this project.

## decision

ordinary declarations evaluate to typed values, constraints, and requests. they do not prescribe an operation sequence.

a domain can interpret those values into artifacts, acceptable external states, or another result. when external mutation is needed, observations and desired values are inputs to a planner. the plan is a derived inspectable value.

## alternatives considered

### ordered tasks

users could describe the exact procedure, with idempotent task implementations making repetition safe.

this is practical for arbitrary machines. ordering, rollback, and hidden dependencies become the author's responsibility.

### provider resources

users could declare resources whose provider implementations own lifecycle behavior.

this produces useful plans and broad integration. provider vocabulary and CRUD behavior become public semantics, which makes cross-platform meaning difficult to preserve.

### desired API objects with controllers

users could submit objects and rely on continuous controllers to move observed state toward them.

this handles changing systems well. behavior becomes distributed across schemas and controllers, and an always-running control plane becomes the assumed execution model.

### values and acceptable states

declarations can denote exact values or constraints over valid realizations. separate planners derive transitions when a domain needs them.

this is the selected direction.

## consequences

some operations need temporal contracts. a database migration cannot be described safely by its final schema alone. libraries must be able to define preconditions, intermediate invariants, completion evidence, and rollback limits.

procedural escape hatches remain available as explicit mutations with reduced guarantees.

