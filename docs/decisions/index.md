---
schema: design-doc/v1
id: decisions-index
title: decisions
summary: chronological record of accepted and proposed architectural choices
kind: decision
status: active
created: 2026-03-11
updated: 2026-08-18
tags:
  - decisions
relations:
  informed_by: []
  depends_on:
    - foundation-problem
  supersedes: []
---

# decisions

decision records preserve the alternatives and the reason for choosing among them. accepted records describe the current direction. proposed records contain a preferred direction that still needs research or a prototype.

when an accepted decision changes, a new record supersedes it. the old record stays in the repository.

## accepted

- [0001: use a generic semantic kernel](0001-generic-kernel.md)
- [0002: declarations denote values and constraints](0002-declarative-semantics.md)
- [0004: first-party without privilege](0004-first-party-without-privilege.md)
- [0008: research design lineages](0008-lineage-research.md)
- [0009: keep first-party domains as peers](0009-peer-first-party-domains.md)
- [0011: separate documentation by role](0011-document-structure.md)

## proposed

- [0003: model effects and capabilities explicitly](0003-explicit-effects.md)
- [0005: separate identity types](0005-separate-identities.md)
- [0006: target Linux first without putting Linux in the kernel](0006-linux-first.md)
- [0007: allow tracked dynamic dependencies](0007-tracked-dynamic-dependencies.md)
- [0010: use a typed, pure, terminating declaration language](0010-typed-pure-language.md)
- [0012: revision-pinned plans](0012-revision-pinned-plans.md)
- [0013: managed-object identity](0013-managed-object-identity.md)
- [0014: separate the reproducibility properties](0014-reproducibility-properties.md)
- [0015: select rules by interface match and refuse ambiguity](0015-interface-rule-selection.md)
- [0016: implement the kernel in rust, graph by arena and index](0016-implementation-language.md)
- [0017: structural types by default, nominal by declaration](0017-structural-with-nominal.md)
- [0018: total pure evaluation by construction, with cycle detection and a backstop limit](0018-termination-and-recursion.md)
- [0019: five type-level effect categories, with nondeterminism as a tracked dependency](0019-effect-categories-and-nondeterminism.md)
- [0020: reuse Nix infrastructure as adapters, not as the substrate](0020-nix-as-adapter-not-substrate.md)

note: 0013 amends 0005 to add a fifth identity type. 0005 stands; the amendment is recorded in 0013.

