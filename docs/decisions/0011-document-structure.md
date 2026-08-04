---
schema: design-doc/v1
id: decision-0011-document-structure
title: separate documentation by role
summary: keep foundation, research, decisions, current design, requirements, and planning as different document classes
kind: decision
status: accepted
created: 2026-04-13
updated: 2026-04-13
tags:
  - documentation
  - metadata
relations:
  informed_by:
    - foundation-principles
  depends_on: []
  supersedes: []
---

# separate documentation by role

## context

the first draft placed scope, principles, requirements, architecture, research, and open questions directly under `docs` or one level below it. documents used a generic `related` list.

that structure was small, but it did not clearly distinguish historical evidence from accepted choices and the current design. `related` also said that two documents were connected without preserving how.

## decision

documents are grouped into `foundation`, `design`, `requirements`, `research`, `decisions`, and `planning`.

every Markdown document uses the `design-doc/v1` frontmatter schema with a stable ID, title, summary, kind, status, dates, tags, and directional relations.

the relation vocabulary currently contains `informed_by`, `depends_on`, and `supersedes`. relations use stable document IDs instead of paths.

only one relation direction is stored. backlinks are derived.

## alternatives considered

### flat files

a flat directory is easy to browse while the project is small. document roles become harder to see as research and decisions accumulate.

### topic directories

all material about builds, deployment, or the language could live together.

this helps topic browsing and mixes evidence, decisions, current design, and requirements. readers cannot tell whether a statement is historical or normative from its location.

### generic relations

one `related` list is simple and flexible.

it erases direction and meaning. maintaining links on both documents would create synchronized copies.

### paths as identifiers

relations could link directly to Markdown paths.

this is convenient for renderers. moving a document changes its identity and rewrites every metadata reference.

## consequences

index pages provide human navigation. a later validation tool can resolve IDs, generate backlinks, and find cycles or missing documents.

research documents add an `evidence` field. more kind-specific fields should only be introduced when a real query needs them.

