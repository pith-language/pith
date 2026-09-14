---
schema: design-doc/v1
id: decision-0077-pre-release-pith-is-experimental
title: until the first release every pith surface is experimental and may change without notice, and its correctness and confinement guarantees hold regardless
summary: the language, builtins, manifests, cli commands and output, pith:core and the host protocol carry no compatibility promise before the first tag; the README and cli help say so; experimental status removes compatibility and nothing else, so identity, caching correctness, authorization and confinement are held to the same standard as after release
kind: decision
status: proposed
created: 2026-09-29
updated: 2026-09-29
tags:
  - planning
  - compatibility
relations:
  informed_by: []
  depends_on:
    - foundation-principles
    - requirements-security-and-trust
    - decision-0048-pre-release-version-pinning
  supersedes: []
---

# until the first release every pith surface is experimental and may change without notice, and its correctness and confinement guarantees hold regardless

## context

0048 covers pre-release compatibility for formats: every version number stays at 1, and a pre-release
incompatibility is handled by rebuilding. it says nothing about the other surfaces a user touches: the
`.pi` language and its builtins, `module.pi`, cli commands and their json output, and now `pith:core` and
the host protocol (0071, 0073).

the next milestone lets people outside this repository write domains against those surfaces. they need to
know whether what they write will keep working. before the first release it will not, and that should be
stated where they look.

## decision

### what is experimental

until the first tag (0048's trigger), these may change in any release without a migration path:

- the `.pi` grammar, builtins and elaboration rules
- the `module.pi` grammar, including `host` clauses and grants
- cli commands, flags and output, including `--output json`
- `pith:core`, the generated WIT worlds and the host protocol
- everything 0048 already covers

### where it is stated

the README says pith is experimental and has no compatibility promise. `pith --help` says the same in one
line. nothing else, such as a warning on every run, is added.

### what experimental does not change

experimental status removes compatibility and nothing else. these hold to the same standard before and
after release:

- identity and caching correctness: a cached result is never served for a computation it does not answer
- authorization of action plans (0072) and confinement of actions and components (0028, 0075)
- the refusals the records define

T-6 asks that weaker guarantees be visible. a compatibility guarantee is the only one being withheld, and
this record is where that is said.

## alternatives considered

promising compatibility for the language early would freeze decisions that the next milestone is meant to
test.

no statement at all is the current state. a user reading the README today learns that pith is a
prototype, and nothing about whether their modules will keep working.

## evidence

the README and `pith --help` carry the statement, and the help snapshot test covers it.

## unresolved

what compatibility pith promises after the first release is not decided here, beyond 0048's rules for
formats.
