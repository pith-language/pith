---
schema: design-doc/v1
id: planning-repository-history
title: repository history
summary: why the forge's commit graph does not date the work, and what to use instead
kind: planning
status: draft
created: 2026-08-31
updated: 2026-08-31
tags:
  - planning
  - history
relations:
  informed_by: []
  depends_on:
    - planning-measured
  supersedes: []
---

the public forge's git history begins on 2026-08-04. the notebook's earliest
documents are dated 2026-02-24. the gap is a tooling artifact, not a rebuilt
history: adopting jujutsu collapsed the commit timestamps of most existing
commits onto the operation's date, and the operation ran more than once before
the effect was noticed. the commits are original; their dates are not reliable
evidence of when the work happened.

dating the work: use the notebook. documents carry front-matter dates, every
closed milestone records its measurement when it closes, and
`docs/planning/measured.md` is append-only. those are the timestamps the
project stands behind, and they are what a reviewer should use.
