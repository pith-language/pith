---
schema: design-doc/v1
id: requirements-composition
title: composition requirements
summary: requirements for canonical definitions, conflict handling, identity, and information preservation
kind: requirements
status: proposed
created: 2026-03-20
updated: 2026-03-20
tags:
  - requirements
  - composition
relations:
  informed_by:
    - foundation-principles
    - research-configuration
  depends_on:
    - design-values-and-types
  supersedes: []
---

# composition requirements

## C-1: canonical definitions

dependent representations are generated from canonical semantic values. manually synchronized copies are not part of the normal workflow.

## C-2: explicit conflicts

composition has deterministic rules. conflicting values fail unless an explicit operation handles the conflict.

## C-3: deliberate replacement

replacement identifies the target and expected owner. replacing a value that has changed ownership fails.

## C-4: preserved information

composition retains types, provenance, diagnostics, constraints, and trust information unless a public operation explicitly discards them.

## C-5: separate identities

semantic, computation, content, and external identities are distinct types.

## C-6: cross-boundary composition

typed values and contracts compose across files, packages, repositories, and process boundaries without degrading into untyped maps or strings.

## C-7: predictable inheritance

when a library offers inheritance or scoped configuration, precedence and merge behavior are deterministic and queryable.

## C-8: derived views

documentation, schemas, clients, lock data, and tool views derive from canonical definitions where those representations describe the same meaning.

