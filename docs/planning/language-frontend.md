---
schema: design-doc/v1
id: planning-language-frontend
title: the language frontend
summary: why the frontend is three separable projects, what each is gated on, and the record sequence that builds them
kind: planning
status: draft
created: 2026-08-17
updated: 2026-08-24
tags:
  - planning
  - language
  - modules
  - tooling
relations:
  informed_by:
    - research-language-frontend
    - research-tooling
    - research-declarations
    - research-dispatch
  depends_on:
    - planning-open-questions
    - planning-reordering
    - planning-module-surface
    - planning-module-system
    - planning-frontend-architecture
    - planning-surface-notation
    - decision-0026-generic-typed-calculus
    - decision-0038-represented-rule-bodies
    - decision-0047-the-declaration-table
  supersedes: []
---

# the language frontend

this is a proposal, not a record. it spans four sibling documents plus this one, decomposes into roughly seven
records and eight amendments, named at the end, and no part of it is prototyped. it is in `planning/` for the
same reason [milestones](milestones.md) is: it is an ordering over work whose individual claims belong to
records.

## three projects, not one

"a language frontend" is three separable things, and conflating them is why the work looks further away
than it is.

the module surface publishes what a domain declares, so that something which has not linked the domain's rust
crate can know that `xylem.Object` exists and what it is. today nothing can: `Declaration`'s fields are
private, its only construction site is `DeclarationTable::declare`, there is no
`Declaration::encode_canonical`, no table encoder, and no declarations table in the sqlite schema. every
ingredient is built and none is assembled. this is the gate on everything else, and it is not blocked by any
part of the 0026 calculus that is missing.

the core IR replaces rust closures as what the kernel evaluates. [0038](../decisions/0038-represented-rule-bodies.md)
settles its shape and explicitly does not enumerate its constructor set — "that enumeration is the first
design task this record gates". it does not sit behind surface syntax: 0038's own first frontend is the
rust registration API over hand-built IR.

the surface notation is the parser, elaborator, module system and editor support. it is the only one of the
three that needs a lexer, and it is the last one gated.

[open-questions](open-questions.md) gates surface syntax on "the 0026 calculus landing in the core". that
gate is correctly sized for the notation and oversized for the other two. what is missing from the
calculus is type parameters, `Map`/`Option`/`Result` and the five uncertainty constructors — and
[0047](../decisions/0047-the-declaration-table.md) removed the first three from 0026's set outright and
gates each uncertainty constructor on the subsystem that would read it. the constructors the existing
corpus actually uses — the six scalars, `Nominal`, `List`, `Record`, `Sum`, `Cut` — are all built.

the declaration surface is buildable today.

four sibling documents carry the designs these rounds discharge. [the module surface](module-surface.md)
holds the declaration artifact: the encoders, the digests, the `.pi` declaration grammar, the loader, and the
two-phase intra-module pass. [the module system](module-system.md) holds identity, `module.pi`, the four
source kinds, the registries, the lock, and the scoped universe. [the frontend architecture](frontend-architecture.md)
holds the syntax, HIR and elaborator crates, the three graph rules, the IR constructor set they produce, and
the tooling and query surface built over them. [the surface notation](surface-notation.md) holds the
spelling — the calling convention, the name lexeme, the operator and builtin sets, local definitions, `about`
blocks, and the generics decline. what stays here is the ordering, the gates, and the ledger of kernel changes
the sequence aggregates.

## what this proposal does not settle

these are named because a proposal that omits them is claiming more than it has.

generics have no surface. 0026 mandates rank-1 prenex polymorphism and the kernel cannot hold a
polymorphic rule: `Interface` is two concrete `Type`s and selection is exact equality. so the surface
declines type parameters for now, and 0026 needs an amendment saying so rather than leaving a mandated
feature with no spelling.

effect categories beyond pure and action have no notation, because they have no step protocol. `Opaque`
is 0019's adoption on-ramp and exists only as a marker type, so the import-an-existing-build-system story
that 0032 settled the granularity for is still untestable.

a generic builder library does not exist, and the language is not what would fix it. this is the
sharpest thing the proposal was missing, and it is worth stating in full because it sets the order of the
remaining work. 0032 already decided how a foreign build system is packaged — "a foreign build system is one
`Opaque` and one tool invocation is one `Action`, declared per target rather than inferred" — so packaging an
autotools program is one boundary that runs its own build, not an enumeration of its compiles. what exists
instead is 0045's shape, where "the package's declared build running as one pure rule over xylem's compile
and link entries" produces an executable, which proves the identity, lock and substitution machinery against
a tree the fixture compiles itself, file by file. that is not a packaging story, and reading it as one
produces exactly the wrong example.

three things block the right form, and none of them is notation. the first is the seccomp allowlist: 0028's
filter admits 77 syscalls each measured from a compiler, the linker or a shell fixture, and a configure script
runs hundreds of small probe programs doing what a compiler never does; 0028 already predicted the list widens
per toolchain and it has been measured twice, clang alone needing `sigaltstack`, `rename` and `alarm`. the
second was the absent timeout, since a hung configure is unbounded — no longer absent: M-8's run bound
([0059](../decisions/0059-a-caller-declared-run-bound.md)) kills a hung child at the run's deadline. the
third is `Opaque`'s lack of a step
protocol, and it is the subtle one: running a foreign build as a plain `Action` works, but an `Action`'s
contract *claims* these inputs and these outputs and this is what happened, and for a foreign build the last
clause is false, so the run does not fail — it overclaims in provenance, and 0014's reproducibility properties
rest on that claim being honest.

past those three, the missing piece is a peer domain library in the shape of `stdenv.mkDerivation`, not a
language feature: nix's answer to "why is packaging easy" is its generic builder, and nobody packages
software there by writing per-compile derivations either. that library is an ordinary peer under 0004 and
0009, which is the right architecture and also meant it was unbuilt work no milestone covered — M-5a is
system composition, M-5b activation, M-6 deployment, M-7 broader execution, and the gap sat between M-4 and
anything a person would use. [the reordering](reordering.md) took that finding as one of its four and gave
the library milestone M-15, with `Opaque`'s step protocol landing there because a foreign build is the
caller that needs it, and the backstop limit ahead of it as M-8 because a hung configure is unbounded.

the language is the last item in that chain, not the first.

secret taint has no owner. the notebook's requirement is that no literal secret can flow into a digest,
and a `.pi` file is the worst possible place for one, because its bytes are a content identity in a store
with a retention policy that keeps roots indefinitely. the surface needs to make a literal secret
unspellable, and nothing here does that.

IR well-formedness and termination validation: the kernel re-checks types at four places, so the
type half is covered. whether the engine must reject a malformed or non-total IR body it did not elaborate —
and it must, since a represented body can arrive from a file — is unaddressed by 0038 and by this.

formatting: U-3 names it and no record mentions a formatter or a canonical printed form. 0038 makes it
safe by keeping formatting out of the digest, but the property has never been asserted, and it is the one
property that makes a formatter safe in a language whose revision is a body digest. the interaction with
0027's retention arithmetic, a format-on-save editor doubling the key-minting rate, is unpriced.

the CLI is `pith eval <label>` against an empty rule set plus `pith store materialize` today, and it
registers no domain. a language needs `check`, `fmt`, `explore`, `diff`, `update` and an evaluator, and
`research/tooling.md` requires the CLI to be one client of a versioned query API rather than the API itself.

intra-module structure, forward references between declarations in a multi-file module, and whether
declaration order in the text may differ from registration order are named and not answered. 0047 leaves
mutual recursion between sums needing "either a two-phase table or a module-level elaboration pass". a
module-level elaboration pass is exactly what a frontend is, so this round discharges that item and has to
say how.

bootstrap ordering has not been checked. the residual host surface is a closed set — lexer, parser,
elaborator, the resolution protocol, the store — and the checkable claim is that no `.pi` file is required to
load a `.pi` file.

process concurrency has not been designed. an editor and a CLI at one store root, against a sqlite state
store with one lock and `synchronous=full`, needs a read-only adapter path.

no performance numbers exist. the requirements put editor latency and graph-query latency in the benchmark set
and warn that "a fast clean demo is weak evidence", and no target exists for elaboration throughput,
cold-start, completion latency or memory per module.

deferred with a named gate: macros, codegen and compile-time evaluation — the answer is no and it should
be written down, since 0026 explicitly leaves comptime to a surface-language decision; a REPL; how the
surface itself is versioned when the calculus is closed by record, which is U-4's obligation; importing
non-pith data such as JSON or TOML, which is probably a rule and not a language feature; and
cross-repository and released-version module identity, which four records defer to nobody and which 0023
leaves open while 0038 depends on it.

two things stated as gaps rather than answered: what defines a project as opposed to a module, and
which module the builtin declarations live in.

## records, in gatedness order

when this was written no milestone covered the work — M-1 through M-7 were semantic, domain and execution
milestones — and open-questions gated surface syntax on the calculus rather than on a milestone. so it was
filed as a record sequence.

that filing turned out to have a scheduling consequence rather than being a neutral description: work
inside [milestones](milestones.md) gets rounds and work outside it gets planning documents, and this
proposal accumulated four documents and no commits. [the reordering](reordering.md) puts the sequence into
the milestone track. the rounds below are unchanged and their measured claims stand; what each gained is a
label — round one is M-10, round two M-11, round three M-12, round four M-13 together with the CLI half of
round six, and round five M-14. round seven, migration, stays distributed the way it is described here:
step one inside round one, and the two body migrations as their own rounds after the notation exists.

1. the declaration artifact and [the module surface](module-surface.md). `Declaration::encode_canonical`, a
   table encoder, the ABI and revision digests, the `.pi` declaration grammar (three forms plus effect-typed
   rule signatures and `= host`), the loader crate, the two-phase intra-module pass. *measured claim:* the four
   crates' live tables and their `.pi` counterparts agree digest-for-digest, and xylem's nine rule revisions
   are bit-identical before and after. this round also has to fix phloem's lazy declaration table, which
   registers on first use and whose own doc says `registered()` returns "the set the crate has reached, not
   the set it will eventually hold" — so an out-of-process consumer asking phloem what it declares gets
   different answers on different runs, and one coordinate it names is not in the table at all. it gates
   everything.
2. the IR constructor set, designed in [the frontend architecture](frontend-architecture.md). 0038's named
   first design task, plus the canonical encoding, the digest domain, and the validator. *measured claim:*
   every corpus rule body that can be expressed is expressed, and the round names the ones that cannot.
3. [the frontend architecture](frontend-architecture.md). the syntax, HIR and elaborator crates; the three
   graph rules; the derived elaborator revision; the `NeedBlob` entry-point question. *measured claim:* the
   ABI cutoff — edit a body in A, assert `bodies-of(B)`'s key is byte-identical and the reusable lookup hits.
4. [the surface notation](surface-notation.md). the calling convention, the name lexeme, the operator and
   builtin sets, local definitions, match-as-handler-record, `about` blocks, the generics decline. *measured
   claim:* `example-domain` — four nominals, one interface, two rules, built so that no crate names it — as
   one `.pi` file producing byte-identical interface encodings and an identical contract-test result, plus
   equal body digests under qualified and unqualified spelling.
5. [the module system](module-system.md). identity and the deferred domain-authority question; `module.pi`;
   the four source kinds as adapters; configured forge sugar; domain-bound registries with no search order;
   workspaces as the bootstrap locator; the lock; the scoped universe; `pith diff`; and the
   index-versus-no-index fork argued rather than assumed. *measured claim:* publish `example-domain` to a
   temporary registry and assert byte-identical elaborated IR against the local-path entry, plus the two
   configuration refusals — a domain claimed twice refused before any network access, and a domain no registry
   serves refused with no fallback.
6. tooling, designed in [the frontend architecture](frontend-architecture.md). `pith check`, `fmt`, `explore`,
   `run`, the entry construct, the query surface, the server, the read-only state adapter. *measured claim:*
   keystroke-path latency on two hundred generated modules built from stele's declaration shapes, plus
   formatter idempotence and semantic preservation.
7. migration, designed in [the module surface](module-surface.md). step one — declarations only, every body
   `= host` — ships inside round 1, and at that point `import xylem` typechecks, completion and hover and
   go-to-definition work, and not one line of rust has moved. steps two and three are their own rounds:
   `stele.render-passwd` first, being a pure projection with no yields over a domain with no rule state, then
   xylem's compile entry, which is 0038's named first migration and the honest hard one. *measured claim:* a
   differential harness, and it only covers rules whose inputs a test can synthesize.

amendments these rounds require, several of which nobody has named as amendments: 0018 (the escape hatch
is marked at the declaration site, not the call site); 0026 (generics have no surface); 0027 (a
retention axis scoped by rule identity or tier); 0038 (an action rule's represented form, which 0038 does
not cover, and the dual-body rule if it is ever wanted); 0043 (`<name>.pith.lock` for every environment
including the default); 0053 (frontend diagnostics carry their own source at the render boundary — an
amendment to its reasoning, argued, not smuggled); 0057 (the two-engine construction in place of
unregistration); and `design/kernel.md:43` with `scope.md:49` (module and interface linkage lives in
the loader layer). the backstop limit is a gate rather than an amendment, and it is now owed to five callers.

## the kernel-change ledger

the project treats a kernel change as a finding to be argued rather than assumed, and the aggregate here is
larger than any one round suggests. `pith-core`: `Declaration::encode_canonical` and a table encoder; a
`tier` field and module attribution on `Rule`, both owed by 0038 and unbuilt; a dot refusal on declaration
names; interned coordinates. `pith-ids`: one or more digest domains, which is the `with_domain` reachability
question above. `pith-engine`: `EngineQuery::action_rules`, which is `pub(crate)` today so nothing can
enumerate action rules at all; an interface-index accessor, since `by_interface` has none; `Engine::get_blob`,
whose absence 0056 already names; and possibly the `NeedBlob` entry point. `pith-diag`: cross-file notes, a
cached line index — `line_col` builds a fresh one per call — warnings on the `Ok` arm, and a frontend code
range.

`scope.md`'s inclusion rule is that a feature belongs in the kernel "when letting each domain implement it
separately would make cross-domain composition incorrect, unsafe, or impossible to explain". most of the
ledger passes: identity, digests, diagnostics, the rule table. what does not obviously pass is anything
placed in the kernel for implementation convenience — reachability of a private encoder is not a composition
argument — and 0001's warning that the kernel boundary "cannot become a hiding place for useful first-party
behavior" is the one to test each item against.
