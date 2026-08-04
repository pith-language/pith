---
schema: design-doc/v1
id: requirements-index
title: requirements
summary: index of behavioral requirements that will later become executable acceptance tests
kind: requirements
status: proposed
created: 2026-03-16
updated: 2026-03-16
tags:
  - requirements
relations:
  informed_by:
    - foundation-scope
    - foundation-principles
  depends_on:
    - design-overview
  supersedes: []
---

# requirements

these requirements describe the intended system. they are grouped by shared mechanism rather than product command.

- [kernel](kernel.md)
- [composition](composition.md)
- [actions and artifacts](actions-and-artifacts.md)
- [external state](external-state.md)
- [security and trust](security-and-trust.md)
- [usability](usability.md)

each requirement needs an executable acceptance test or a narrower specification before implementation begins. identifiers stay stable when wording changes. a semantic change gets a new requirement or an explicit migration.

