---
schema: design-doc/v1
id: decision-0065-entry-evaluation-and-the-cli-query-surface
title: an entry is a represented pure request and the CLI exposes evaluation, explanation, selection, planning, and provenance without linking a domain
summary: load one transitive program, register represented rules, synthesize each invoked entry as a digest-revised zero-input pure rule, refuse collisions and unbound host rules, drive planning to the first action, and expose the results through query API version 4
kind: decision
status: proposed
created: 2026-08-28
updated: 2026-09-21
tags:
  - cli
  - language
  - engine
  - queries
  - entries
relations:
  informed_by:
    - planning-cli-surface
  depends_on:
    - decision-0015-interface-rule-selection
    - decision-0024-persistent-engine-state
    - decision-0031-action-cache-identity
    - decision-0038-represented-rule-bodies
    - decision-0051-transitive-revalidation
    - decision-0057-the-rule-index
    - decision-0061-the-declaration-artifact
    - decision-0063-the-frontend-graph-tier
  amends:
    - decision-0057-the-rule-index
  supersedes: []
---

# an entry is a represented pure request and the CLI exposes evaluation, explanation, selection, planning, and provenance without linking a domain

> edit, 2026-09-21: amended by [0083](0083-outputs-and-commands.md). `entry` becomes `output`,
> which other projects can also reference; `pith run` starts a program declared as `pith.Exec`; `pith exec` is
> removed; `pith eval` and `pith test` are added. `pith graph select` goes with selection by type
> ([0082](0082-rules-are-called-by-name-and-functions-are-values.md)).

> amends [0057](0057-the-rule-index.md): one CLI process now constructs two engines over the same rule
> table semantics. read-only selection registers declarations in a query-only in-memory engine that has
> reader authority and cannot publish; evaluation and planning consume `Session<Writable>` and build an
> engine over the filesystem content store and SQLite state store. convergence of their selection answers
> is enforced by the shared `Program::bind` path, not assumed from two registration implementations.

## context

the notation had entries but the loader discarded their elaborated bodies, and the CLI could check or
describe a module but not request anything it declared. the engine already exposed live and durable
invalidation explanations, direct selection, action planning, and dependency attempts; none had a command.
the CLI also deliberately depends on no domain crate, so copying the first-party Rust registration tables
into it would make the entry construct a disguised built-in domain.

the query driver was about to acquire a third copy of file-relative import loading: `check`, `explore`, and
evaluation all need the root and its transitive imports, in dependency order, with each source loaded once.

## decision

### one loaded program

`pith-query::program` owns root loading, transitive file-relative import resolution, the built-in module
environment, and engine registration. `check`, `explore`, `run`, `explain`, and `graph` share it or its
import-environment half. a program registers represented pure rules from every loaded module. a host rule
is registered as a refusing binding so selection remains truthful and evaluation reports the coordinate:
`` `xylem.compile` is `= host`; the CLI links no domain crate ``. the CLI does not guess a domain adapter.

### entries are synthetic represented rules

invoking entry `name` in module `module` constructs a pure represented rule at
`module::entry.name` with interface `() -> T`. its body is the entry's elaborated request body and its
revision is derived by `Rule::represented`, hence from the canonical body digest and interface. the request
has no inputs. a second process therefore derives the same pure computation key and may hydrate the first
process's result without any entry-specific cache mechanism.

module rules bind before the synthetic rule. if a module already supplies `() -> T`, selection sees both
ordinary candidates and returns E-1102. the diagnostic explains that an entry name chooses
a request, not a preferred rule, and asks the author to distinguish the interfaces. 0015 is unchanged: the
entry does not introduce priority or an implicit coordinate preference.

`Session<Writable>` alone exposes entry evaluation, planning, explanation, and exec preparation; the
compile-fail documentation test proves `Session<ReadOnly>` has no evaluation method. `graph select` uses the
read-only query engine and does not create SQLite state. this is the two-engine construction 0057 had not
recorded.

### the command surface

- `pith run <entry>` evaluates through the ordinary action-capable engine path and renders
  `Value -> ValueRepr -> QueryView::Run`, including whether the answer was computed, reused, or hydrated.
- `pith explain <entry>` evaluates or finds the computation and asks the durable
  `EngineStateReader::explain_invalidation(PureComputationKey)` or the live
  `EngineQuery::explain_invalidation(ComputationId)`. it uses the existing `Payload::Explain` vocabulary.
- `pith graph select <entry>` reports the selected coordinate, tier, and interface without publishing.
- `pith graph deps <entry>` walks the most recent durable attempt and its recorded 0051 dependency edges,
  reporting attempt identities and terminal states as a tree.
- `pith graph plan <entry>` calls the new `Engine::plan_entry`. the driver evaluates pure steps, admits
  requested blobs, and at the first `NeedAction` calls the existing action planner and returns its
  `ActionPlan` without allocating or executing an action computation. a pure entry that completes first is
  E-1219; an observation reached first retains the pure/effect refusal. live pure frames are cancelled at
  the pause.
- `pith exec <entry>` evaluates and requires the nominal builtin `pith.Exec`, represented by
  `{ arguments: List<Text>, program: Text }`. on Unix it calls `CommandExt::exec`; a return is the error
  path, while successful child status reaches the shell because the process image was replaced.

the root accepts `diff`, `update`, and `add` as documented M-14 variants and refuses each with “requires a
workspace”. one root `after_help` statement names the workspace boundary. entry commands accept
`--module PATH`, defaulting to M-14's future `module.pi`, so their short shape is already final.

> amended by [0067](0067-local-module-workspaces.md): `--module` now names a manifest, and passing
> `module.pi` selects manifest loading unconditionally. the file-relative neighbor resolver is reachable
> only through explicitly loading a standalone `.pi` file, never as a workspace fallback. the version
> sentence above is historical: with no release cut, the query contract restarts at version 1.

### contract and rendering

`pith explore` now includes entries and `about` blocks beside imports, declarations, rules, interfaces,
and tiers. `QueryView` remains exhaustive and gains run, selection, action-plan, and dependency views.
`QUERY_API_VERSION` moves from 3 to 4 because the JSON vocabulary is the versioned contract even though the
change is additive. the action-plan DTO projects the entire inert `ActionSpec`, not a renderer-specific
summary.

the formatter's safety claim is finally executable: every written corpus body keeps the same body digest
and module ABI after formatting, and formatting twice equals formatting once.

## alternatives considered

### silently prefer the entry rule on collision

rejected because it creates exactly the priority 0015 refuses, only under a syntactic side channel. it
would also make `select` and evaluation disagree unless every query knew the side channel.

### plan only recorded action dependencies

this was the cheap fallback. rejected as the destination because no previous attempt is required to exist
and a stale recorded branch need not be the branch the current entry reaches. the recorded dependency tree
is exposed separately by `graph deps`; `graph plan` drives current pure code to the pause.

### link first-party domain crates into the CLI

rejected because the command would work only for domains known when the binary was compiled and would
erase the module-system adapter boundary M-14 exists to define. an `= host` declaration is an explicit
unbound dependency here and its coordinate is the useful answer.

### defer exec until workspaces exist

rejected because the value boundary is already sufficient. `pith.Exec` is one builtin nominal and process
replacement is one caller effect; neither needs dependency resolution. Linux-first in 0006 means no
windows emulation is owed by this round.

## evidence

the entry query suite proves compute then cross-session hydration, source-free read-only selection, the
E-1102 collision lesson, coordinate-bearing host refusal, built-in exec decoding, and explore's entry/about
projection. the engine suite proves planning reaches the first action, returns its contract, allocates no
action computation, and cancels the paused pure frame. CLI end-to-end tests prove the JSON run/selection/deps
views, the explain payload, host planning refusal, failed `exec`, and all three M-14 stubs. query snapshots
cover all twelve exhaustive views under API version 4; help snapshots cover the root, run, and graph group.

## unresolved

a represented action rule still has no surface `plan { ... } / complete { ... }` contract projection, as
0062 records. `plan_entry` is complete at the kernel boundary and a domain-free CLI correctly refuses the
host planner it reaches; a future module adapter may bind that host coordinate without changing the entry
or planning protocols.

`pith.Exec` currently accepts a program path rather than a content-owned executable. it is a caller effect,
not an action contract, so this does not claim sandboxing or cache identity for the child.
