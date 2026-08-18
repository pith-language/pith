---
schema: design-doc/v1
id: planning-module-surface
title: the module surface
summary: a module publishes its declarations and rule signatures as pi text, the rust crate binds host bodies to coordinates it does not own, and one artifact crosses the boundary
kind: planning
status: draft
created: 2026-08-18
updated: 2026-08-18
tags:
  - planning
  - language
relations:
  informed_by:
    - research-language-frontend
  depends_on:
    - planning-language-frontend
    - decision-0038-represented-rule-bodies
    - decision-0047-the-declaration-table
    - decision-0004-first-party-without-privilege
  supersedes: []
---

# the module surface

this is round one of [the language frontend](language-frontend.md), and it gates every round after it.

## the module surface is the source of truth, and rust binds to it

a module's declarations and its rule signatures are authored in `.pi` text. the rust crate binds a host
body to a coordinate it does not own.

the alternative — rust stays authoritative and each domain emits an interface manifest, the
`.rmeta`/`.hi`/`.pyi` shape — was the close call, and it loses on one sentence in 0047's own unresolved
section: "the surface syntax for declarations — the `.pi` file — is untouched." the notebook already
schedules a written declaration notation. an emitted manifest is therefore a second written notation for
one concept, and the emit step becomes redundant the moment the first one exists. one mechanism per
concern decides it.

two consequences are worth more than the artifact question.

migration stops needing a flag day. 0038 makes a represented rule and a host rule over one interface an
ordinary `E-1102`, so migrating a rule under the manifest design means deleting the rust registration in
the same commit as adding the represented one. with one declaration carrying a tier, the represented body
replaces the host binding, there is never a moment with two providers, and 0015 is never touched.

the language server has no staleness window. rust cannot make a declaration stale, because rust does not
state declarations. the manifest design's staleness between edit and re-emit is unfixable by mechanism,
and it is the rust-analyzer proc-macro complaint reproduced deliberately.

what is not an argument for it, though three of the four candidate designs claimed it: drift soundness.
selection is a hash lookup into `IndexMap<Interface, …>` under derived equality, so a frontend that
elaborates a declaration wrong builds a request whose interface misses the bucket and gets `E-1101`. it
cannot reuse a computation under a wrong key, because there is no rule to key on. declaration drift in pith
fails closed. what the single statement buys is a diagnostic, and the diagnostic matters more than it
sounds: `Display for Type` prints a nominal as bare `coordinate.spelling()`, so a representation mismatch
today renders `xylem.Object` on both sides of the message and never names what differs.

the frontend must print the representation whenever two coordinates print identically. that is the
corpus's worst silent hazard and it is the one place a diagnostic has to teach.

## what crosses a module boundary

one artifact, and this is where three of the four judgments disagreed with each other. the `.pi` surface
bytes are the artifact, published through `ContentStore::put_blob`; everything else is derived from them.
there is no second interface product served beside the source, because that is the notation duplication
just rejected, and it also removes the trust gap a served interface artifact would open.

three digests over it, with three jobs, and getting these confused kills the design's one incrementality
mechanism. the blob `ContentId` says which bytes. the ABI digest says what a dependent's elaboration
consumes: the module name, the grammar and encoding versions, the per-declaration `Declaration::digest`
values in name order, the per-import `(name, digest)` pairs in name order, and the sorted multiset of
`(effect-category tag, Interface::encode_canonical())`. it excludes labels, rule identities, rule
revisions, tier, doc text, formatting and spans. of the three, only the ABI digest may appear in a
computation key. the per-declaration `Declaration::digest` is what a lock witnesses per import.

the multiset is keyed on `(category, interface)` and not on labels because `label` is decorative by test
and because one `Interface` is legitimately registered in two tables — xylem declares `link` and
`link-entry` over the same `types::link_interface()`. a reader that flattened the two arenas would report a
false `E-1102` on xylem as shipped.

excluding labels from the ABI digest is what lets a decorative rename avoid invalidating every dependent.
excluding doc text is what lets a comment edit avoid it. the second is not optional: an `ImportEnv` keyed
on the artifact's `ContentId` rather than on its ABI digest means editing a doc comment in `xylem.pi`
re-elaborates phloem, which is precisely the failure 0047 forbids for a `RuleRevision`. reformatting a file
would do the same.

doc text, spans and formatting ride in the artifact and reach no key, which in turn means a cross-module
hover doc cannot come from the import environment — it comes from the position-carrying sidecar or from the
artifact fetched on demand. hover docs and the cutoff are otherwise in direct competition.

the position-carrying sidecar is a third artifact whose digest participates in no rule revision and which
is no dependency of body elaboration. it is a sibling, the `.hie`/`.cmt` split, advisory by contract and
permitted to be partial when typechecking failed. it carries per-use-site the coordinate the author wrote
before alias expansion, because `Type::of_declaration` returns `target.clone()` for an alias and an alias
therefore has no use-site residue in the IR at all — go-to-definition on an alias use is unimplementable
from the IR, full stop, and this table is the only place it can come from.

## the host tier, and how a rule migrates

a rule declaration carries a tier. `= host` marks a rust body; a represented body replaces it on the same
declaration.

the temptation here is lean's `@[extern]`, where the surface body is "the logical model" and the native
symbol is an override, so migration is deleting the override with no identity change and no revision
change.

that shape is unsound for pith as stated and must not ship. under it the *executed* body is the rust
override while the *derived* revision is over the IR model, so editing the override changes what the engine
computes and moves no key and no revision. 0038 names that case as the one with no spelling left, and 0023
calls it out in so many words: "false invalidation is acceptable; reuse across a possible semantic change
is not."

so: one declaration, one provider, one body. flipping a rule from `= host` to a represented body moves its
`RuleRevision` once, because 0038 derives a represented rule's revision from its body digest while a host
rule's comes from the conservative manifest. that is 0023's acceptable false invalidation and it is the
honest price. what is preserved is the identity — the coordinate survives, so `RuleIdentity` survives —
and the distinction between those two is exactly what 0023 exists to draw. a proposal claiming migration is
not a cache flush is conflating them.

if a dual-body rule is wanted later for differential testing, the sound version is that the override
carries its own `BodyRevision` at the bind site *in addition to* the derived body digest, with both
participating in `RuleRevision`. that is a separate record and it should not be smuggled in as the same
mechanism.

`= host` is visible at the declaration site and in queries, not at the call site. that contradicts
[0018](../decisions/0018-termination-and-recursion.md), which requires an escape hatch to stay "marked at
the call site". the contradiction is real and the amendment is the honest resolution rather than a
workaround: selection is by interface, and a tier inside the interface would make host-to-represented an
API break. so 0018's sentence is amended, and the tooling takes the obligation — an inlay hint or hover
saying which tier answered, which is a rendering and not an interface field. what must not happen is
shipping the construct while leaving 0018's sentence standing.

`phloem.resolve` stays `= host` indefinitely, and the reason is a genuine third case beside 0038's
dichotomy: its state is *code*, two `Box<dyn VersionScheme>` implementations of Debian version algebra, so
it is neither a request input nor part of a representable body. confining it to one visible line is the
whole improvement available.

## where the binding lifecycle lives

not on `Engine`. a loader crate reads a module surface, produces `Rule` values, and calls the existing
public `register_rule` and `register_action_rule`. those two calls stay public and stay the peerhood proof
[0056](../decisions/0056-peerhood-is-a-registered-crate.md) rests on — demoting them behind a test feature
would run U-10's only executable evidence through a surface whose own manifest says it is "not part of the
engine", and cargo's feature unification would make the restriction advisory anyway.

this placement is also what makes the doc correction honest. `design/kernel.md:43` and `scope.md:49` both
put "module and interface linkage" inside the kernel; 0038 has the elaborator resolve imports to digests
before the kernel sees anything. J-3 is decided against those two documents — linkage lives in the loader
layer and the kernel sees elaborated IR — and a proposal that decided that while adding a load-and-seal
lifecycle to `Engine` would be recording the opposite of what it built.

the one kernel-side reachability problem to resolve rather than route around: `encode_length` and
`encode_str` are behind a private `mod manifest`, and `ContentId::with_domain` is `pub(crate)`, which is why
phloem has its own `value_content_id` with a different prefix convention — dotted and per-artifact
versioned rather than colon-separated with a shared segment, and nothing binds the two by test. so either
`ContentId::with_domain` becomes public with a documented prefix discipline, argued once, retiring phloem's
second convention; or the frontend's artifacts use the peer path phloem uses today and take no kernel edit.
what must not happen is one artifact kind taking a kernel table while phloem's five keep the peer path,
because that is two mechanisms with the privileged one newer.
