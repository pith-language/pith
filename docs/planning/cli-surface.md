---
schema: design-doc/v1
id: planning-cli-surface
title: the cli surface
summary: one binary whose commands are thin clients of one query api, a grouping rule that bounds the verb set, and a machine-global store and state with no project directory
kind: planning
status: draft
created: 2026-08-25
updated: 2026-08-28
tags:
  - planning
  - tooling
relations:
  informed_by:
    - research-tooling
  depends_on:
    - planning-language-frontend
    - planning-frontend-architecture
    - planning-surface-notation
    - decision-0027-retention-and-gc
  supersedes: []
---

# the cli surface

this is the cli half of round six of [the language frontend](language-frontend.md), folded into M-13
because a notation nobody can invoke is not testable by a person. M-13 implements this surface through
the versioned `pith-query` API; [0065](../decisions/0065-entry-evaluation-and-the-cli-query-surface.md)
records the entry and engine boundaries it added.

## two rules generate the surface

the first is already in the notebook. [tooling](../research/tooling.md) requires the cli to be one client
of a versioned query api and never the api itself, and the language server is the second client. two
clients keep the api honest, because a capability reachable from only one of them is a capability living
in a driver.

nix is the counterexample, and the usual reading blames its subcommands. what went wrong is that a
capability arrived as a *program*. `nix-build`, `nix-env`, `nix-instantiate`, `nix-shell`, `nix-store`,
`nix-channel` and `nix-collect-garbage` each own logic, so the surface grew one binary at a time, and two
vocabularies for one operation survived long enough to both be documented: `nix-store --gc` beside
`nix-collect-garbage`, `nix-env` beside `nix profile`. the unified `nix` command has coexisted with the
originals since 2020 without retiring them. when the logic sits behind a query api, a capability arrives
as a call, and there is nothing to grow a binary around.

the second rule bounds the verb set. a top-level verb is the daily loop, anything else joins a noun group,
and a new noun group requires a new durable object kind. below the frontend there are two such kinds,
content and engine state, so there are two groups and no third becomes available until something new
needs persisting. that rule is what would have stopped `nix-env` from being a program.

## the daily loop

| | |
| --- | --- |
| `pith check [path]` | elaborate the module at path; errors and warnings |
| `pith fmt [path]` | format in place; `--check` verifies without writing |
| `pith explore [module]` | what a module declares: types, rules, entries, interfaces, and which tier answers |
| `pith run <entry>` | evaluate an entry and render the value |
| `pith exec <entry>` | evaluate to `pith.Exec`, then exec it |
| `pith explain <entry>` | why the last result was not reused |

`check`, `fmt` and `explore` touch no store and no engine. they call the elaborator library directly,
which is the library `interface-of` calls from inside the graph rule, and is what
[the frontend architecture](frontend-architecture.md) means by one elaborator serving two drivers.

`check` is the only command that has to produce useful output when elaboration fails, and that is why it
cannot be an entry. an entry is a value in the graph, so anything required to work while the graph does
not elaborate sits outside the entry mechanism. the same line decides the rest of this section.

`explore` carries the obligation [0061](../decisions/0061-the-declaration-artifact.md)'s amendment left
with tooling. `= host` is visible at the declaration site and deliberately not at the call site, so
something has to say which tier answered, and the amendment names an inlay hint or a hover. the terminal
needs the same answer and this is where it goes.

there is no `pith build`. build is an entry name. the milestone text's "a real `pith build`" predates the
entry construct, and the milestone is what should be corrected.

## graph introspection

| | |
| --- | --- |
| `pith graph select <entry>` | which rule serves the request, and whether it is ambiguous |
| `pith graph plan <entry>` | the action contract, without running it |
| `pith graph deps <entry>` | the recorded dependency subtree of the last attempt |

`select` is the read-only command, drive-to-pause entry planning reaches the current first action without
executing it, and `deps` reads the subtree [0051](../decisions/0051-transitive-revalidation.md)'s walk
already records.

all three take an entry name, which falls out of the design. an entry *is* a named request, so it is
already the call-site spelling these three were missing, and no request-literal syntax appears on the
command line.

## content and state

| | |
| --- | --- |
| `pith store add <path>` | put a file or directory; print the identity |
| `pith store cat <id>` | write a blob to stdout |
| `pith store ls <id>` | list a tree's entries |
| `pith store materialize <id> <dir>` | render a tree into a new directory |

thin over the four `ContentStore` methods and the existing `materialize_tree`. the current `materialize`
takes `--store`, `--tree` and `--output` as flags; positional arguments with `--store` promoted to a
global is the shape the rest of the group wants.

| | |
| --- | --- |
| `pith state info` | schema version, adapter, attempt and reusable-index counts |
| `pith state check` | scan the durable records through their decode validation |

`state check` is thin over what `pith-engine/src/state/records.rs` already does on every read. running it
over a whole store is an integrity command that costs almost nothing to expose.

## gc is one command, and it starts as a dry run

```
pith gc [--dry-run]
```

[0027](../decisions/0027-retention-and-gc.md) names roots, policy axes and *cross-store ordering*.
content and state cannot be pruned independently without leaving dangling references, so splitting this
into `store gc` and `state gc` would contradict the record that defines the operation.

it ships dry-run only. nothing in the tree prunes anything today, there being no collector in
`pith-store`, `pith-state-sqlite` or `pith-engine/src/state`, and 0027's default numeric parameters wait
on workload evidence that does not exist. a dry run reporting what the root set holds and what would be
reclaimed produces that evidence. deletion is its own round, and it needs the retention axis
[the frontend architecture](frontend-architecture.md) names as an amendment 0027 owes: the reusable index
is per-key, each save mints a key, and a permanent root per save is a growth rate nobody has priced.

## linting is severity, not a command

[the frontend architecture](frontend-architecture.md) names the gap. `PithResult<T> = Result<T,
DiagnosticSink>` carries no sink on the `Ok` arm, so a successful parse cannot return warnings, and an
editor with no warnings is not an editor. The CLI half does not invent a second diagnostic path: successful
frontend results already carry their warning list. Wiring incremental publication into the language-server
half remains editor work, not an M-13 command debt.

once warnings ride the `Ok` arm, `check` emits them, and a separate `lint` verb would be a second
diagnostic path drifting from the first. severity lives in `check`, with `--deny warnings` for ci.

four lints have a reader already. an unused import and an unused local definition or declaration are the
cheap ones. an interface this module provides that another in-scope module also provides is the `E-1102`
hazard, warned at check time before registration discovers it. the fourth is stated outright in
[the surface notation](surface-notation.md): a wide positional interface is a signal to take a declared
record, so the notation's own advice becomes a diagnostic.

## formatting, and the property that makes it safe

[0038](../decisions/0038-represented-rule-bodies.md) keeps formatting out of the digest, and the language
frontend notes that the property has never been asserted. it is the property that makes a formatter safe
in a language whose revision is a body digest, so it is the formatter's measured claim: formatting every
corpus `.pi` file leaves every body digest and every module abi digest byte-identical, and `fmt(fmt(x))`
equals `fmt(x)`.

format-on-save in the editor stays out. the architecture flags a format-on-save editor doubling the
key-minting rate against 0027's retention arithmetic, that interaction is unpriced, and it belongs to the
editor. a cli invocation does not mint keys at a keystroke rate.

## where the store and the state live

a repository holds `.pi` source and, from M-14, the locks
[0043](../decisions/0043-the-development-environment.md)'s amendment names. there is no project
directory. store and state are machine-global:

```
$PITH_HOME/
  store/blobs/
  store/trees/
  state.db
```

`$PITH_HOME` defaults to `$XDG_CACHE_HOME/pith`. `--store` and `--state` override the halves separately,
for a hermetic run or a test fixture.

content is machine-global because content addressing already says so: two checkouts of one repository, or
two repositories over one toolchain, compute the same blob and should hold one copy of it. state is
machine-global for the same reason, and that is the half that surprises. a `PureComputationKey` digests
rule identity, revision, interface and encoded inputs, naming no machine and no directory, so an attempt
recorded under one project is valid for any other project whose key matches. scoping state to a directory
discards reuse the keys have already earned, and does it silently.

both halves regenerate from source, so xdg's cache is the right root and deleting either loses no
authored bytes. content admitted through `pith store add` and held nowhere else is the exception, and
0027 owns it.

who may write a store, and how many stores a machine holds once M-5b activates a composed tree onto it,
is a question this document does not answer. it is filed in [open questions](open-questions.md). nothing
here forecloses it, because the two roots are parameters and not compiled-in paths.

## sharing a store between runs

no transport shares a store across machines, and none is scheduled. a ci job caches `$PITH_HOME` as an
ordinary cache artifact keyed on the lock, with one writer per job: sqlite is a single file, so two jobs
over one cache entry produce a lost update no adapter can arbitrate. M-13's read-only adapter path makes
concurrent readers safe and leaves concurrent writers untouched. a build matrix therefore wants one cache
entry per leg.

## globals

`--output pretty|plain|json`, which exists. `--store` and `--state`. every command emits `OutputRecord`s
through `Sink`, so `--output json` is the machine surface, and that is the other half of what keeps the
cli from becoming the api.

`pretty` is a semantic style sheet, not a promise to emit one hard-coded escape sequence. the cli detects
stdout once and derives truecolor, ansi-256, ansi-16, no-color, or no-tty styles from the same palette clap
help and the pretty renderer consume. rgb and 256-color variants are chosen deliberately; ansi-16 remains
symbolic so the terminal's theme can keep it readable. `NO_COLOR`, `FORCE_COLOR`, `CLICOLOR_FORCE`, and
their peers are part of capability detection.

an interactive color terminal gets one bounded theme probe before clap renders anything. the probe asks
for the current default foreground and background with osc 10/11, restores terminal mode on every exit,
and gives up after 100 ms. truecolor hues are then shifted by the smallest amount needed to reach a 4.5:1
srgb luminance ratio against the reported background. ansi-256 chooses the nearest contrast-safe entry
from the fixed xterm cube. ansi-16 selects normal or bright symbolic slots according to the reported
light/dark theme; pith cannot prove their contrast because the terminal owns those slot values. if probing
is unsupported, times out, or color is disabled, the balanced palette remains the fallback. the report is
also the terminal's configured default background, not the desktop visible through a transparent window.

color capability does not choose the output shape: non-tty stdout still defaults to `plain`, even when a
ci log renderer advertises color support, and no theme query is sent for that output.

## what is not here

`pith diff`, `pith update` and `pith add` belong to M-14 and need the lock and the module system. they
should appear in `--help` as requiring a workspace, so the shape is visible before it is built.

the language server is round six's other half, and the second client of the query api. building the cli
alone would leave the api with one consumer and no pressure on it.

entries take no arguments. `pith run test --filter foo` has nowhere to land, because an entry is a name
bound to a request with no parameters. a second entry is the workaround and it does not scale. the
question interacts with computation keys, since an argument is an input and moves the key, so it wants its
own argument and not a mechanism smuggled into the round that lands the notation.
