---
schema: design-doc/v1
id: requirements-security-and-trust
title: security and trust requirements
summary: requirements for secret references, authority, provenance, attestations, and visible unsafe boundaries
kind: requirements
status: proposed
created: 2026-03-19
updated: 2026-03-19
tags:
  - requirements
  - security
relations:
  informed_by:
    - research-artifacts-and-trust
  depends_on:
    - design-effects-and-capabilities
    - design-identity-and-storage
  supersedes: []
---

# security and trust requirements

## T-1: typed secret references

ordinary values contain secret handles, not plaintext. handles declare scope and intended consumer.

## T-2: late binding

secret bytes are resolved as late as the target permits and do not enter source, content digests, ordinary caches, plans, or logs.

## T-3: least authority

rules and adapters receive only the capabilities required for the current request. capability scopes are part of provenance.

## T-4: derived supply-chain evidence

dependency manifests, provenance attestations, signatures, and software bills of materials are derived from the graph used to construct artifacts.

## T-5: separate trust claims

content signatures, reproducibility, dependency policy, builder identity, and deployment evidence remain distinct claims.

## T-6: visible loss of guarantees

unsafe actions and low-level adapters are available when needed. weaker guarantees appear in types, plans, queries, and user interfaces.

## T-7: replaceable boundary implementations

external dependencies and platform capabilities can be replaced with deterministic test handlers.

