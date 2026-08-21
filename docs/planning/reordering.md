---
schema: design-doc/v1
id: planning-reordering
title: the reordering
summary: why the milestone sequence should stop adding domain libraries and put the frontend, three ownerless mechanisms, and the observation record in front of them
kind: planning
status: draft
created: 2026-08-21
updated: 2026-08-21
tags:
  - planning
  - milestones
  - language
relations:
  informed_by:
    - research-language-frontend
    - research-tooling
  depends_on:
    - planning-milestones
    - planning-measured
    - planning-language-frontend
    - planning-open-questions
    - foundation-scope
  amends:
    - planning-milestones
  supersedes: []
---

# the reordering

this is a proposal against [milestones](milestones.md), not a record. it changes no claim any milestone
made about what it measured; it changes what comes next and why. it is written after reading the tree
against the milestone statements, and the findings that produced it are stated here as measurements so a
later round can refute them.

## what the tree measures

60,314 lines of rust across fourteen crates, 310 commits, 58 decision records of which 10 are accepted, 23
research notes. M-1 through M-5a all read `status: complete`.

`crates/pith-cli` is 240 lines. its manifest depends on `pith-core`, `pith-engine`, `pith-store`,
`pith-diag`, `pith-ids` and `pith-output`, and on none of `xylem`, `phloem` or `stele`. its two commands
are `pith eval` against an empty rule set and `pith store materialize`. the crates that name a domain
library are that domain's own tests, plus phloem reaching xylem — nothing else in the workspace.

so the three first-party libraries, 23,600 lines between them, are reachable only from test functions
written in the round that built them. `explain_invalidation` is implemented in the engine, in both state
adapters, and in the cross-adapter conformance suite, and is reachable from no command a person can type.
[scope](../foundation/scope.md)'s first useful vertical slice ends at "show why every dependency, rebuild,
configuration value, and deployment action exists", and there is no surface on which to show it.

## the premise that expired

M-1 opens the file with the reason domains come before syntax: "the first implementation should test the
kernel through real domain libraries instead of polishing syntax around an unproven engine." M-4 turned
that into something falsifiable — "M-5a is the first that can confirm or refute convergence, because it is
the first domain whose shapes the calculus was not extended for" — and M-5a answered it: "the calculus
converged. stele declares twelve types over constructors that all predate it and drives no constructor, no
engine or core change, and no encoding version — the first milestone whose convergence entry is an empty
diff."

that was the question domains-first existed to answer, and it is answered. M-5b and M-6 as scheduled would
gather more evidence for a settled question. the questions that can still move the architecture — the
elaborator's latency bet (measured since, by the spike below), whether the ABI cutoff pays for itself,
whether the declaration surface can express the corpus — had no evidence at all and sat behind four
planning documents with no milestone
number, because [the language frontend](language-frontend.md) files itself as "a record sequence, not a
milestone".

that filing is a category error with scheduling consequences. work inside this file gets rounds; work
outside it gets planning documents. the frontend has four planning documents and no commits.

## the ordering constraint nobody has named

round two of the frontend sequence is [0038](../decisions/0038-represented-rule-bodies.md)'s named first
design task: enumerate the IR constructor set and fix its canonical encoding and digest domain.
[the frontend architecture](frontend-architecture.md) states what that set encodes — "the `PureStep`
protocol — `Need`, `NeedAll`, `NeedBlob`, `NeedAction` — is the effect vocabulary of the core ir".

`Observation` has no step protocol, and M-5b enumerates what adding one costs: "new variants in
`PureStep`, `Resumption`, `DependencyEdge`, `ComputationKind`, `DurableComputation` and
`DurableProvenance`". a variant arriving in `PureStep` after the IR encoding is fixed moves the IR body
encoding version, moves every represented body's digest, and moves every `RuleRevision` derived from one.
designing the constructor set while two of five effect categories have no step is designing in an
amendment.

M-5b already separates the two halves — it "opens with a record rather than a prototype, because the shape
of the problem is not implementation" — so the record detaches from the activation library cleanly. it
belongs before the IR constructor set rather than two milestones after it. the current order cannot see
this because the frontend is not in the sequence.

nothing is released, so the amendment would be cheap to take late. the reason to take it early is not the
migration cost, it is that a constructor set designed against a vocabulary known to be
incomplete records a design nobody argued.

## three mechanisms with no owner

the backstop limit. there is no `Duration`, timeout or deadline anywhere under `crates/*/src`; the only
occurrences in the workspace are in `pith-engine/benches/scale.rs`.
[0018](../decisions/0018-termination-and-recursion.md) requires it, 0022 requires it,
[0028](../decisions/0028-sandboxed-local-executor.md)'s unresolved section claims the executor already
accepts one, M-5a's image tools want one, M-6's mutations cannot be unbounded, and the configure-script
case in [the language frontend](language-frontend.md) is blocked on it. one mechanism owed to five callers is
one record under one-mechanism-per-concern, not five deferrals. it currently lives as a paragraph inside
M-6, the furthest-out milestone that needs it.

`Opaque`'s step protocol. `pith-core/src/effect.rs` carries `pub struct Opaque;` and
`effect_category!(Opaque, false)` and nothing else. U-2's gradual adoption is unbuildable without it,
[0032](../decisions/0032-action-granularity.md) settled the granularity for an import nobody can test, and
the generic builder needs it for the reason 0032's own text gives — an `Action`'s contract claims what
happened, and for a foreign build that claim is false, so the run overclaims in provenance rather than
failing. no milestone owns it.

the read-only state adapter. [the language frontend](language-frontend.md) names it in one line — "an
editor and a CLI at one store root, against a sqlite state store with one lock and `synchronous=full`,
needs a read-only adapter path" — and it is a hard blocker for the language server, found in planning and
owned by nobody.

each is cheaper before its caller than after, and each is currently scheduled after all of them.

## the gap between M-4 and a user

[the language frontend](language-frontend.md) states it plainly: "the missing piece is a peer domain
library in the shape of `stdenv.mkDerivation`, not a language feature ... that library is an ordinary peer
under 0004 and 0009, which is the right architecture and also means it is unbuilt work no milestone covers
— M-5a is system composition, M-5b activation, M-6 deployment, M-7 broader execution, and the gap sits
between M-4 and anything a person would use."

the planning set diagnosed this and the sequence walked past it. the reordering gives it a milestone.

## what M-5a earned and did not collect

48 of 58 records were `proposed`. that ratio is not itself a problem — records are promoted against a
criterion they set for themselves — but the convergence result was sitting in a milestone paragraph
instead of in the records it bears on, which meant the notebook's own confidence signal was not tracking
the code.

[0015](../decisions/0015-interface-rule-selection.md) is now `accepted`. its unresolved section asked for
a prototype of the exact interface fields and four domains supplied one, and every collision it predicted
was answered the way the record says it should be — by making the types distinguish the rules, never by a
rule that picks. two items stay open there and neither blocks acceptance: both are call-site spellings
that wait on the notation M-13 delivers.

[0026](../decisions/0026-generic-typed-calculus.md) is not promoted, and the distinction matters more than
the promotion would have. what M-5a measured is that the *built* constructor set converged. the record
mandates rank-1 prenex polymorphism, the kernel cannot hold it — `Interface` is two concrete `Type`s and
selection is exact equality — and the uncertainty constructors are unbuilt. so a mandated feature has no
implementation and no spelling, and accepting the record would claim the parametric half on evidence that
covers only the part every domain has used. the measurement is recorded in the record; acceptance waits on
the amendment [the language frontend](language-frontend.md) already names, that generics have no surface.

an earlier draft of this proposal said both should be promoted. that was wrong about 0026 for the reason
above, and the correction stays here instead of being quietly dropped, because "the calculus converged" and "the
calculus is accepted" are exactly the two sentences this notebook is built to keep apart.

one correction went alongside. [open questions](open-questions.md) listed "arbitrary-precision `Int`,
deferred to the record that lands arithmetic" among what stands between the calculus and a surface syntax.
0055 landed it, and the gating section overstated what was in the way.

## milestones.md has stopped being a plan

M-2's evidence is a single paragraph of roughly 1,400 words, explicitly append-only, which the file itself
says contains clauses "overtaken inside M-3 without being retracted". roughly four fifths of the document
is retrospective evidence for finished work and the forward plan is four short sections. a sequence cannot
be reordered by someone who has to read eight thousand words of changelog to find it.

the mechanical half is done. the evidence paragraphs moved unchanged into
[what the completed milestones measured](measured.md), and [milestones](milestones.md) now holds the
order, the gates and what each milestone still owes. what stayed in the milestone file is the
forward-looking half of each completed milestone, because what a milestone owes is a claim about work
rather than about evidence.

## the case against

the strongest objection is M-1's own premise, turned around: do not polish syntax around an unproven
engine. the frontend sequence is seven records plus eight amendments and not one line of it is prototyped,
which is enough work to stall the project for months with no domain evidence arriving.

two things answer it. the engine is no longer unproven, per the expiry above. and the sequence is not
a prerequisite chain in which nothing ships until round four —
[the module surface](module-surface.md) says round one ships on its own: "step one — declarations only,
every body `= host` — ships inside round 1, and at that point `import xylem` typechecks, completion and
hover and go-to-definition work, and not one line of rust has moved."

the residual risk is real and it is concentrated in one place: round one gates rounds two through seven,
so a slip there slips everything. that argues for putting the cheap measurement in front of round one, not
for deferring the sequence.

## the proposed order

a spike, before anything, and it is not a round. measure the latency bet.
[the frontend architecture](frontend-architecture.md) calls it "the design's central latency bet", and
[0021](../decisions/0021-arena-graph-engine.md) forecloses the obvious fallback by an accepted record.
generate two hundred modules from stele's declaration shapes, write a throwaway parser and elaborator, and
get one number against the 50 ms bracket. the code is discarded and the number goes into the frontend
architecture as measured or refuted. if the bet fails the frontend changes shape, and better to know that
before the round it gates than after. it ran on 2026-08-21 and the bet holds — the edited module's path
measured about 106 µs at p50 against the 50 ms bracket — and the number is in the frontend architecture.

the labels below are new, not renumbered, and that is a decision here rather than a convenience. five records and a research note cite `M-5b`, `M-6` and `M-7` with weight — 0051 on "the time
and resource bounds M-6 already owes four callers", 0052 on "the deployment library M-6 opens", 0040 on a
decision that "binds M-6 as much as M-4", the system-composition note placing assertions over a target "in
M-5b, where the observation and mutation effects exist". renumbering those would edit record prose to
express an ordering. so a label is an identity and the file states the order, which is
[0023](../decisions/0023-rule-and-cache-identity.md)'s distinction applied to the plan.

1. M-8, the backstop limit. one record, five callers. it also retracts 0028's claim to have one, and
   discharges M-2's remaining timeout debt.
2. M-9, observation identity and freshness. the record only, detached from M-5b, so the step vocabulary is
   closed before the IR encodes it. whether it ships with one thin observation prototyped beside it is the
   second fork below.
3. M-10, the declaration artifact and [the module surface](module-surface.md). frontend round one. it gates
   everything after it, needs no unbuilt constructor, and ships hover, completion and go-to-definition on
   its own. it also discharges phloem's lazy declaration table.
4. M-11, the IR constructor set. frontend round two, now enumerated against a closed vocabulary.
5. M-12, the elaborator and the three graph rules. frontend round three. the ABI cutoff is measured here,
   and it decides whether the graph tier exists at all.
6. M-13, the surface notation and the CLI. frontend round four and the CLI half of round six. `pith check`
   and a real `pith build` first exist here, `explain_invalidation` becomes something a person can see, and
   the read-only state adapter is needed.
7. M-14, [the module system](module-system.md). frontend round five.
8. M-15, the generic builder. the `stdenv.mkDerivation`-shaped peer, authored in `.pi`, which is what makes
   packaging a real program tractable. `Opaque`'s step protocol lands here, because a foreign build is the
   caller that needs it, and M-8 comes first because a hung configure is unbounded.
9. M-5b, Linux system activation. the prototype half, over a designed observation protocol instead of an
   assumed one.
10. M-6, deployment. unchanged, and still gated on the ordering contradiction in
    [scope](../foundation/scope.md). that research round depends on nothing here and can run in parallel at
    any point.
11. M-7, broader execution. unchanged.

what this moves: two domain libraries go behind the frontend, three ownerless mechanisms get owners, the
observation record moves ahead of the IR enumeration for a structural reason rather than a preference, and
the first point at which a person can use the thing moves from unscheduled to M-13.

## two forks this proposal does not close

the IR-and-notation fork. M-11 and M-12 can be built against hand-constructed IR with no notation at all —
0038's own first frontend is "the rust registration API over hand-built ir". so M-13 is not required by
anything the kernel needs and is required only by a user. stopping after M-12 is a coherent plan if the
goal is proving the design; the expiry above is the argument that the design-proving budget is spent, and
it is an argument rather than a measurement.

the observation-prototype fork. scheduling M-9 as a record with no implementation puts a variant into
`PureStep` that nothing exercises until M-5b, and an unexercised step variant is a guess. the alternative
is one thin observation — a file's mtime is the cheapest — prototyped beside the record, which prices the
variant honestly and costs perhaps a third more round. this proposal does not pick.
