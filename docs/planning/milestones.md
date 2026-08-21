---
schema: design-doc/v1
id: planning-milestones
title: milestones
summary: the order the work runs in, what each milestone owes, and why the sequence puts the frontend ahead of the remaining domain libraries
kind: planning
status: draft
created: 2026-03-23
updated: 2026-08-21
tags:
  - planning
  - milestones
relations:
  informed_by:
    - foundation-scope
  depends_on:
    - planning-open-questions
    - planning-reordering
    - planning-measured
  supersedes: []
---

# milestones

this file states an order and what each milestone owes. the evidence for the milestones that closed is in
[what the completed milestones measured](measured.md), moved there unchanged. the argument for the current
order is in [the reordering](reordering.md), and the short version is at the end of this section.

## labels are identities, not positions

a milestone label is a citation handle. five records and a research note cite `M-5b`, `M-6` and `M-7` with
weight: 0051 names "the time and resource bounds M-6 already owes four callers", 0052 names "the deployment
library M-6 opens", 0040 says a decision there "binds M-6 as much as M-4", and the system-composition note
places assertions over a target "in M-5b, where the observation and mutation effects exist". renumbering
would falsify record prose in order to express an ordering. in a repository whose records are meant to
stay put, that is the wrong trade.

so labels do not move and do not imply position. that is
[0023](../decisions/0023-rule-and-cache-identity.md)'s distinction applied to the plan rather than to a
rule — a stable identity, and a separate thing that moves. the order is the order of this file, and it is
stated once, here:

M-1, M-2, M-3, M-4, M-5a, M-8 and M-9 are complete, and the latency spike has run. then M-10,
M-11, M-12, M-13, M-14, M-15, and then M-5b, M-6 and M-7.

## why the order changed

the earlier sequence ran the domain libraries out to deployment before anything read them, on the premise
M-1 opened this file with: "the first implementation should test the kernel through real domain libraries
instead of polishing syntax around an unproven engine." M-4 made that falsifiable and M-5a answered it —
the calculus converged, stele driving no constructor, no engine or core change and no encoding version.

that premise has therefore been discharged rather than abandoned. what is left unmeasured is not whether
the kernel holds a new domain but whether a person can reach it: `pith-cli` is 240 lines, depends on none
of `xylem`, `phloem` or `stele`, and offers an evaluation stub and content materialization, so three
first-party libraries are reachable only from the tests written in the round that built them, and
`explain_invalidation` is implemented four times over and typeable nowhere. the questions that can still
move the architecture are all in front of that boundary.

## the latency spike

not a milestone and not a round. [the frontend architecture](frontend-architecture.md) rests on a bet it
names as such — the design's central latency bet — that no in-process incremental layer is needed because
re-elaborating an edited module is fast enough, with sorbet as the control case.
[0021](../decisions/0021-arena-graph-engine.md) forecloses the fallback by an accepted record, so a failed
bet changes the frontend's shape rather than its tuning.

it ran on 2026-08-21, before M-10, which gates everything after it: two hundred modules generated from
stele's declaration shapes, a throwaway parser and elaborator, measured against the 50 ms keystroke
bracket. the bet holds — the edited module's path is 105.6 µs at p50 and 117.3 µs at p99, about four
hundred and twenty times inside the bracket, and the whole world elaborates cold in 21.9 ms. the number,
its boundaries and one arithmetic correction it found in the architecture's sorbet paragraph are recorded
in the frontend architecture; the code is discarded.

## M-9: observation identity and freshness

status: complete. the evidence is in [measured](measured.md), and
[0060](../decisions/0060-observation-identity-and-freshness.md) is the record.

the record M-5b opens with, detached from the activation library it was going to open.

M-5b already separates the two, "because the shape of the problem is not implementation":
`Observation::CACHEABLE_AS_RESULT` is false, so an observation has no computation key, so 0033's equality
pruning has no analogue on an observation edge and every consumer of one would be permanently
non-reusable. what an observation's identity and freshness are is the question.

it moves ahead of the frontend for a structural reason rather than a preference. M-11 fixes the IR
constructor set, and [the frontend architecture](frontend-architecture.md) states what that set encodes —
"the `PureStep` protocol — `Need`, `NeedAll`, `NeedBlob`, `NeedAction` — is the effect vocabulary of the
core ir". M-5b enumerates the cost of an observation step: "new variants in `PureStep`, `Resumption`,
`DependencyEdge`, `ComputationKind`, `DurableComputation` and `DurableProvenance`". a variant arriving in
`PureStep` after the IR encoding is fixed moves the IR body-encoding version, every represented body's
digest, and every `RuleRevision` derived from one.

nothing is released, so taking that amendment late would be cheap. the reason to take it early is not the
migration cost — it is that a constructor set enumerated against a vocabulary known to be incomplete
records a design nobody argued.

the fork is picked: one thin file-mtime observation ships as the prototype witness, so the step, async
adapter boundary, durable record and freshness admission are exercised rather than inferred.

## M-10: the declaration artifact

round one of [the language frontend](language-frontend.md), designed in
[the module surface](module-surface.md). `Declaration::encode_canonical`, a table encoder, the ABI and
revision digests, the `.pi` declaration grammar, the loader crate, the two-phase intra-module pass.

*measured claim:* the four crates' live tables and their `.pi` counterparts agree digest-for-digest, and
xylem's nine rule revisions are bit-identical before and after.

it gates every milestone after it and is blocked by no unbuilt constructor —
[0047](../decisions/0047-the-declaration-table.md) removed type parameters, `Map`, `Option` and `Result`
from 0026's required set, and 0055 landed the arbitrary-precision `Int` that
[open questions](open-questions.md) still listed as outstanding. it also ships on its own: with
declarations only and every body `= host`, `import xylem` typechecks and completion, hover and
go-to-definition work with no rust moved.

it discharges phloem's lazy declaration table, whose own doc says `registered()` returns "the set the crate
has reached, not the set it will eventually hold", so an out-of-process consumer gets different answers on
different runs and one coordinate it names is not in the table at all.

## M-11: the IR constructor set

round two. [0038](../decisions/0038-represented-rule-bodies.md)'s named first design task — the constructor
enumeration it explicitly declined to make — plus the canonical encoding, the digest domain, and the
validator.

*measured claim:* every corpus rule body that can be expressed is expressed, and the round names the ones
that cannot.

it follows M-9 so the step vocabulary is closed before the encoding fixes it.

## M-12: the elaborator and the three graph rules

round three, designed in [the frontend architecture](frontend-architecture.md). the syntax, HIR and
elaborator crates; `interface-of`, `bodies-of` and `index-of`; the derived elaborator revision; the
`NeedBlob` entry-point question.

*measured claim:* the ABI cutoff — edit a body in A, assert `bodies-of(B)`'s key is byte-identical and the
reusable lookup hits.

that measurement decides whether the graph tier exists at all. the architecture says so itself: if it
fails, "the graph tier collapses into a single host rule that the CLI and the server both call, with the
local layer untouched."

## M-13: the surface notation and the CLI

round four and the CLI half of round six, folded, because a notation nobody can invoke is not testable by a
person.

*measured claim:* `example-domain` as one `.pi` file producing byte-identical interface encodings and an
identical contract-test result, plus equal body digests under qualified and unqualified spelling.

this is where `pith check` and a real `pith build` first exist and where `explain_invalidation`,
`plan_action` and `select` — three query surfaces no other language server has, all built and all
unreachable — become something a person can see. it needs the read-only sqlite adapter path that
[the language frontend](language-frontend.md) names in one line and no milestone owned: "an editor and a
CLI at one store root, against a sqlite state store with one lock and `synchronous=full`".

## M-14: the module system

round five, designed in [the module system](module-system.md). identity and the deferred domain-authority
question; `module.pi`; the four source kinds as adapters; domain-bound registries with no search order;
workspaces; the lock; the scoped universe; `pith diff`; the index-versus-no-index fork argued rather than
assumed.

*measured claim:* publish `example-domain` to a temporary registry and assert byte-identical elaborated IR
against the local-path entry, plus the two configuration refusals.

## M-15: the generic builder

the peer domain library in the shape of `stdenv.mkDerivation`, authored in `.pi`.

[the language frontend](language-frontend.md) diagnosed this and no milestone covered it: "the missing
piece is a peer domain library in the shape of `stdenv.mkDerivation`, not a language feature ... it is
unbuilt work no milestone covers ... and the gap sits between M-4 and anything a person would use." 0045's
shape — a package's declared build running as one pure rule over xylem's compile and link entries, file by
file — proves the identity, lock and substitution machinery and is not a packaging story.

`Opaque`'s step protocol lands here, because a foreign build is the caller that needs it. `Opaque` is
`pub struct Opaque;` and `effect_category!(Opaque, false)` and nothing else, U-2's gradual adoption is
unbuildable without it, and [0032](../decisions/0032-action-granularity.md) settled the granularity for an
import nobody can test. the subtle half is 0032's own: running a foreign build as a plain `Action` works,
but an `Action`'s contract claims what happened, and for a foreign build that clause is false, so the run
overclaims in provenance rather than failing.

M-8's backstop is a hard prerequisite here — a configure script runs hundreds of small probe programs and a
hung one is unbounded — and so is the allowlist widening 0028 predicted and has measured twice.

## M-5b: Linux system activation

install a composed artifact onto a running machine, and switch to it.

this is the half that needs `Observation` and `Mutation`. its opening record is now M-9 and runs ahead of
the frontend; what stays here is the prototype, over a designed observation protocol rather than an assumed
one.

the mechanical cost is enumerable: the new variants M-9 names, a freshness concept that exists nowhere in
the tree, and a `computations` table that encodes its binary as a nullable digest-column pair and needs a
shape for five categories. under 0048 no encoding version moves for it; the pre-release database is
discarded.

## M-6: deployment library

observe one machine, derive a plan, apply it, confirm the result, and return to an earlier realization.
secrets use references resolved at the target.

this milestone is blocked by a contradiction in the foundation rather than by missing code, and naming it
here is cheaper than discovering it at the start of the round. S-4, S-5 and S-6 want a derived operation
sequence carrying destructive effects, temporary states, preconditions, invariants, compatibility windows,
completion evidence, and rollback limits. the graph's only sequencing construct is data dependency: `Need`
orders a child because its value is required, and `NeedAll` declares a batch explicitly independent.
neither expresses "this must happen before that, and if it fails, undo it." and scope.md says "the project
is also not an ordered task runner ... users should not have to turn the desired system into a hand-written
sequence," which is a good principle and is also the reason no ordering primitive exists.

so the first work here is a research round and a record that decides one of two things: whether a
precondition and a rollback limit are expressible as declared inputs and derived values, keeping scope.md
intact, or whether scope.md's sentence needs amending. systemd's distinction between ordering and
requirement dependencies is the sharpest primary source, with Terraform's plan graph and Kubernetes'
level-triggered reconciliation as the named alternatives.

that research round depends on nothing else in this file and can run in parallel from any point. deciding
it early removes a phantom dependency from everything downstream, the reason the older form of this
paragraph gave for settling it before M-5b.

the time and resource bounds this milestone also needed are M-8 and are no longer owed here.

## M-7: broader execution

only after the vertical slice works: remote execution, additional operating systems, multi-machine
placement, continuous reconciliation, and richer transition protocols.

## complete

M-1 semantic prototype, M-2 action prototype, M-3 first build library, M-4 package and environment
libraries, M-5a Linux system composition, M-8 the backstop limit and M-9 observation identity are complete at their own scopes.
their evidence is in [what the completed milestones measured](measured.md).

what they still owe is a claim about work, not about evidence, so it stays here:

M-1 owes nothing. persistent storage, change pruning and invalidation explanations went to M-2 as that
paragraph said they would. operational support for `Observation` landed in M-9; `Mutation` and `Opaque`
remain scheduled in M-5b and M-15.

M-2 owed timeouts and partial cancellation. the timeout half is discharged in M-8 and
[0059](../decisions/0059-a-caller-declared-run-bound.md); the partial-cancellation half stays open beside
it, as 0059's unresolved section names, and is what still holds 0022 from acceptance.

M-3 owes nothing its own statement named. what remains is scale rather than capability — the fixture is a
handful of files, not a small project — and M-15 is where a real one comes from. U-5's remainder is
separate and still partly ownerless: multi-language targets and a check concept distinct from a test have
no milestone, and M-7 holds the remote-execution half.

M-4 owes nothing, after the four rounds that followed its close: 0044, 0045, 0046 and then 0053 and 0054.

M-5a owes nothing. it is the milestone that answered the convergence question M-4 posed, which is why the
order after it is different from the order before it.

M-8 owes nothing its own statement named. what it leaves beside it is named in 0059's unresolved section:
partial cancellation, a bound and a cancel signal in one run, the rlimit half of the resource bound, and
whether the pure-only entry point carries a bound once represented bodies evaluate on it.
