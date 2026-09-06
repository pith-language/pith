---
schema: design-doc/v1
id: planning-modules-registry-integration
title: registry integration
summary: the registry as a domain Pith evaluates rather than a service it calls — one language, one engine, one admission mechanism, one store, one artifact, one policy language — with the seams that would betray a bolted-on registry and the tests that catch them
kind: planning
status: draft
created: 2026-09-06
updated: 2026-09-06
tags:
  - planning
  - modules
  - registries
  - engine
relations:
  informed_by:
    - research-module-distribution
    - research-index-formats
    - research-dependency-resolution
    - research-language-frontend
  depends_on:
    - planning-modules-distribution
    - planning-modules-system
    - planning-modules-registry
    - decision-0040-declared-constraints-and-resolution
    - decision-0041-the-written-lock
    - decision-0042-binary-reuse-as-admitted-substitution
    - decision-0044-the-first-source-adapter
    - decision-0056-peerhood-is-a-registered-crate
    - decision-0027-retention-and-gc
  supersedes: []
---

# registry integration

Most package managers are a second program wearing the first one's name. They carry their own
configuration language, their own resolver, their own cache, their own notion of why something changed,
and their own answer to whether content may be used. The build system and the package manager agree on a
directory layout and almost nothing else.

The claim here is that Pith does not have to pay that. Not because a registry is unimportant, but because
every mechanism a registry needs — a declared-input boundary, a pure resolver protocol, a content store
with retention, an admission decision over an offer, a canonical interface artifact, a language for
policy — **already exists in this repository for other reasons**. The registry's job is to be expressed in
them, not to bring its own.

This document states how far that goes, where it stops, and what test fails if it regresses. It is a plan.
Nothing below is measured, and two of the eight are not yet implemented for modules at all.

## the seams that would betray a bolted-on registry

These are the tells, written first so the rest can be checked against them. Each is a plausible outcome of
building [module distribution](distribution.md) without this document.

- A second configuration language: requirements in TOML or JSON beside a program in `.pi`.
- A resolver that runs beside the engine, so dependency selection is the one computation Pith cannot
  explain, reuse, or replay.
- Two answers to "why did this change" — one for builds, one for dependencies.
- Two admission mechanisms: one deciding whether a fetched source may be used, another deciding whether a
  prebuilt artifact may be substituted, with separate refusal types that drift apart.
- Two stores: the engine's content store, and a package cache beside it with its own eviction.
- An ABI field invented for the registry index, unrelated to the interface artifact the frontend already
  produces and digests.
- Policy that lives in the registry's database, so a consumer cannot state, read, or diff their own rule
  about what they will accept.

None of these is hypothetical. Each is the cheapest next commit at some point in the distribution
sequence.

## eight integrations

Each states the claim, the mechanism, what already exists, and the limit. Where something already exists
it is named by file, because "already exists" is the load-bearing part of the argument.

### 1. one language

The manifest is `.pi`. Subjects, version requirements, registry bindings, and workspace membership are
Pith values with Pith spans, produced by the same lexer and parser as the program they configure, and
their errors render through the same diagnostic path.

Already true: `module.pi` and the manifest grammar landed with [workspaces](workspaces.md), and
`crates/pith-hir/src/manifest.rs` puts the manifest in the same HIR the program uses.

The limit: `.pi` for the manifest does not make the *index* a Pith document. The index is data a registry
serves, read by an adapter, and it should stay a boring line-oriented format that a static host can serve
and a human can diff. One language for what a person writes; a wire format for what a machine serves.

### 2. one engine, one resolver protocol

Resolution is a pure rule registered on the same engine API a peer domain uses, not a subroutine called
beside it. 0040's four inputs and its solved/unsatisfiable/underdetermined/exhausted outcomes are the
protocol; a domain supplies its own constraint model and its own ordering.

Already true for packages: `crates/phloem/src/resolve.rs` is exactly this — "resolver registration and
pure rule execution", a `PureRule` over decoded request values. The module domain does not need a new
mechanism, and [distribution](distribution.md)'s slice 2 requires it to register on the same API rather
than linking phloem as a shortcut.

The limit, and it is real: a registered resolver whose answer is a computation means dependency resolution
becomes engine state. That collides with the CLI contract's promise that read-only commands create no
database, and with 0027's retention policy. The resolution is that *solving* is a computation and
*consulting an existing lock* is not — a locked read validates recorded selections and never enters the
solver. Say this in the opening record, because getting it wrong makes `pith check` create a database.

### 3. one boundary between what is fetched and what is computed

Acquisition is a caller-side effect. Solving is pure over a declared universe. This is what makes the
previous claim legal rather than a violation of 0007's ban on ambient discovery during evaluation.

Already true: `crates/phloem/src/registry.rs` states it in its own header — reads produce candidate
universes and evidence, and "all reads are caller-side effects". 0044 settled the argument and named the
future home for the fetch: a fixed-output action, once an executor admits network under a declared output
digest, with no change to the verification.

This is the single most important structural fact in the document. A registry that is well integrated is
*not* one the engine can call. It is one the engine never learns exists, whose results arrive as declared
inputs like any other value. Integration here means the universe participates in the computation key, so a
moved index line moves a digest — not that the solver may open a socket.

### 4. one explanation

Because 3 makes the universe a declared input and 2 makes the solve a computation, `explain_invalidation`
applies to resolution. "Why did my dependency version move" is answered by the machinery that answers "why
did this build rerun", and names the moved input rather than reciting the solver's steps.

Already exists: `explain_invalidation` runs through `crates/pith-query/src/entry.rs` and the state store.
0041 already measured a fabricated universe's digest moving and the lock's diff naming it as the moved
input.

This is the integration that is hardest to obtain any other way, and it is nearly free here. No package
manager surveyed in the [research](../../research/index.md) offers it, because none of them holds
resolution in an incremental engine that records why a computation's answer changed.

The limit: the explanation is about *inputs*, not about the search. "The universe moved and the answer
followed" is a different, weaker statement than "this constraint from this manifest line forced this
version", and both are wanted. The second is the resolver's own trace obligation under 0040 and is not
delivered by the engine.

### 5. one admission mechanism, distinct claims

Whether a fetched module source may be used, and whether a prebuilt artifact may be substituted, are the
same decision shape over different evidence: an offer, a policy, measured facts, and a refusal that names
which clause failed.

Already exists: `crates/phloem/src/substitution/` has `Admission`, `Admitted`, `AdmittedOrigins`, `admit`,
and `Refusal` for 0042's binary substitution. The admission record in
[the registry](registry.md)'s admission step is deliberately that shape. Unify the mechanism.

The limit, and it is a requirement rather than a preference: T-5 says content signatures, reproducibility,
dependency policy, builder identity, and deployment evidence are separate claims. Sharing the machinery
must not merge the claims. One `Refusal` type with a clause per claim; never one boolean that five
different checks can set.

### 6. one store, and no second one

The registry stores no module content, so there is no package cache to sit beside the engine's content
store with its own eviction. Acquired module trees are measured into the store Pith already has, under the
identity scheme and the retention policy it already has, and a locked build reacquires from a pinned
revision rather than from a registry-shaped cache that had better not be evicted.

The consequence worth having: a registry needs no server at all. It is a Git repository, so a clone, a
fetch, and a static HTTP mount are three transports over one layout, and publishing is proposing a commit.
[The registry](registry.md) makes this the whole model rather than an optimization over one.

The limit: "no server" is a claim about the registry, not about everything a registry ecosystem might want.
Availability still depends on the source forges the index names, and mirroring is the operational answer —
a mirror cannot lie, because the tree digest is pinned, but it does have to exist. Do not let "no server
needed" become "nothing needs operating".

### 7. one artifact crossing the boundary

The surface a registry publishes is the frontend's own interface artifact, canonicalized and digested,
not an ABI field invented for the index. So comparing two published versions is comparing two frontend
artifacts, and reusing a dependency's elaboration is a graph cache hit rather than a registry feature.

The [language-frontend research](../../research/language-frontend.md) found near-unanimity on this among
batch compilers that achieved cross-module parallelism: the canonicalized interface is the unit that
crosses a module boundary and its digest is the downstream key. Pith already builds that artifact.
Publishing anything else would mean maintaining two notions of a module's surface.

The limit, already recorded in [distribution](distribution.md): the artifact carries its dependency ABI
context and elaborator revision. A published surface is directly reusable only when both match; otherwise
the consumer re-elaborates the source. A registry surface is a cache entry, not an authority.

### 8. one policy language

The admission record is a Pith value, so a consumer's policy over it is a Pith rule — in the same language,
the same file tree, and the same review as their build. "Refuse anything published in the last seven days",
"require two endorsements for this domain", "admit binaries only from these origins" are declarations a
person writes, diffs, and tests, not settings in someone else's database.

0056's peerhood argument applies directly: a peer reaches what the built-ins reach, and proves it by
coming through the same door rather than by assertion. A policy only the registry can express is a policy
the consumer cannot audit.

Two limits, both hard. Policy is an input to admission, so it must be pure and declared — a policy that
reads the network or the clock re-introduces exactly the ambient authority 3 exists to prevent; time
enters as the declared cutoff. And policy is consumer authority: a *dependency's* manifest can never
supply, weaken, or extend it, on the same grounds a dependency cannot alter routing.

## what integration does not mean

Integration is about mechanism, not scope. These stay outside, and the boundary is that Pith models their
**results** as values without implementing them:

- The forge, and any particular identity provider. A stage capability is a value; which issuer minted it
  is adapter-local, and no forge acquires compiler privilege.
- Key custody, ceremonies, and rotation procedure. The client verifies; it does not hold an opinion about
  where a maintainer keeps a key.
- Witness operation, monitoring, scanners, and vulnerability feeds. Their findings are evidence values in
  an admission record. Running them is an operator's job, and detection requires someone to look.
- Namespace governance. Domain authority is consumer configuration plus a registry's own grant, and no
  global ownership service is implied.

A registry that is "well integrated" because Pith took over these responsibilities would be a worse
system, not a better one.

## the integration tests

These turn the claim into something that can fail. Each is a test that passes trivially in a well
integrated design and is expensive to make pass in a bolted-on one — which is the point of writing them
before the code.

| test | integration it defends |
| --- | --- |
| a manifest error and a program error render through one diagnostic path, with spans into `module.pi` | 1 |
| the module resolver registers through the same engine API as phloem's, and the driver links neither phloem, xylem, nor stele | 2 |
| the solver performs no I/O; removing network access changes no solved answer, only whether a universe could be acquired | 3 |
| `pith check` and the read-only surfaces create no database and no lock while consuming an existing lock | 2 |
| editing one index line and re-resolving names the moved declared input in the invalidation explanation | 4 |
| a refused source and a refused binary substitution produce the same refusal type with different clauses, and no check can set another's clause | 5 |
| the same registry content served from a directory, a Git remote, and a static HTTP mount yields identical admission records | 6 |
| a dependency acquired from a registry hits the same frontend artifact as the identical bytes on a path dependency | 7 |
| a consumer-declared policy rule refuses a release the registry itself admitted, with a diagnostic pointing at the consumer's own rule | 8 |
| a dependency manifest attempting to supply routing or admission policy is ignored for authority and diagnosed | 8 |

The last two are the ones most likely to be dropped under schedule pressure, and they are the two that
make the difference between a registry a person configures and a registry a person programs.

## open questions

1. **Resolution as engine state.** If a solve is a computation, when is it collected, and does a collected
   resolution make an offline `update --check` impossible? 0027's axes were not written with resolution
   in mind.
2. **Trace obligation.** Claim 4's limit says the engine explains inputs and the resolver must explain
   constraints. Is the constraint trace a value the resolver returns, or a second computation? It affects
   whether an explanation survives without rerunning the solve.
3. **One store, two lifetimes.** Registry metadata snapshots are needed to replay a resolution; module
   source is needed to build. They have different retention pressures and currently one policy.
4. **Policy distribution.** A consumer-written policy is auditable, but every consumer writing their own is
   how a default becomes "whatever the first tutorial said". Can a policy be a published module —
   distributed by the mechanism it governs — without becoming dependency-supplied authority?
5. **Index as a wire format.** Claim 1's limit keeps the index out of `.pi`. If a registry later wants
   richer per-version metadata, does that pressure the format toward being a document value after all?
