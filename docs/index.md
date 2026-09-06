---
schema: design-doc/v1
id: documentation-index
title: documentation
summary: map of the project foundation, current design, requirements, research, decisions, and planning notes
kind: index
status: active
created: 2026-02-24
updated: 2026-09-06
tags:
  - documentation
relations:
  informed_by: []
  depends_on: []
  supersedes: []
---

# documentation

the repository separates current design from the evidence and decisions that produced it.

## foundation

- [problem](foundation/problem.md)
- [scope](foundation/scope.md)
- [principles](foundation/principles.md)
- [name and brand](foundation/name.md)
- [glossary](foundation/glossary.md)

## design

- [overview](design/overview.md)
- [kernel boundary](design/kernel.md)
- [values and types](design/values-and-types.md)
- [rules and graph](design/rules-and-graph.md)
- [effects and capabilities](design/effects-and-capabilities.md)
- [identity and storage](design/identity-and-storage.md)
- [first-party domains](design/first-party-domains.md)

## brand

- [identity process](brand/process.md)
- [identity system](brand/system.md)

## requirements

- [requirements index](requirements/index.md)
- [kernel](requirements/kernel.md)
- [composition](requirements/composition.md)
- [actions and artifacts](requirements/actions-and-artifacts.md)
- [external state](requirements/external-state.md)
- [security and trust](requirements/security-and-trust.md)
- [usability](requirements/usability.md)

## evidence and history

- [research](research/index.md)
- [decisions](decisions/index.md)

## planning

planning notes are named for their subject. the order the work runs in lives in one place, the
milestones, so that renaming a milestone never renames a design note.

- [open questions](planning/open-questions.md)
- [milestones](planning/milestones.md)
- [what the completed milestones measured](planning/measured.md)
- [the reordering](planning/reordering.md)
- [the cli surface](planning/cli-surface.md)
- [repository history](planning/repository-history.md)

### the frontend

- [the language frontend](planning/frontend/language.md)
- [the frontend architecture](planning/frontend/architecture.md)
- [the surface notation](planning/frontend/surface-notation.md)

### modules

- [the module system](planning/modules/system.md)
- [the module surface](planning/modules/surface.md)
- [module workspaces](planning/modules/workspaces.md)
- [module distribution](planning/modules/distribution.md)
- [the module registry](planning/modules/registry.md)
- [registry integration](planning/modules/registry-integration.md)
