---
schema: design-doc/v1
id: decision-0070-module-acquisition-resolution-and-replay
title: a module version is its canonical segment sequence, solving is a computation while consulting a lock is not, and how much authority a command has over its own inputs is one named mode
summary: two version spellings that compare equal are one version, every source kind reaches one admission boundary, resolution is a pure rule over an acquired universe so a locked read never enters the solver and creates no database, a lock replays a selection without replaying a judgment about its fitness, and current/pinned/sealed/strict replace a set of composable booleans
kind: decision
status: proposed
created: 2026-09-06
updated: 2026-09-06
tags:
  - modules
  - resolution
  - locks
  - identity
  - cli
relations:
  informed_by:
    - planning-modules-distribution
    - planning-modules-registry-integration
    - planning-cli-surface
    - research-dependency-resolution
  depends_on:
    - decision-0007-tracked-dynamic-dependencies
    - decision-0040-declared-constraints-and-resolution
    - decision-0041-the-written-lock
    - decision-0044-the-first-source-adapter
    - decision-0067-local-module-workspaces
    - decision-0069-module-authority-is-consumer-configuration
  amends:
    - decision-0067-local-module-workspaces
  supersedes: []
---

# a module version is its canonical segment sequence, solving is a computation while consulting a lock is not, and how much authority a command has over its own inputs is one named mode

> amends [0067](0067-local-module-workspaces.md): manifest versions are no longer metadata that nothing
> resolves against. A version is a canonical segment sequence under a declared scheme, two spellings that
> compare equal are one version, and `use` clauses may carry ranges. Path dependencies keep 0067's
> behaviour exactly — live, unwitnessed, no range, no solver, no lock entry beyond their route.

## context

[0069](0069-module-authority-is-consumer-configuration.md) settles who may say that a subject is satisfied
by some content. This record settles what that content *is*, how one version of it is chosen among many,
and what a lock preserves when the choice is replayed later.

Three of its questions cannot be deferred past the first encoder. A version's identity decides whether
`1.2` and `1.2.0` are one release or two, which an index cannot revise afterwards. Whether solving is a
computation decides whether `pith check` creates a database, which
[integration](../planning/modules/registry-integration.md) names as the collision most likely to be
discovered rather than decided. And what a lock does *not* freeze decides whether a withdrawn release can
still be refused, which a lock format either leaves room for or does not.

## decision

### a version is a canonical segment sequence

A module version is a sequence of numeric segments under the subject's declared version scheme. The first
scheme is `numeric-segments`, phloem's, where missing trailing segments compare as zero.

That comparison has a consequence the manifest grammar cannot avoid making: `1.2` and `1.2.0` compare
equal. **Two spellings that compare equal are one version.** A version's identity is its segment sequence
with trailing zeros removed, so both spellings name the release `1.2`, a registry refuses a second entry
for a version it already carries under another spelling, and a lock records the canonical form. The
alternative — two entries a solver treats as distinct and a person reads as identical — is a confusion
that no later rule can undo, because both entries are already published.

Comparison is the scheme's, and nothing outside the scheme parses a version spelling. A subject declares
its scheme; a consumer does not choose one for someone else's module.

### ranges are written out, and mean one thing

A `use` clause may carry a range. The spellings are the comparison operators and nothing else:

```text
use text = example/text 1.2                -- exactly 1.2
use text = example/text >= 1.2             -- at least, inclusive
use text = example/text > 1.2              -- at least, exclusive
use text = example/text <= 2.0             -- at most, inclusive
use text = example/text < 2.0              -- at most, exclusive
use text = example/text >= 1.2, < 2.0      -- between
use text = example/text any                -- any version the registry carries
```

There is deliberately no caret, tilde, or other compatibility sigil. Those spellings mean measurably
different things in different ecosystems, and each encodes a *policy about what a version number promises*
that belongs to the publisher's scheme rather than to a consumer's punctuation. A consumer who wants the
common `>= 1.2, < 2.0` shape writes it, and a reader knows what it says without knowing which ecosystem
taught them the sigil.

These map onto 0040's existing range model — `Any`, `Exactly`, `AtLeast`, `AtMost`, `Between` — because
that model is already the resolver protocol's, and inventing a second one for modules is the "two
resolvers" seam [integration](../planning/modules/registry-integration.md) exists to prevent.

### four source kinds, one admission boundary

`registry` is the default source and the only one a bare range implies. `path`, `git`, and `archive` are
written out. Whatever the route, acquisition is the same pipeline: locate, read bounded content, verify
transport integrity, normalize the module tree, measure its identity, validate the manifest's declared
subject and version against what was requested, verify applicable evidence, then expose the admitted source
set.

No source kind gets a quieter path than the registry's. Every route reaches the same admission record —
content identity, evidence consulted, policy in force, time context, reason — and that record is
recomputed rather than stored as a trusted bit, so a later withdrawal or an expired snapshot changes the
answer without changing the bytes.

An explicit `git` or `archive` route names **one module, not an index**. Its manifest is acquired outside
the solver and presented as a singleton candidate carrying its own declared requirements; a written range
is checked against that manifest's version rather than used to search. Tags are not scanned to invent a
version universe. Its transitive registry dependencies then use the ordinary metadata closure. This is the
necessary acquisition cost of a source with no index, not permission for the solver to perform I/O.

Paths keep 0067's behaviour: live, explicitly unwitnessed, no range, no version resolution, `Unchecked` in
every surface that reports admission.

**A diamond whose two routes to one subject disagree is refused across the whole resolution**, naming both
clauses. Not the narrower route, not the first one read, not a merge — a subject is satisfied by one
content identity in one program, and two explicit routes claiming it is a contradiction in the consumer's
own configuration rather than something to rank. Two routes that agree — the same canonical location, or
the same revision through two locators — load once, which 0067 already established for paths and 0069's
locator rule extends to the rest.

### solving is a computation; consulting a lock is not

Resolution is a pure rule registered on the same engine API a peer domain uses, over a universe acquired
outside it. This is 0040's protocol with its four inputs and its solved, unsatisfiable, underdetermined and
exhausted outcomes, and it is what makes `explain_invalidation` answer "why did my dependency version
move" with the machinery that answers "why did this build rerun".

The solver performs no I/O. Removing network access changes only whether a universe could be acquired,
never a solved answer. Resolution time enters as a **declared cutoff**, recorded, not as a clock read
inside the solve — so an `update` does not drift as time passes, and a cooldown is a consumer policy
comparing that recorded cutoff against a registry's admission time rather than a mechanism the registry
operates.

The collision this creates is real and is settled here, because getting it wrong makes `pith check` create
a database. **Solving is a computation. Consulting an existing lock is not.** A locked read validates
recorded selections against current requirements and never enters the solver, so it publishes no
computation, creates no state database, and writes no lock. Only `update` solves.

A search limit is never evidence of unsatisfiability. Exhaustion is its own outcome, distinct from an
unsatisfiable constraint set, and the diagnostic says which.

### a lock replays a selection, not a judgment about it

A lock records what was selected and what evidence supported selecting it: the canonical version, the
content identity, the origin, the effective bindings in force, the resolver identity and revision, the
scheme, the preferences, the recorded resolution cutoff, and the snapshot references needed to reacquire.
Writing it stays a caller effect under 0041, and a failed acquisition, validation, or resolution leaves the
prior lock bytes intact.

What a lock does **not** freeze is a judgment about the selection's fitness. Withdrawal lives in the index
and is read locally on every run, so a release withdrawn after a lock was written makes the next run refuse
— naming the issuer and the reason, leaving the recorded selection unchanged, and substituting nothing.
Freezing withdrawal into the lock would make a lock a way to keep using a release that has been retracted,
which is the opposite of what pinning is for.

Two things follow. An ordinary run consults withdrawal without network access, because the index is local
and `update` is what refreshes it. And a newer registry snapshot does not by itself invalidate lock
consumption: only `update` asks whether a fresh universe changes the answer.

The lock must preserve enough to reacquire content after the engine database is deleted. A snapshot
identity that is only an unrecoverable hash fails that test; every recorded identity carries a locator or
retained data.

### authority over inputs is one named mode

How much authority an operation has over its own inputs is a single named value, not a set of composable
flags. Three booleans describe eight states of which four are meaningful, `--frozen` is a composite alias
for two of the others, and none of the names says what it permits.

| mode | what it permits |
| --- | --- |
| `current` | may acquire a fresh candidate universe and change the answer; only `update` runs here |
| `pinned` | the lock is authoritative; recorded content may be reacquired at its recorded identity, and nothing may be reselected |
| `sealed` | no network at all; already-admitted local content only, and withdrawal knowledge is whatever the local index holds |
| `strict` | `sealed`, and every authority binding must be project-owned rather than user-level |

The modes decrease in authority, so a project declares its floor once and no command can quietly exceed
it. A refusal names the mode and what it forbade. `update --check` is not a mode: it is `current` with the
write withheld, which is why it stays a flag on one command.

### `pith diff` has two meanings and two spellings

Bare `diff` is **workspace drift**: current declarations and bindings compared against the lock and its
cached evidence, naming what has drifted without refreshing the registry. This is the meaning a person
reaches for by default and it keeps the bare spelling.

**Surface comparison** — two module versions or artifacts and their dependency contexts — is a different
question with different operands, and it gets an explicit form rather than being inferred from whether
arguments were supplied. Under [0068](0068-published-surfaces-are-context-bound.md) it reports structure,
context, and named-consumer evidence separately and never merges them, and it subtracts the imported-ABI
region before attributing any surface difference to the edited module.

The help contract is pinned when the commands land: one line each, the operand forms distinguishable
without reading prose.

### loading still begins where it is told

0067 refused upward directory search for manifests, and the same rule governs a registry snapshot and a
workspace root. A snapshot is selected by configuration, not discovered by walking; a workspace member
built from another directory names its workspace root explicitly rather than having it rediscovered.

The reason is unchanged and worth restating because acquisition makes it sharper: a check of one manifest
must be one deterministic input set. Ambient discovery would reintroduce exactly the user-configuration
ambiguity 0069 spends its routing rules removing.

## the refusal table

| refusal | where | code |
| --- | --- | --- |
| a version range outside the written comparison forms | parse | E-3058 |
| a range on a path dependency | parse | E-3059 |
| a `git` route with no revision, or an `archive` route with no digest | parse | E-3060 |
| two explicit routes to one subject that disagree | resolve, naming both clauses | E-3061 |
| a registry carrying two entries for one canonical version | acquisition | E-3062 |
| an acquired manifest whose subject or version is not the one requested | acquisition | E-3063 |
| a constraint set with no solution | resolve, reporting the chain | E-3064 |
| a search budget exhausted before a solution was found or excluded | resolve, distinctly from E-3064 | E-3065 |
| a lock entry whose recorded requirements no longer match the manifest | locked read | E-3066 |
| an operation attempting authority above the declared mode | wherever the mode is exceeded | E-3067 |

## alternatives considered

**Caret and tilde ranges.** Familiar, compact, and what most consumers expect. Rejected because they encode
a promise about version numbering that belongs to the publisher's scheme, they mean different things across
ecosystems, and a range a reader has to look up is worse than one they can read.

**Distinct entries for `1.2` and `1.2.0`.** Simpler at the encoder: keep the spelling, compare with the
scheme, publish what the manifest said. Rejected because two entries a solver treats as different and a
person reads as identical is a confusion no later rule can undo, since both are already published and an
index is append-only.

**Freezing withdrawal knowledge into the lock.** It would make a locked build fully self-contained, which
is a real property to want. Rejected because it converts a lock into a way to keep consuming a retracted
release, and because it puts a judgment about fitness in the same artifact as the selection — after which
there is no mechanism left to change the judgment without rewriting the selection.

**Composable authority flags.** `--offline`, `--locked`, `--frozen`. Rejected as the defect a named value
fixes: eight states of which four mean anything, one flag that is an alias for a combination, and no name
that says what it permits.

**A larger solver dependency.** Deferred rather than refused. Start with deterministic bounded backtracking
at fixture scale, with exhaustive small-universe enumeration as an independent correctness oracle, and
introduce a solver dependency only against a measured need.

## evidence

None yet, by design. This record settles contracts ahead of their encoders. Its claims become measurable in
the slices that build the round trip and the resolver, and it stays proposed until they run.

The obligations it creates, stated so they can be checked against later: a locked read that creates no
database, a solve whose answer is unchanged by removing network access, a diamond that shares one version,
a case requiring backtracking, input permutations producing one answer and one lock, a withdrawn selection
refused at run with the recorded selection intact, and the same lock replayed offline reporting `Stale`
with its withdrawal knowledge's age visible.

## unresolved

**Resolution as engine state.** If a solve is a computation, when is it collected, and does a collected
resolution make an offline `update --check` impossible? 0027's retention axes were not written with
resolution in mind.

**The trace obligation.** The engine explains inputs — "the universe moved and the answer followed". The
resolver owes the stronger statement, "this constraint from this manifest line forced this version", under
0040. Whether that trace is a value the resolver returns or a second computation decides whether an
explanation survives without rerunning the solve.

**Two lifetimes in one store.** Registry metadata snapshots are needed to replay a resolution; module
source is needed to build. They have different retention pressures and currently one policy.

**Whether a consumer policy can itself be a published module**, distributed by the mechanism it governs,
without becoming the dependency-supplied authority 0069 forbids.
