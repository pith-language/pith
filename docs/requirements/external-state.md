---
schema: design-doc/v1
id: requirements-external-state
title: external-state requirements
summary: requirements for observations, ownership, plans, transitions, and partial failure
kind: requirements
status: proposed
created: 2026-03-18
updated: 2026-03-18
tags:
  - requirements
  - state
  - deployment
relations:
  informed_by:
    - research-deployment-and-state
  depends_on:
    - design-effects-and-capabilities
  supersedes: []
---

# external-state requirements

## S-1: explicit observations

observed values record their source, revision or observation time, freshness, and uncertainty.

## S-2: honest unknowns

unavailable or incomplete information remains represented as unknown, unreachable, stale, conflicted, or unchecked. it is never silently replaced with a normal value.

## S-3: explicit ownership

mutations declare which external state they own. absence from one declaration does not authorize deletion of unowned state.

## S-4: derived plans

users state desired values, constraints, and safety properties. planners derive operation sequences and expose destructive effects, temporary states, assumptions, and rollback limits.

## S-5: transition contracts

changes with important intermediate states can declare preconditions, invariants, compatibility windows, completion evidence, and reversibility.

## S-6: partial failure

plans expose transaction boundaries. the system does not claim atomicity across platforms that cannot provide it.

## S-7: expected outcomes and faults

known failures are part of rule, observation, plan, and mutation contracts. engine defects and adapter protocol violations cross a separate fault boundary.

## S-8: execution strategy independence

one-shot application and continuous reconciliation can execute the same desired-state and transition semantics.

