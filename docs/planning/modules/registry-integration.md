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

most package managers are a second program wearing the first one's name: their own configuration
language, resolver, cache, notion of why something changed, and answer to whether content may be used.
the build system and the package manager agree on a directory layout and almost nothing else.

pith does not have to pay that, because every mechanism a registry needs (a declared-input boundary, a
pure resolver protocol, a content store with retention, an admission decision over an offer, a canonical
interface artifact, a language for policy) **already exists in this repository for other reasons**. the
registry's job is to be expressed in them, not to bring its own.

this document states how far that goes, where it stops, and what test fails if it regresses. it is a plan:
nothing below is measured, and two of the eight are not yet implemented for modules.

## the seams that would betray a bolted-on registry

written first so the rest can be checked against them, each is a plausible outcome of
building [module distribution](distribution.md) without this document.

- a second configuration language: requirements in TOML or JSON beside a program in `.pi`.
- a resolver that runs beside the engine, so dependency selection is the one computation pith cannot
  explain, reuse, or replay.
- two answers to "why did this change": one for builds, one for dependencies.
- two admission mechanisms: one deciding whether a fetched source may be used, another deciding whether a
  prebuilt artifact may be substituted, with separate refusal types that drift apart.
- two stores: the engine's content store, and a package cache beside it with its own eviction.
- an ABI field invented for the registry index, unrelated to the interface artifact the frontend already
  produces and digests.
- policy that lives in the registry's database, so a consumer cannot state, read, or diff their own rule
  about what they will accept.

none of these is hypothetical. each is the cheapest next commit at some point in the distribution
sequence.

## eight integrations

each states the claim, the mechanism, what already exists, and the limit. where something already exists
it is named by file.

### 1. one language

the manifest is `.pi`. subjects, version requirements, registry bindings, and workspace membership are
pith values with pith spans, produced by the same lexer and parser as the program they configure, and
their errors render through the same diagnostic path.

already true: `module.pi` and the manifest grammar landed with [workspaces](workspaces.md), and
`crates/pith-hir/src/manifest.rs` puts the manifest in the same HIR the program uses.

the limit: `.pi` for the manifest does not make the *index* a pith document. the index is data a registry
serves, read by an adapter, and it should stay a boring line-oriented format that a static host can serve
and a human can diff. one language for what a person writes; a wire format for what a machine serves.

### 2. one engine, one resolver protocol

resolution is a pure rule registered on the same engine API a peer domain uses, not a subroutine called
beside it. 0040's four inputs and its solved/unsatisfiable/underdetermined/exhausted outcomes are the
protocol; a domain supplies its own constraint model and its own ordering.

already true for packages: `crates/phloem/src/resolve.rs` is exactly this, "resolver registration and
pure rule execution", a `PureRule` over decoded request values. the module domain does not need a new
mechanism, and [distribution](distribution.md)'s slice 2 requires it to register on the same API rather
than linking phloem as a shortcut.

the limit, and it is real: a registered resolver whose answer is a computation means dependency resolution
becomes engine state. that collides with the CLI contract's promise that read-only commands create no
database, and with 0027's retention policy. the resolution is that *solving* is a computation and
*consulting an existing lock* is not: a locked read validates recorded selections and never enters the
solver. say this in the opening record, because getting it wrong makes `pith check` create a database.

### 3. one boundary between what is fetched and what is computed

acquisition is a caller-side effect. solving is pure over a declared universe. this is what makes the
previous claim legal rather than a violation of 0007's ban on ambient discovery during evaluation.

already true: `crates/phloem/src/registry.rs` states it in its own header: reads produce candidate
universes and evidence, and "all reads are caller-side effects". 0044 settled the argument and named the
future home for the fetch: a fixed-output action, once an executor admits network under a declared output
digest, with no change to the verification.

a well integrated registry is *not* one the engine can call; it is one the engine never learns exists,
whose results arrive as declared inputs like any other value. integration means the universe participates
in the computation key, so a moved index line moves a digest, not that the solver may open a socket.

### 4. one explanation

because 3 makes the universe a declared input and 2 makes the solve a computation, `explain_invalidation`
applies to resolution. "why did my dependency version move" is answered by the machinery that answers "why
did this build rerun", and names the moved input rather than reciting the solver's steps.

already exists: `explain_invalidation` runs through `crates/pith-query/src/entry.rs` and the state store.
0041 already measured a fabricated universe's digest moving and the lock's diff naming it as the moved
input.

no package manager surveyed in the [research](../../research/index.md) offers this, because none of them
holds resolution in an incremental engine that records why a computation's answer changed.

the limit: the explanation is about *inputs*, not about the search. "the universe moved and the answer
followed" is a different, weaker statement than "this constraint from this manifest line forced this
version", and both are wanted. the second is the resolver's own trace obligation under 0040 and is not
delivered by the engine.

### 5. one admission mechanism, distinct claims

whether a fetched module source may be used, and whether a prebuilt artifact may be substituted, are the
same decision shape over different evidence: an offer, a policy, measured facts, and a refusal that names
which clause failed.

already exists: `crates/phloem/src/substitution/` has `Admission`, `Admitted`, `AdmittedOrigins`, `admit`,
and `Refusal` for 0042's binary substitution. the admission record in
[the registry](registry.md)'s admission step is deliberately that shape. unify the mechanism.

the limit, and it is a requirement rather than a preference: T-5 says content signatures, reproducibility,
dependency policy, builder identity, and deployment evidence are separate claims. sharing the machinery
must not merge the claims. one `Refusal` type with a clause per claim; never one boolean that five
different checks can set.

### 6. one store, and no second one

the registry stores no module content, so there is no package cache to sit beside the engine's content
store with its own eviction. acquired module trees are measured into the store pith already has, under the
identity scheme and the retention policy it already has, and a locked build reacquires from a pinned
revision rather than from a registry-shaped cache that had better not be evicted.

a consequence worth having: a registry needs no server at all. it is a git repository, so a clone, a
fetch, and a static HTTP mount are three transports over one layout, and publishing is proposing a commit.
[the registry](registry.md) makes this the whole model rather than an optimization over one.

the limit: "no server" is a claim about the registry, not about everything a registry ecosystem might want.
availability still depends on the source forges the index names, and mirroring is the operational answer:
a mirror cannot lie, because the tree digest is pinned, but it does have to exist. do not let "no server
needed" become "nothing needs operating".

### 7. one artifact crossing the boundary

the surface a registry publishes is the frontend's own interface artifact, canonicalized and digested,
not an ABI field invented for the index. so comparing two published versions is comparing two frontend
artifacts, and reusing a dependency's elaboration is a graph cache hit rather than a registry feature.

the [language-frontend research](../../research/language-frontend.md) found near-unanimity on this among
batch compilers that achieved cross-module parallelism: the canonicalized interface is the unit that
crosses a module boundary and its digest is the downstream key. pith already builds that artifact.
publishing anything else would mean maintaining two notions of a module's surface.

the limit, already recorded in [distribution](distribution.md): the artifact carries its dependency ABI
context and elaborator revision. a published surface is directly reusable only when both match; otherwise
the consumer re-elaborates the source. a registry surface is a cache entry, not an authority.

### 8. one policy language

the admission record is a pith value, so a consumer's policy over it is a pith rule, in the same language,
the same file tree, and the same review as their build. "refuse anything published in the last seven days",
"require two endorsements for this domain", "admit binaries only from these origins" are declarations a
person writes, diffs, and tests, not settings in someone else's database.

0056's peerhood argument applies directly: a peer reaches what the built-ins reach, and proves it by
coming through the same door rather than by assertion. a policy only the registry can express is a policy
the consumer cannot audit.

two limits, both hard. policy is an input to admission, so it must be pure and declared: a policy that
reads the network or the clock re-introduces exactly the ambient authority 3 exists to prevent; time
enters as the declared cutoff. and policy is consumer authority: a *dependency's* manifest can never
supply, weaken, or extend it, on the same grounds a dependency cannot alter routing.

## what integration does not mean

integration is about mechanism, not scope. these stay outside, and the boundary is that pith models their
**results** as values without implementing them:

- the forge, and any particular identity provider. a stage capability is a value; which issuer minted it
  is adapter-local, and no forge acquires compiler privilege.
- key custody, ceremonies, and rotation procedure. the client verifies; it does not hold an opinion about
  where a maintainer keeps a key.
- witness operation, monitoring, scanners, and vulnerability feeds. their findings are evidence values in
  an admission record. running them is an operator's job, and detection requires someone to look.
- namespace governance. domain authority is consumer configuration plus a registry's own grant, and no
  global ownership service is implied.

a registry that is "well integrated" because pith took over these responsibilities would be a worse
system, not a better one.

## the integration tests

these turn the claim into something that can fail. each is a test that passes trivially in a well
integrated design and is expensive to make pass in a bolted-on one, which is the point of writing them
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

the last two are the ones most likely to be dropped under schedule pressure, and they are the two that
make the difference between a registry a person configures and a registry a person programs.

## open questions

1. **resolution as engine state.** if a solve is a computation, when is it collected, and does a collected
   resolution make an offline `update --check` impossible? 0027's axes were not written with resolution
   in mind.
2. **trace obligation.** claim 4's limit says the engine explains inputs and the resolver must explain
   constraints. is the constraint trace a value the resolver returns, or a second computation? it affects
   whether an explanation survives without rerunning the solve.
3. **one store, two lifetimes.** registry metadata snapshots are needed to replay a resolution; module
   source is needed to build. they have different retention pressures and currently one policy.
4. **policy distribution.** a consumer-written policy is auditable, but every consumer writing their own is
   how a default becomes "whatever the first tutorial said". can a policy be a published module
   (distributed by the mechanism it governs) without becoming dependency-supplied authority?
5. **index as a wire format.** claim 1's limit keeps the index out of `.pi`. if a registry later wants
   richer per-version metadata, does that pressure the format toward being a document value after all?
