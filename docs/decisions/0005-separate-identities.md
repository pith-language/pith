---
schema: design-doc/v1
id: decision-0005-separate-identities
title: separate identity types
summary: use different types for semantic objects, computations, immutable content, and external objects
kind: decision
status: proposed
created: 2026-03-25
updated: 2026-03-25
tags:
  - identity
  - provenance
relations:
  informed_by:
    - research-nix
    - research-artifacts-and-trust
    - research-deployment-and-state
  depends_on:
    - foundation-principles
  supersedes: []
---

# separate identity types

> amended by [0013: managed-object identity](0013-managed-object-identity.md), which adds a fifth identity type. the four types below stand; the amendment is recorded in 0013 and reflected in the glossary and identity-and-storage design doc.

## context

a content digest answers whether immutable bytes are equal. it does not say that two builds represent the same package across platforms. a configuration address can name desired meaning, while a cloud provider assigns another identifier to the current remote object.

using one identifier for all of these makes refactors, replacement, adoption, caching, and provenance interfere with each other.

## proposed decision

the core distinguishes semantic identity, computation identity, content identity, and external identity.

provenance records relations between them. the type system prevents accidental substitution.

## alternatives considered

### content identity for everything

every meaningful object could be its hash.

this is excellent for immutable storage. meaning changes whenever representation changes, and mutable external objects cannot be addressed naturally.

### source address as identity

module path and declaration name could define identity.

this is easy to understand. moving code becomes a semantic replacement unless a separate migration mechanism restores continuity.

### provider identity as truth

external platforms could supply the durable identity for deployed resources.

this works inside one provider. it makes semantic identity depend on a selected realization and complicates migration between implementations.

## unresolved

the exact construction and migration rules for semantic identities remain open. implicit IDs are convenient, while explicit IDs survive refactors more reliably.

