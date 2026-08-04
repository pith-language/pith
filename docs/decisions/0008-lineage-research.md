---
schema: design-doc/v1
id: decision-0008-lineage-research
title: research design lineages
summary: study the pressures, alternatives, and successor reactions behind a design instead of comparing current feature lists
kind: decision
status: accepted
created: 2026-03-30
updated: 2026-03-30
tags:
  - research
  - method
relations:
  informed_by: []
  depends_on:
    - foundation-problem
  supersedes: []
---

# research design lineages

## context

the project is meant to think beyond Nix's current restrictions. copying the newest features from current tools would also copy decisions shaped by their compatibility and organizational history.

the Bazel and Buck2 example made the requirement concrete: the useful question is why their graph, rule, and remote-execution models differ, and which earlier systems influenced the rewrite.

## decision

research records the pressure, invariant, mechanism, alternatives, consequences, descendants, and later reactions for each substantial choice.

primary design documents and papers come before summaries. current marketing claims are treated as claims.

## alternatives considered

### current feature matrix

compare tools by supported languages, caches, deployment targets, and syntax.

this helps product selection. it gives weak evidence for a ground-up architecture.

### study only Nix

trace Nix's original requirements and current limitations in depth.

this is necessary and too narrow. build engines, configuration languages, solvers, deployment systems, and trust frameworks have explored adjacent choices.

### design from principles alone

derive the architecture from the project principles without historical work.

this produces a clean theory and risks repeating failures whose causes are already documented.

## consequences

research notes distinguish established facts, project inference, and open questions. uneven research depth is visible instead of smoothed over.

decision records link to the research that informed them.

