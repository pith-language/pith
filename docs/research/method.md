---
schema: design-doc/v1
id: research-method
title: research method
summary: how design lineages are researched and turned into project decisions
kind: research
status: accepted
evidence: reviewed
created: 2026-02-27
updated: 2026-02-27
tags:
  - research
  - method
relations:
  informed_by: []
  depends_on:
    - decision-0008-lineage-research
  supersedes: []
---

# research method

a current feature comparison tells us what survived. it usually does not tell us why the feature exists or which assumption made it reasonable.

every substantial design question should get a lineage record with the following fields.

## pressure

what failed or became too expensive? include the scale, organization, hardware, language ecosystem, and operational environment when they mattered.

## invariant

what did the designers refuse to give up? examples include clean-build equivalence, graph queryability, local usability, repository scale, or compatibility with an existing workflow.

## mechanism

what did the system actually implement? this section should be specific enough to distinguish a semantic choice from an implementation accident.

## alternatives

which credible alternatives existed at the time? record why they were rejected when a source says so. do not invent a debate from the options visible today.

## consequences

what became easier and what became difficult? later rewrites, workarounds, extension APIs, and operational habits are useful evidence here.

## descendants and reactions

which systems copied the mechanism? which ones preserved the goal but replaced the mechanism? a rewrite by people who operated the previous system is unusually useful evidence.

## result for this project

the research ends with one of four outcomes:

- adopt the invariant and mechanism
- adopt the invariant through another mechanism
- reject the invariant because our context differs
- leave the decision open and name the missing evidence

## evidence rules

primary sources come first: papers, specifications, official design documents, source code, issue discussions, and retrospectives by the designers.

marketing pages can establish what a project claims. they are weak evidence for whether the design works.

the date of a source matters. a current manual may describe the final shape without explaining the original constraint.

claims and inferences stay separate. if a successor changed something, that is evidence of pressure. it is not proof that the earlier design was a mistake in its original setting.

## writing

the notes should say what happened in plain language. avoid inflated claims, mechanical summaries, repeated conclusion sections, fake quotations, and citations that only point to a search page.

headings describe content. bold text is not used as a substitute for structure. lists are used for actual sets, not to make a paragraph look complete.

this guidance was informed by [Wikipedia's field guide to signs of AI writing](https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing). the useful part for this repository is the warning against vague significance, superficial synthesis, canned contrast, repetitive triples, excessive formatting, and unsupported attribution.
