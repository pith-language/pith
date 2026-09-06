---
schema: design-doc/v1
id: decision-0068-published-surfaces-are-context-bound
title: a published module surface is a claim about one dependency context, and an added provider is not a minor change
summary: a consumer's ABI and interface surface both move when a dependency's public surface moves, with the consumer's source unchanged, so a registry publishes a surface together with the imported subject/ABI pairs and elaborator revision it was built against; an addition is potentially breaking, the break belongs to a consumer's closure rather than to the edited module, and a publisher's own check cannot detect it
kind: decision
status: proposed
created: 2026-09-06
updated: 2026-09-06
tags:
  - modules
  - registries
  - compatibility
  - identity
relations:
  informed_by:
    - planning-modules-distribution
    - planning-modules-registry
    - research-module-distribution
  depends_on:
    - decision-0015-interface-rule-selection
    - decision-0047-the-declaration-table
    - decision-0061-the-declaration-artifact
    - decision-0067-local-module-workspaces
  supersedes: []
---

# a published module surface is a claim about one dependency context, and an added provider is not a minor change

> settles two assumptions the module-system proposal carried into M-14: that a registry can hold one ABI
> per released version, and that an added declaration or rule is a minor change. Both were written before
> 0067 put imported subject/ABI pairs inside the module ABI, and before anything measured what an addition
> does to a consumer. Both are false, and the second is false in a more useful way than its critics guessed.

## context

The proposal requires a registry's candidate metadata to carry a module's ABI, and classifies `pith diff`'s
verdicts so that "an added declaration or rule is minor". Neither claim had an executable counterexample,
and both were about to become file-format promises: the candidate encoding needs to know whether one ABI
per version is meaningful, and the publication gate needs to know what an addition means.

0067 changed the ground underneath the first claim without the proposal being updated. A module's ABI and
its interface surface now encode a sorted, deduplicated set of imported subject/ABI pairs, which makes a
module's published identity depend on what it imported, not only on what it says. The question this record
answers is how far that dependence reaches and what a registry may therefore publish.

0015 is the ground underneath the second. It refuses two rules matching one request rather than ranking
them, which is what makes an added provider dangerous at all. What nobody had established is *where* that
refusal happens, and that turns out to decide who can see the break.

## decision

### a published surface is a claim about one dependency context

Measured: with a consumer's source bytes byte-identical across two runs, a public addition to its
dependency moves the dependency's ABI, the consumer's ABI, the consumer's recorded imported ABIs, and the
consumer's *interface surface*. What moved inside that surface is exactly the dependency's ABI digest —
established by redacting the digest from both encodings and finding the remainders equal. A dependency
body edit that changes nothing public moves none of them.

So a registry field holding one ABI or one surface per released version means "under the dependency context
this release recorded". It never means "under every resolution this release's requirements admit", and it
must not be read as a context-free property of the source.

A release therefore publishes source, plus a surface **together with its context**: the imported
subject/ABI pairs and the elaborator revision it elaborated against. A consumer may reuse the published
surface only when its own resolved context matches both. Otherwise it re-elaborates the source, which is
the ordinary case and not an error. A different resolved context is not tampering, and normal local body
edits do not require republishing a surface or rewriting an immutable pin.

### an added provider is potentially breaking, and the publisher cannot see it

Measured, in two halves that fall on opposite sides of the frontend boundary.

Within one module, two rules providing one interface are refused at elaboration, `E-3012`, naming the
second provider and the first. A publisher cannot build that break, so cannot publish it.

Across modules, a closure in which two dependencies each provide one interface **elaborates cleanly** —
`check` reports no errors and produces an ABI — and the consumer fails only when it evaluates, `E-1102`.

Four consequences, and they are the record's substance:

- An addition is potentially breaking. The unconditional minor classification is withdrawn.
- The break is a property of a consumer's whole closure, not of the edited module. Neither dependency is
  wrong on its own; the pair is.
- A publisher's own `check` cannot detect it, because their module is fine. The party with the power to
  act has no signal.
- No comparison of two versions of one module can prove an addition safe, because the counterexample is
  not in either version.

### what a differ may claim, and what publication may gate on

`pith diff` reports three things and never merges them.

Its **structural report** covers the module's own declarations, computed after subtracting the imported-ABI
region — otherwise a dependency's movement is attributed to the author's edit, which the measurement above
shows would happen for byte-identical source. Its **context report** names a change in imported
subject/ABI pairs or elaborator revision as its own fact. Its **consumer report** is optional evidence that
one named closure still selects and typechecks, and its claim is limited to that closure.

Publication gates on a conservative classification. Removals, nominal representation moves, constructor-set
changes, and interface changes are breaking. Additions are potentially breaking and require the same bump
discipline rather than being waved through. Formatting, declaration order, and alias edits are not changes
at all, which 0067 already guaranteed by keeping them out of every digest. An unchanged structural report
is not a compatibility proof, and the gate must not describe it as one.

## alternatives considered

**One context-free ABI per released version**, as the proposal assumed. Rejected as measured false. Keeping
it would mean a registry field whose meaning silently varies with the consumer's resolution, which is the
worst of the available outcomes because it looks authoritative.

**Publish source only, no surface.** Coherent, simpler, and it removes the context question entirely. It
gives up the cross-module reuse the frontend graph tier was built for: the whole point of a canonicalized
interface artifact is that a consumer can skip re-elaborating a dependency. Retained instead as a cache
entry carrying its context, which is reusable exactly when the context matches and inert otherwise.

**Keep additions minor and let consumers pin.** Rejected. The measurement shows the party who could act —
the publisher — has no signal, so the policy would push a cost onto consumers who cannot anticipate it
either. Conservative classification is the only honest default when the break is invisible to both sides
until evaluation.

**Refuse a two-provider closure at elaboration, moving `E-1102` to check time.** Attractive, because it
would give the consumer the error at the boundary where they are already reading diagnostics. Not decided
here: it requires the frontend to know the full closure's rule table, which is a larger change to the graph
tier than this record's evidence justifies. Named as unresolved below.

## evidence

Four tests, run on 2026-09-06. They are measurements first and regressions second; each names the claim it
settles.

| test | shows |
| --- | --- |
| `pith-loader` `probes::an_unchanged_consumer_gets_a_new_abi_when_its_dependency_surface_moves` | with the consumer's source asserted byte-identical, a dependency's public addition moves the dependency ABI, the consumer ABI, the consumer's imported ABIs, and the consumer's interface surface; redacting the dependency ABI digest makes the two surface encodings equal |
| `pith-loader` `probes::a_dependency_body_edit_leaves_the_consumer_abi_alone` | the converse, so the first result is context dependence and not sensitivity to any dependency edit |
| `pith-cli` `end_to_end::two_rules_providing_one_interface_in_one_module_are_refused_at_elaboration` | the intra-module break is `E-3012` at elaboration, so a publisher cannot build it |
| `pith-cli` `end_to_end::a_rule_added_to_one_dependency_breaks_a_consumer_that_imports_another` | the cross-module break elaborates cleanly, `check` passes, and the consumer fails at evaluation with `E-1102` |

What the evidence does not cover, stated so the record is not read past its scope. Every fixture is path
dependencies inside one workspace: no registry, no versions, no solver, and no acquired content. The
cross-module fixture has exactly two providers, one interface, and one consumer. The redaction argument
shows the surface difference is confined to the dependency's ABI digest *for these encodings*, not that
every future surface encoding will isolate its imported region as cleanly. The selection probe runs against
cold engine state deliberately, so it measures selection and says nothing about revalidation.

## unresolved

Whether a consumer's elaboration should refuse a closure carrying two providers of one interface, moving
`E-1102` to check time. It would put the error where the affected party is looking, at the cost of the
frontend needing the closure's rule table.

Whether the rule table participates in an entry computation's key. While the selection probe was being
built, a run made after a dependency gained a competing provider returned a hydrated answer rather than
re-selecting. The probe was given cold state so it would measure one thing, and this observation was
deliberately not turned into a test asserting current behavior, because it is not yet known to be correct.
It belongs to the revalidation witness the local-workspace slice already owes.

How a published surface's context is encoded in a registry entry, and whether the elaborator revision
belongs beside the imported pairs or in the entry's own metadata. This record fixes the meaning; the
encoding waits for the entry format.

The third planned probe — withdraw an admitted, locked selection and replay the lock — remains unrunnable
until registry machinery exists. It is the obligation that decides whether a lock replays a selection
without replaying a judgment about it.
